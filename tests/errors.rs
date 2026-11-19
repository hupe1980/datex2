//! A failed read says where, what was expected and what was found, on one line.

mod common;

use common::*;
use datex2::{ErrorKind, v3};

fn error(xml: &str) -> datex2::Error {
    v3::read(xml.as_bytes()).expect_err("the read should fail")
}

#[test]
fn truncated_document_names_the_open_element() {
    let xml = document(&accident(""));
    let cut = &xml[..xml.find("<sit:validity>").unwrap() + 20];
    let e = error(cut);
    assert_eq!(e.kind, ErrorKind::Syntax);
    assert_eq!(e.path, "payload/situation/situationRecord/validity");
    // The offset points at the start of the token that could not be completed.
    assert_eq!(&cut[e.offset as usize..], "<com:v");
    assert_eq!(
        e.to_string(),
        format!(
            "malformed XML: expected well-formed XML, found syntax error: tag not closed: `>` not found before end of input at payload/situation/situationRecord/validity (byte {})",
            e.offset
        )
    );
    // Cut between two elements instead: the end tag is what is missing.
    let cut = &xml[..xml.find("<sit:validity>").unwrap() + "<sit:validity>".len()];
    let e = error(cut);
    assert_eq!(
        e.to_string(),
        format!(
            "malformed XML: expected the end tag of \"validity\", found the end of the input at payload/situation/situationRecord/validity (byte {})",
            cut.len()
        )
    );
}

#[test]
fn mismatched_end_tag() {
    let e = error(&document(&accident("")).replace("</sit:validity>", "</sit:valid>"));
    assert_eq!(e.kind, ErrorKind::Syntax);
    assert!(
        e.to_string()
            .starts_with("malformed XML: expected well-formed XML, found "),
        "{e}"
    );
}

#[test]
fn version_2_document_is_named_as_such() {
    let e = error(
        r#"<d2LogicalModel xmlns="http://datex2.eu/schema/2/2_0" modelBaseVersion="2"><exchange/></d2LogicalModel>"#,
    );
    assert_eq!(e.kind, ErrorKind::NotDatex2);
    assert_eq!(
        e.to_string(),
        "not a DATEX II document of this version: expected a DATEX II v3 payload, messageContainer or publication, found element \"d2LogicalModel\" in namespace \"http://datex2.eu/schema/2/2_0\" (DATEX II version 1 or 2, which this reader does not support) at d2LogicalModel (byte 75)"
    );
}

#[test]
fn unrelated_xml() {
    let e = error("<html><body/></html>");
    assert_eq!(e.kind, ErrorKind::NotDatex2);
    assert!(e.found.contains("\"html\" in no namespace"), "{e}");
}

#[test]
fn soap_without_datex_content() {
    let e = error(
        r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><s:Fault/></s:Body></s:Envelope>"#,
    );
    assert_eq!(e.kind, ErrorKind::NotDatex2);
}

#[test]
fn empty_input_and_input_that_ends_in_the_declaration() {
    let e = error("");
    assert_eq!(
        e.to_string(),
        "malformed XML: expected a root element, found the end of the input at document start (byte 0)"
    );
    assert_eq!(error("<?xml version=\"1.0\"").kind, ErrorKind::Syntax);
    assert_eq!(error("   \n").kind, ErrorKind::Syntax);
}

#[test]
fn unknown_publication_type_on_the_payload() {
    let e = error(
        &document(&accident("")).replace("sit:SituationPublication", "sit:WeatherPublication"),
    );
    assert_eq!(e.kind, ErrorKind::UnknownPublication);
    assert_eq!(e.expected, "a DATEX II v3 publication type in xsi:type");
    assert_eq!(e.found, "\"sit:WeatherPublication\"");
}

#[test]
fn encoding_other_than_utf8() {
    let e = error("<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a/>");
    assert_eq!(e.kind, ErrorKind::UnsupportedEncoding);
    assert_eq!(e.found, "\"ISO-8859-1\"");
    let mut bytes = document(&accident("")).into_bytes();
    let at = bytes.windows(7).position(|w| w == b"EXAMPLE").unwrap();
    bytes[at] = 0xFF;
    assert_eq!(
        v3::read(&bytes[..]).unwrap_err().kind,
        ErrorKind::UnsupportedEncoding
    );
}

#[test]
fn undeclared_prefix() {
    let e = error(&document(&accident("<nope:x/>")));
    assert_eq!(e.kind, ErrorKind::Syntax);
    assert_eq!(
        (e.expected.as_str(), e.found.as_str()),
        ("a declared namespace prefix", "\"nope\"")
    );
}

/// read-v3/SR-007: text taken from the input cannot forge or flood a log line.
#[test]
fn sr007_quoted_input_is_shortened_and_has_no_control_characters() {
    let hostile = format!("line1\nline2\u{1b}[31m{}", "A".repeat(10_000));
    // In an error: the type name on the payload.
    let e = error(&document(&accident("")).replace(
        "sit:SituationPublication",
        &hostile.replace('\n', "&#10;").replace('\u{1b}', "&#27;"),
    ));
    for text in [e.to_string(), e.found.clone()] {
        assert!(!text.chars().any(char::is_control), "{text:?}");
        assert!(text.chars().count() < 400, "{}", text.len());
    }
    assert!(
        e.found.contains("\\u{a}") && e.found.contains("\\u{1b}") && e.found.ends_with("…\""),
        "{}",
        e.found
    );
    // In a warning: a value of the wrong form, and an unknown element name cannot contain such text at all.
    let doc = read(&document(&accident(&format!(
        "<sit:totalNumberOfVehiclesInvolved>{hostile}</sit:totalNumberOfVehiclesInvolved>"
    ))));
    let line = doc.warnings[0].to_string();
    assert!(!line.chars().any(char::is_control), "{line:?}");
    assert!(line.chars().count() < 400);
}

/// read-v3/SR-007, found by the fuzz target: the parser accepts control characters in element names, and
/// names end up in the path.
#[test]
fn sr007_element_names_in_the_path_are_made_safe_too() {
    let name = format!("x\u{0}y\u{85}{}", "n".repeat(500));
    let doc = read(&document(&accident(&format!("<sit:{name}/>"))));
    let w = &doc.warnings[0];
    for text in [w.to_string(), w.path.clone(), w.detail.clone()] {
        assert!(!text.chars().any(char::is_control), "{text:?}");
        assert!(text.chars().count() < 400);
    }
    assert!(
        w.path
            .starts_with("payload/situation/situationRecord/x\\u{0}y\\u{85}nnn"),
        "{}",
        w.path
    );
    // The same in an error: a truncated document whose open element has such a name.
    let xml = document(&accident(&format!("<sit:{name}>")));
    let e = error(&xml[..xml.find(&name).unwrap() + name.len() + 1]);
    assert!(!e.to_string().chars().any(char::is_control), "{e:?}");
}

/// read-v3/SR-007, found by the fuzz target: the parser's own messages repeat input.
#[test]
fn sr007_parser_messages_are_made_safe_too() {
    for xml in [
        "</\n>".to_owned(),
        format!("<a></{}\u{1b}>", "b".repeat(5000)),
        "<a \u{7}='1' \u{7}='2'/>".to_owned(),
    ] {
        let e = error(&xml);
        assert!(!e.to_string().chars().any(char::is_control), "{e:?}");
        assert!(e.to_string().chars().count() < 400, "{e}");
    }
}

#[test]
fn error_is_a_std_error_and_cheap_to_report() {
    fn takes(_: &(dyn std::error::Error + Send + Sync + 'static)) {}
    takes(&error("<a/>"));
}
