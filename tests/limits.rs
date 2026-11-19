//! Untrusted input cannot make the reader do unbounded work or reach outside.
//!
//! Comments such as `read-v3/SR-001` name the security requirement a test is evidence for.

mod common;

use common::*;
use datex2::{ErrorKind, Limits, v3};

fn nested(levels: usize) -> String {
    // Unknown elements inside a record, `levels` deep below it.
    let open: String = (0..levels).map(|_| "<sit:x>").collect();
    let close: String = (0..levels).map(|_| "</sit:x>").collect();
    document(&accident(&format!("{open}{close}")))
}

/// read-v3/SR-001
#[test]
fn sr001_depth_limit_names_itself_and_can_be_raised() {
    // payload/situation/situationRecord are three levels; the limit counts all of them.
    assert!(v3::read(nested(97).as_bytes()).is_ok());
    let e = v3::read(nested(98).as_bytes()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::LimitExceeded);
    assert_eq!(e.expected, "nesting of at most 100 elements (max_depth)");
    let mut limits = Limits::default();
    limits.max_depth = 200;
    assert!(v3::read_with(nested(98).as_bytes(), &limits).is_ok());
    limits.max_depth = 2;
    assert_eq!(
        v3::read_with(FIXTURE.as_bytes(), &limits)
            .unwrap_err()
            .expected,
        "nesting of at most 2 elements (max_depth)"
    );
}

/// read-v3/SR-001: the limit also holds inside content that is kept raw.
#[test]
fn sr001_depth_limit_applies_inside_extension_content() {
    let open: String = (0..200).map(|_| "<x>").collect();
    let close: String = (0..200).map(|_| "</x>").collect();
    let xml = document(&accident(&format!(
        "<sit:_situationRecordExtension>{open}{close}</sit:_situationRecordExtension>"
    )));
    assert_eq!(
        v3::read(xml.as_bytes()).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

/// read-v3/SR-001
#[test]
fn sr001_attribute_limit_names_itself_and_can_be_raised() {
    let attrs: String = (0..300).map(|i| format!(" a{i}=\"1\"")).collect();
    let xml = document(&accident("")).replace("version=\"1\"", &format!("version=\"1\"{attrs}"));
    let e = v3::read(xml.as_bytes()).unwrap_err();
    assert_eq!(
        (e.kind, e.expected.as_str()),
        (
            ErrorKind::LimitExceeded,
            "at most 256 attributes on an element (max_attributes)"
        )
    );
    let mut limits = Limits::default();
    limits.max_attributes = 400;
    limits.max_warnings = 10;
    let doc = v3::read_with(xml.as_bytes(), &limits).unwrap();
    assert_eq!(doc.warnings.len() + doc.suppressed_warnings, 300);
}

/// read-v3/SR-001
#[test]
fn sr001_text_limit_names_itself_and_can_be_raised() {
    let mut limits = Limits::default();
    limits.max_text_len = 1000;
    let long = "x".repeat(1001);
    let in_text = document(&accident("")).replace("EXAMPLE", &long);
    let e = v3::read_with(in_text.as_bytes(), &limits).unwrap_err();
    assert_eq!(
        (e.kind, e.expected.as_str()),
        (
            ErrorKind::LimitExceeded,
            "text of at most 1000 bytes (max_text_len)"
        )
    );
    let in_attribute = document(&accident("")).replace("id=\"R1\"", &format!("id=\"{long}\""));
    assert_eq!(
        v3::read_with(in_attribute.as_bytes(), &limits)
            .unwrap_err()
            .expected,
        "attribute values of at most 1000 bytes (max_text_len)"
    );
    let in_extension = document(&accident(&format!(
        "<sit:_situationRecordExtension><x>{long}</x></sit:_situationRecordExtension>"
    )));
    assert_eq!(
        v3::read_with(in_extension.as_bytes(), &limits)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    limits.max_text_len = 2000;
    assert!(v3::read_with(in_text.as_bytes(), &limits).is_ok());
}

/// read-v3/SR-001 and read-v3/SR-002: the parser's buffer is bounded too. A text is refused while it
/// is being read, not after all of it has been buffered.
#[test]
fn sr001_oversized_token_is_refused_before_it_is_buffered() {
    /// The start of a document followed by an endless run of letters.
    struct Endless(usize);
    impl std::io::Read for Endless {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let head = b"<d2:payload xmlns:d2=\"http://datex2.eu/schema/3/d2Payload\" xmlns:sit=\"http://datex2.eu/schema/3/situation\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:type=\"sit:SituationPublication\"><d2:x>";
            if self.0 < head.len() {
                let n = (head.len() - self.0).min(out.len());
                out[..n].copy_from_slice(&head[self.0..self.0 + n]);
                self.0 += n;
                return Ok(n);
            }
            out.fill(b'a');
            self.0 += out.len();
            Ok(out.len())
        }
    }
    let mut source = Endless(0);
    let e = v3::read(std::io::BufReader::new(&mut source)).unwrap_err();
    assert_eq!(
        (e.kind, e.expected.as_str()),
        (
            ErrorKind::LimitExceeded,
            "a tag or text of at most 8388608 bytes (max_text_len)"
        )
    );
    assert!(
        source.0 < (8 << 20) + (256 << 10),
        "stopped after {} bytes of an endless text",
        source.0
    );
    // An endless tag is stopped the same way.
    let tag = format!(
        "<d2:payload xmlns:d2=\"http://datex2.eu/schema/3/d2Payload\" a=\"{}",
        "b".repeat(9 << 20)
    );
    assert_eq!(
        v3::read(tag.as_bytes()).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

/// read-v3/SR-003
#[test]
fn sr003_document_type_declaration_is_refused_before_content() {
    let bomb = r#"<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;"><!ENTITY c "&b;&b;&b;&b;&b;&b;&b;&b;">]>"#;
    let xml = format!(
        "{bomb}{}",
        document(&accident("")).replace("EXAMPLE", "&c;")
    );
    let e = v3::read(xml.as_bytes()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::DoctypeNotAllowed);
    assert_eq!(e.path, "", "refused before any element was read");
    let external = format!(
        r#"<!DOCTYPE d2 SYSTEM "file:///etc/passwd">{}"#,
        document(&accident(""))
    );
    assert_eq!(
        v3::read(external.as_bytes()).unwrap_err().kind,
        ErrorKind::DoctypeNotAllowed
    );
}

/// read-v3/SR-003
#[test]
fn sr003_undeclared_entity_is_refused_by_name() {
    for (xml, what) in [
        (
            document(&accident("")).replace("EXAMPLE", "a&xxe;b"),
            "in text",
        ),
        (
            document(&accident("")).replace("id=\"R1\"", "id=\"&xxe;\""),
            "in an attribute",
        ),
        (
            document(&accident(
                "<sit:_situationRecordExtension>&xxe;</sit:_situationRecordExtension>",
            )),
            "in extension content",
        ),
    ] {
        let e = v3::read(xml.as_bytes()).unwrap_err();
        assert_eq!(e.kind, ErrorKind::DoctypeNotAllowed, "{what}");
        assert_eq!(e.found, "\"&xxe;\"", "{what}");
    }
    // The predefined entities and character references are plain XML.
    let doc = read(
        &document(&accident("")).replace("EXAMPLE", "A&amp;B &lt;&#x41;&#66;&gt; &quot;&apos;"),
    );
    let datex2::v3::Publication::Situation(p) = &doc.publications[0] else {
        panic!()
    };
    assert_eq!(
        p.publication_creator
            .as_ref()
            .unwrap()
            .national_identifier
            .as_deref(),
        Some("A&B <AB> \"'")
    );
}

/// read-v3/SR-004
#[test]
fn sr004_warnings_are_capped_and_the_rest_is_counted() {
    let unknown: String = (0..2500).map(|_| "<sit:x/>").collect();
    let doc = read(&document(&accident(&unknown)));
    assert_eq!((doc.warnings.len(), doc.suppressed_warnings), (1000, 1500));
    let mut limits = Limits::default();
    limits.max_warnings = 0;
    let doc = v3::read_with(document(&accident(&unknown)).as_bytes(), &limits).unwrap();
    assert_eq!((doc.warnings.len(), doc.suppressed_warnings), (0, 2500));
}

/// read-v3/SR-001: the parser's own bound on namespace declarations is reported as one of ours.
#[test]
fn sr001_namespace_declarations_in_scope_are_bounded() {
    let mut limits = Limits::default();
    limits.max_attributes = 40; // 160 declarations in scope
    limits.max_depth = 1000;
    let nested: String = (0..10).map(|i| format!("<sit:x xmlns:a{i}=\"urn:a\" xmlns:b{i}=\"urn:b\" xmlns:c{i}=\"urn:c\" xmlns:d{i}=\"urn:d\" xmlns:e{i}=\"urn:e\" xmlns:f{i}=\"urn:f\" xmlns:g{i}=\"urn:g\" xmlns:h{i}=\"urn:h\" xmlns:i{i}=\"urn:i\" xmlns:j{i}=\"urn:j\" xmlns:k{i}=\"urn:k\" xmlns:l{i}=\"urn:l\" xmlns:m{i}=\"urn:m\" xmlns:n{i}=\"urn:n\" xmlns:o{i}=\"urn:o\" xmlns:p{i}=\"urn:p\" xmlns:r{i}=\"urn:r\">")).collect();
    let e = v3::read_with(document(&accident(&nested)).as_bytes(), &limits).unwrap_err();
    assert_eq!(
        (e.kind, e.expected.as_str()),
        (
            ErrorKind::LimitExceeded,
            "at most 160 namespace declarations in scope (4 × max_attributes)"
        )
    );
    assert!(e.offset > 0);
}

/// read-v3/SR-002: work grows with the input, not faster. A coarse guard; the fuzz target is the real check.
#[test]
fn sr002_many_namespace_declarations_and_attributes_stay_linear() {
    let decls: String = (0..250)
        .map(|i| format!(" xmlns:q{i}=\"urn:q{i}\""))
        .collect();
    let many: String = (0..2000)
        .map(|_| accident("").replace("version=\"1\"", &format!("version=\"1\"{decls}")))
        .collect();
    let started = std::time::Instant::now();
    let doc = read(&document(&many));
    assert_eq!(records(&doc).len(), 2000);
    assert!(
        started.elapsed().as_secs() < 20,
        "took {:?}",
        started.elapsed()
    );
}
