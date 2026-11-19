//! The writer core: everything about producing XML that the generated per-type writers do not
//! need to know. It escapes, keeps the element path, tracks whether a start tag is still open, and
//! holds the checks that make the writer strict.

use std::fmt::Write as _;
use std::io::Write;

use quick_xml::events::Event;
use quick_xml::reader::NsReader;

use crate::error::{Error, ErrorKind, quote};
use crate::types::{Date, DateTime, Decimal, Extension, MultilingualString, Time};

/// A value that is written as the text of an element or attribute.
pub trait Text {
    /// Appends the lexical form.
    fn put(&self, out: &mut String);
}

impl Text for String {
    fn put(&self, out: &mut String) {
        out.push_str(self);
    }
}

macro_rules! text_display {
    ($($t:ty),*) => {$(
        impl Text for $t {
            fn put(&self, out: &mut String) {
                let _ = write!(out, "{self}");
            }
        }
    )*};
}
text_display!(i32, i64, u64, bool, Decimal, DateTime, Date, Time);

macro_rules! text_float {
    ($($t:ty),*) => {$(
        impl Text for $t {
            fn put(&self, out: &mut String) {
                if self.is_nan() {
                    out.push_str("NaN");
                } else if self.is_infinite() {
                    out.push_str(if *self > 0.0 { "INF" } else { "-INF" });
                } else {
                    let _ = write!(out, "{self}");
                }
            }
        }
    )*};
}
text_float!(f32, f64);

impl Text for Vec<u8> {
    fn put(&self, out: &mut String) {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        for chunk in self.chunks(3) {
            let n = u32::from(chunk[0]) << 16
                | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
                if i <= chunk.len() {
                    out.push(ALPHABET[(n >> shift & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
    }
}

/// The patterns the schemas use. Each is small enough to check by hand; the generator refuses a
/// schema that brings a pattern not listed here.
#[derive(Debug, Clone, Copy)]
pub enum Pattern {
    /// `[1-9]|[4-6].[1-2]|4.3`
    AdrClass,
    /// Four numbers from 0 to 255 joined by dots.
    IpAddress,
    /// Three capital letters.
    CurrencyCode,
    /// `Z` or a signed `hh:mm`.
    TimeZone,
    /// At least four numbers, each pair separated by one white-space character.
    GmlPosList,
    /// `xs:language`: one to eight letters, then groups of one to eight letters or digits after hyphens.
    Language,
    /// `xs:duration`, for example `PT15M`.
    Duration,
}

/// Digits followed by each of `units` at most once and in order; `fraction` allows a decimal
/// point in the last number. Returns whether anything was read.
fn duration_part(s: &str, units: &[u8], fraction: bool) -> Option<bool> {
    let (mut rest, mut next, mut any) = (s.as_bytes(), 0, false);
    while !rest.is_empty() {
        let digits = rest
            .iter()
            .take_while(|b| b.is_ascii_digit() || (fraction && **b == b'.'))
            .count();
        let unit = *rest.get(digits)?;
        let at = units[next..].iter().position(|u| *u == unit)? + next;
        let number = &rest[..digits];
        let dots = number.iter().filter(|b| **b == b'.').count();
        if digits == 0
            || dots > usize::from(unit == b'S')
            || number.first() == Some(&b'.')
            || number.last() == Some(&b'.')
        {
            return None;
        }
        (rest, next, any) = (&rest[digits + 1..], at + 1, true);
    }
    Some(any)
}

fn is_number(s: &str) -> bool {
    let digits = |d: &str| d.bytes().all(|b| b.is_ascii_digit());
    let s = s.strip_prefix(['-', '+']).unwrap_or(s);
    match s.split_once('.') {
        Some((int, frac)) => digits(int) && !frac.is_empty() && digits(frac),
        None => !s.is_empty() && digits(s),
    }
}

impl Pattern {
    fn describe(self) -> &'static str {
        match self {
            Self::AdrClass => "an ADR class such as 3 or 4.1",
            Self::IpAddress => "an IPv4 address such as 192.0.2.1",
            Self::CurrencyCode => "three capital letters such as EUR",
            Self::TimeZone => "Z or an offset such as +01:00",
            Self::GmlPosList => "at least four numbers separated by single spaces",
            Self::Language => "a language code such as en or de-AT",
            Self::Duration => "a duration such as PT15M",
        }
    }

    fn matches(self, s: &str) -> bool {
        let b = s.as_bytes();
        match self {
            Self::AdrClass => {
                let c: Vec<char> = s.chars().collect();
                match c.as_slice() {
                    [d] => ('1'..='9').contains(d),
                    [first, _, last] => {
                        (('4'..='6').contains(first) && ('1'..='2').contains(last))
                            || (*first == '4' && *last == '3')
                    }
                    _ => false,
                }
            }
            Self::IpAddress => {
                let parts: Vec<&str> = s.split('.').collect();
                parts.len() == 4
                    && parts.iter().all(|p| {
                        (1..=3).contains(&p.len())
                            && p.bytes().all(|c| c.is_ascii_digit())
                            && (p.len() < 3 || matches!(p.as_bytes()[0], b'1' | b'2'))
                            && p.parse::<u32>().is_ok_and(|v| v <= 255)
                    })
            }
            Self::CurrencyCode => b.len() == 3 && b.iter().all(u8::is_ascii_uppercase),
            Self::TimeZone => {
                s == "Z"
                    || (b.len() == 6
                        && matches!(b[0], b'-' | b'+')
                        && b[3] == b':'
                        && [1, 2, 4, 5].iter().all(|&i| b[i].is_ascii_digit()))
            }
            Self::GmlPosList => {
                let numbers: Vec<&str> = s.split([' ', '\t', '\n', '\r']).collect();
                numbers.len() >= 4 && numbers.iter().all(|n| is_number(n))
            }
            Self::Language => {
                let mut parts = s.split('-');
                let first = parts.next().unwrap_or_default();
                (1..=8).contains(&first.len())
                    && first.bytes().all(|c| c.is_ascii_alphabetic())
                    && parts.all(|p| {
                        (1..=8).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_alphanumeric())
                    })
            }
            Self::Duration => {
                let s = s.strip_prefix('-').unwrap_or(s);
                let Some(s) = s.strip_prefix('P') else {
                    return false;
                };
                let (date, time) = match s.split_once('T') {
                    Some((date, time)) => (date, Some(time)),
                    None => (s, None),
                };
                let date = duration_part(date, b"YMD", false);
                let time = time.map(|t| duration_part(t, b"HMS", true));
                match (date, time) {
                    (Some(any), None) => any,
                    // After `T` at least one time component is required.
                    (Some(_), Some(Some(true))) => true,
                    _ => false,
                }
            }
        }
    }
}

/// The sink and state of one write.
pub struct Out<'a> {
    sink: &'a mut dyn Write,
    written: u64,
    path: String,
    marks: Vec<usize>,
    /// A start tag has been written up to its attributes and not yet closed.
    open: bool,
    scratch: String,
}

impl<'a> Out<'a> {
    pub(crate) fn new(sink: &'a mut dyn Write) -> Self {
        Self {
            sink,
            written: 0,
            path: String::new(),
            marks: Vec::new(),
            open: false,
            scratch: String::new(),
        }
    }

    fn error(
        &self,
        kind: ErrorKind,
        expected: impl Into<String>,
        found: impl Into<String>,
    ) -> Error {
        Error {
            kind,
            path: self.path.clone(),
            offset: self.written,
            expected: expected.into(),
            found: found.into(),
        }
    }

    fn put(&mut self, bytes: &str) -> Result<(), Error> {
        match self.sink.write_all(bytes.as_bytes()) {
            Ok(()) => {
                self.written += bytes.len() as u64;
                Ok(())
            }
            Err(e) => Err(self.error(
                ErrorKind::Io,
                "a sink that accepts the output",
                crate::error::parser_message(&e.to_string()),
            )),
        }
    }

    /// Writes text or an attribute value, escaped. The five markup characters become entities, and
    /// in attributes so do tab and line breaks, which a parser would otherwise turn into spaces.
    fn escaped(&mut self, value: &str, attribute: bool) -> Result<(), Error> {
        let mut out = std::mem::take(&mut self.scratch);
        out.clear();
        for c in value.chars() {
            match c {
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '&' => out.push_str("&amp;"),
                '"' if attribute => out.push_str("&quot;"),
                '\t' | '\n' if attribute => {
                    let _ = write!(out, "&#{};", c as u32);
                }
                // A parser turns a carriage return into a line feed unless it is written as a reference.
                '\r' => out.push_str("&#13;"),
                '\t' | '\n' => out.push(c),
                c if c < ' ' || c == '\u{fffe}' || c == '\u{ffff}' => {
                    self.scratch = out;
                    return Err(self.error(
                        ErrorKind::InvalidValue,
                        "characters XML 1.0 can carry",
                        quote(value),
                    ));
                }
                c => out.push(c),
            }
        }
        let result = self.put(&out);
        self.scratch = out;
        result
    }

    fn close_start(&mut self) -> Result<(), Error> {
        if self.open {
            self.open = false;
            self.put(">")?;
        }
        Ok(())
    }

    /// Starts the document.
    pub(crate) fn declaration(&mut self) -> Result<(), Error> {
        self.put("<?xml version=\"1.0\" encoding=\"UTF-8\"?>")
    }

    /// Opens an element. `tag` is a constant with its canonical prefix.
    pub fn begin(&mut self, tag: &'static str) -> Result<(), Error> {
        self.close_start()?;
        self.marks.push(self.path.len());
        if !self.path.is_empty() {
            self.path.push('/');
        }
        self.path
            .push_str(tag.split_once(':').map_or(tag, |(_, local)| local));
        self.put("<")?;
        self.put(tag)?;
        self.open = true;
        Ok(())
    }

    /// Writes constant attribute text (` name="value"`), produced by the generator.
    pub fn constant_attrs(&mut self, attrs: &'static str) -> Result<(), Error> {
        self.put(attrs)
    }

    pub fn attr(&mut self, name: &str, value: &str) -> Result<(), Error> {
        self.put(" ")?;
        self.put(name)?;
        self.put("=\"")?;
        self.escaped(value, true)?;
        self.put("\"")
    }

    pub fn attr_value(&mut self, name: &'static str, value: &impl Text) -> Result<(), Error> {
        let mut text = String::new();
        value.put(&mut text);
        self.attr(name, &text)
    }

    /// Closes the element opened last; an element without content is written as `<tag/>`.
    pub fn end(&mut self, tag: &'static str) -> Result<(), Error> {
        if self.open {
            self.open = false;
            self.put("/>")?;
        } else {
            self.put("</")?;
            self.put(tag)?;
            self.put(">")?;
        }
        if let Some(mark) = self.marks.pop() {
            self.path.truncate(mark);
        }
        Ok(())
    }

    /// Writes `<tag>value</tag>`.
    pub fn leaf(&mut self, tag: &'static str, value: &impl Text) -> Result<(), Error> {
        let mut text = String::new();
        value.put(&mut text);
        self.begin(tag)?;
        if !text.is_empty() {
            self.close_start()?;
            self.escaped(&text, false)?;
        }
        self.end(tag)
    }

    /// Writes an enumeration value: its literal, or `_extended` with the value as an attribute.
    pub fn enumeration(
        &mut self,
        tag: &'static str,
        (literal, extended): (&str, Option<&str>),
    ) -> Result<(), Error> {
        self.begin(tag)?;
        if let Some(value) = extended {
            self.attr("_extendedValue", value)?;
        }
        self.close_start()?;
        self.escaped(literal, false)?;
        self.end(tag)
    }

    pub fn multilingual(
        &mut self,
        tag: &'static str,
        value: &MultilingualString,
    ) -> Result<(), Error> {
        self.begin(tag)?;
        self.begin("com:values")?;
        for entry in &value.values {
            self.begin("com:value")?;
            self.pattern("lang", &entry.lang, Pattern::Language)?;
            self.attr("lang", &entry.lang)?;
            if entry.text.chars().count() > 1024 {
                return Err(self.error(
                    ErrorKind::InvalidValue,
                    "a text of at most 1024 characters",
                    quote(&entry.text),
                ));
            }
            if !entry.text.is_empty() {
                self.close_start()?;
                self.escaped(&entry.text, false)?;
            }
            self.end("com:value")?;
        }
        self.end("com:values")?;
        self.end(tag)
    }

    // ---- the rules -----------------------------------------------------------------------------

    /// The error for a required element that is absent.
    pub fn missing(&self, tag: &'static str) -> Error {
        self.error(
            ErrorKind::MissingRequired,
            format!("required element {}", quote(tag)),
            "none",
        )
    }

    /// The error for a required attribute that is absent.
    pub fn missing_attr(&self, name: &'static str) -> Error {
        self.error(
            ErrorKind::MissingRequired,
            format!("required attribute {}", quote(name)),
            "none",
        )
    }

    pub fn count(
        &self,
        tag: &'static str,
        len: usize,
        min: usize,
        max: usize,
    ) -> Result<(), Error> {
        if (min..=max).contains(&len) {
            return Ok(());
        }
        let bounds = if max == usize::MAX {
            format!("at least {min}")
        } else {
            format!("{min} to {max}")
        };
        Err(self.error(
            ErrorKind::InvalidCount,
            format!("{bounds} of element {}", quote(tag)),
            len.to_string(),
        ))
    }

    pub fn max_len(&self, name: &'static str, value: &str, max: usize) -> Result<(), Error> {
        if value.chars().count() <= max {
            return Ok(());
        }
        Err(self.error(
            ErrorKind::InvalidValue,
            format!("at most {max} characters in {}", quote(name)),
            quote(value),
        ))
    }

    pub fn range(&self, name: &'static str, value: u64, min: u64, max: u64) -> Result<(), Error> {
        if (min..=max).contains(&value) {
            return Ok(());
        }
        let bounds = if max == u64::MAX {
            format!("at least {min}")
        } else {
            format!("{min} to {max}")
        };
        Err(self.error(
            ErrorKind::InvalidValue,
            format!("{bounds} in {}", quote(name)),
            value.to_string(),
        ))
    }

    pub fn digits(
        &self,
        name: &'static str,
        value: &Decimal,
        total: usize,
        fraction: usize,
    ) -> Result<(), Error> {
        let text = value.as_str().trim_start_matches(['+', '-']);
        let (int, frac) = text.split_once('.').unwrap_or((text, ""));
        let (int, frac) = (int.trim_start_matches('0'), frac.trim_end_matches('0'));
        if int.len() + frac.len() <= total && frac.len() <= fraction {
            return Ok(());
        }
        Err(self.error(
            ErrorKind::InvalidValue,
            format!(
                "at most {total} digits, {fraction} of them after the point, in {}",
                quote(name)
            ),
            quote(value.as_str()),
        ))
    }

    pub fn pattern(&self, name: &'static str, value: &str, pattern: Pattern) -> Result<(), Error> {
        if pattern.matches(value) {
            return Ok(());
        }
        Err(self.error(
            ErrorKind::InvalidValue,
            format!("{} in {}", pattern.describe(), quote(name)),
            quote(value),
        ))
    }

    // ---- extension content -----------------------------------------------------------------------

    /// Checks that `xml`, placed inside an element that declares `declarations`, is well-formed,
    /// stays inside that element, and brings no document type declaration or custom entity.
    fn check_raw(&self, xml: &str, declarations: &str) -> Result<(), Error> {
        let refuse = |why: String| {
            self.error(
                ErrorKind::InvalidExtension,
                "well-formed extension content without a document type declaration",
                why,
            )
        };
        let wrapped = format!("<x{declarations}>{xml}</x>");
        let mut reader = NsReader::from_str(&wrapped);
        reader.config_mut().check_end_names = true;
        let (mut depth, mut closed) = (0usize, false);
        loop {
            match reader.read_event() {
                Err(e) => return Err(refuse(crate::error::parser_message(&e.to_string()))),
                Ok(Event::Eof) => break,
                Ok(_) if closed => {
                    return Err(refuse(
                        "content after the end of the extension element".to_owned(),
                    ));
                }
                Ok(Event::Start(start)) => {
                    if let (quick_xml::name::ResolveResult::Unknown(prefix), _) =
                        reader.resolver().resolve_element(start.name())
                    {
                        return Err(refuse(format!("the undeclared prefix {}", quote(&prefix))));
                    }
                    depth += 1;
                }
                Ok(Event::Empty(start)) => {
                    if let (quick_xml::name::ResolveResult::Unknown(prefix), _) =
                        reader.resolver().resolve_element(start.name())
                    {
                        return Err(refuse(format!("the undeclared prefix {}", quote(&prefix))));
                    }
                }
                Ok(Event::End(_)) => {
                    depth = depth.saturating_sub(1);
                    closed = depth == 0;
                }
                Ok(Event::DocType(_)) => {
                    return Err(refuse("a document type declaration".to_owned()));
                }
                // An extension element holds elements only; the schema allows no text beside them.
                Ok(Event::Text(t)) if depth == 1 && !t.trim().is_empty() => {
                    return Err(refuse("text outside an element".to_owned()));
                }
                Ok(Event::CData(_)) | Ok(Event::GeneralRef(_)) if depth == 1 => {
                    return Err(refuse("text outside an element".to_owned()));
                }
                Ok(Event::GeneralRef(r)) => {
                    if !r.is_char_ref() && !matches!(&*r, "amp" | "lt" | "gt" | "apos" | "quot") {
                        return Err(refuse(format!(
                            "the entity reference {}",
                            quote(&format!("&{};", &*r))
                        )));
                    }
                }
                Ok(Event::Decl(_)) | Ok(Event::PI(_)) => {
                    return Err(refuse(
                        "an XML declaration or processing instruction".to_owned(),
                    ));
                }
                Ok(_) => {}
            }
        }
        if closed {
            Ok(())
        } else {
            Err(refuse("an element that is not closed".to_owned()))
        }
    }

    /// Writes an extension slot: the slot element, the bindings its content needs, the content.
    pub fn extension(&mut self, tag: &'static str, ext: &Extension) -> Result<(), Error> {
        self.begin(tag)?;
        let declarations = ext.declarations(|_| false);
        self.check_raw(&ext.xml, &declarations)?;
        self.put(&declarations)?;
        if !ext.xml.is_empty() {
            self.close_start()?;
            self.put(&ext.xml)?;
        }
        self.end(tag)
    }

    /// Writes an element from another namespace that was kept whole, with its own declarations.
    pub fn foreign_element(&mut self, ext: &Extension) -> Result<(), Error> {
        self.close_start()?;
        self.check_raw(&ext.xml, &ext.declarations(|_| false))?;
        if !ext.xml.starts_with('<') || !ext.namespaces.is_empty() {
            return Err(self.error(
                ErrorKind::InvalidExtension,
                "one element that declares its own namespaces",
                quote(&ext.xml),
            ));
        }
        self.put(&ext.xml)
    }
}

#[cfg(test)]
mod tests {
    use super::Pattern as P;
    use super::*;

    #[test]
    fn digit_rule_counts_significant_digits() {
        let mut sink = Vec::new();
        let out = Out::new(&mut sink);
        for (text, ok) in [
            ("12.50", true),
            ("123456.78", true),
            ("0.5", true),
            ("00012.500", true),
            ("1234567.89", false),
            ("1.234", false),
            ("-999999.99", true),
        ] {
            let value: Decimal = text.parse().unwrap();
            assert_eq!(out.digits("price", &value, 8, 2).is_ok(), ok, "{text}");
        }
        let e = out
            .digits("price", &"1.234".parse().unwrap(), 8, 2)
            .unwrap_err();
        assert_eq!(
            e.expected,
            "at most 8 digits, 2 of them after the point, in \"price\""
        );
    }

    #[test]
    fn base64_is_written_with_padding() {
        for (bytes, text) in [
            (&b""[..], ""),
            (b"h", "aA=="),
            (b"he", "aGU="),
            (b"hel", "aGVs"),
            (b"hello", "aGVsbG8="),
        ] {
            let mut out = String::new();
            bytes.to_vec().put(&mut out);
            assert_eq!(out, text);
            assert_eq!(crate::types::base64(text).as_deref(), Some(bytes));
        }
    }

    #[test]
    fn patterns_match_what_the_schema_patterns_match() {
        for (pattern, good, bad) in [
            (
                P::AdrClass,
                &["1", "9", "4.1", "6.2", "4.3", "5x1"][..],
                &["", "0", "10", "4.4", "3.1", "4.12"][..],
            ),
            (
                P::IpAddress,
                &["192.0.2.1", "0.0.0.0", "255.255.255.255", "09.1.1.1"],
                &[
                    "256.1.1.1",
                    "1.1.1",
                    "1.1.1.1.1",
                    "099.1.1.1",
                    "a.b.c.d",
                    "1..1.1",
                ],
            ),
            (
                P::CurrencyCode,
                &["EUR", "SEK"],
                &["eur", "EU", "EURO", "E1R"],
            ),
            (
                P::TimeZone,
                &["Z", "+01:00", "-05:30"],
                &["z", "01:00", "+1:00", "+01:0", "+0100"],
            ),
            (
                P::GmlPosList,
                &["1 2 3 4", "52.1 4.3 52.2 4.4 52.3 4.5", "-1 +2 .5 4"],
                &["1 2 3", "1  2 3 4", "1 2 3 x", "1 2 3 4.", ""],
            ),
            (
                P::Language,
                &["en", "de-AT", "deu", "en-us", "x-private1"],
                &["", "toolonglanguage", "en_US", "en-", "1n", "en us"],
            ),
            (
                P::Duration,
                &["PT15M", "P1D", "P1Y2M3DT4H5M6.5S", "-P1D", "PT0S"],
                &[
                    "", "P", "PT", "15M", "P1S", "PT1.5M", "P1DT", "P1M1Y", "PT.5S",
                ],
            ),
        ] {
            for text in good {
                assert!(pattern.matches(text), "{pattern:?} should accept {text:?}");
            }
            for text in bad {
                assert!(!pattern.matches(text), "{pattern:?} should refuse {text:?}");
            }
        }
    }
}
