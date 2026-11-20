//! Errors and warnings, and the rule that keeps input text safe to print.

use std::fmt;

/// Why a read or a write failed.
///
/// Every error says where it happened ([`path`](Error::path) and
/// [`offset`](Error::offset)), what the reader expected and what it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The class of failure.
    pub kind: ErrorKind,
    /// Element path from the root, local names joined by `/`.
    pub path: String,
    /// Byte offset in the input at which the failure was noticed; when writing, the number of
    /// bytes written before it.
    pub offset: u64,
    /// What was expected at this place.
    pub expected: String,
    /// What it found instead. Input text is shortened and control characters are escaped.
    pub found: String,
}

/// The class of an [`Error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The input is not well-formed XML, or it ended early.
    Syntax,
    /// The document is XML but not DATEX II of the expected major version.
    NotDatex2,
    /// The payload names a publication type that is not known, or names none and none can be inferred.
    UnknownPublication,
    /// The publication cannot hold records of the type a stream was asked for.
    UnexpectedPublication,
    /// Writing: content the schema requires is absent.
    MissingRequired,
    /// Writing: a list has fewer or more entries than the schema allows.
    InvalidCount,
    /// Writing: a value breaks a length, range, digit or pattern rule of the schema, or holds a
    /// character XML cannot carry.
    InvalidValue,
    /// Writing: extension content is not well-formed, or holds a document type declaration or a
    /// custom entity.
    InvalidExtension,
    /// One of the [`Limits`](crate::Limits) was exceeded.
    LimitExceeded,
    /// The document has a document type declaration or refers to an entity it would have to define.
    DoctypeNotAllowed,
    /// The document declares an encoding other than UTF-8.
    UnsupportedEncoding,
    /// The underlying reader failed.
    Io,
}

impl ErrorKind {
    fn text(self) -> &'static str {
        match self {
            Self::Syntax => "malformed XML",
            Self::NotDatex2 => "not a DATEX II document of this version",
            Self::UnknownPublication => "unknown publication type",
            Self::UnexpectedPublication => "publication without such records",
            Self::MissingRequired => "required content missing",
            Self::InvalidCount => "wrong number of elements",
            Self::InvalidValue => "value not allowed by the schema",
            Self::InvalidExtension => "extension content not allowed",
            Self::LimitExceeded => "limit exceeded",
            Self::DoctypeNotAllowed => {
                "document type declarations and custom entities are not allowed"
            }
            Self::UnsupportedEncoding => "unsupported encoding",
            Self::Io => "read error",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: expected {}, found {}",
            self.kind.text(),
            self.expected,
            self.found
        )?;
        write_place(f, &self.path, self.offset)
    }
}

impl std::error::Error for Error {}

/// A deviation the reader tolerated.
///
/// The read succeeded; the warning says what was unusual and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The class of deviation.
    pub kind: WarningKind,
    /// Element path from the root, local names joined by `/`.
    pub path: String,
    /// Byte offset in the input at which the deviation was noticed.
    pub offset: u64,
    /// A short detail, for example the name that was not recognised.
    pub detail: String,
}

/// The class of a [`Warning`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum WarningKind {
    /// A `http://levelC/schema/3/…` namespace was read as the standard namespace. Reported once per namespace.
    LevelCNamespace,
    /// A known element was in a different DATEX II namespace than the schema declares.
    WrongNamespace,
    /// The payload had no `xsi:type`; the publication type was inferred from its content.
    MissingXsiType,
    /// An element the model does not know was skipped.
    UnknownElement,
    /// An attribute the model does not know was skipped.
    UnknownAttribute,
    /// An object named a type the model does not know, or named none where one is needed; it was skipped.
    UnknownType,
    /// An object named a type that is not allowed at this place; the declared type was used.
    UnexpectedType,
    /// An enumeration value outside the standard list was kept as an extended value.
    UnknownEnumValue,
    /// A value could not be read as its type (text where a number belongs, for example); the field is empty.
    InvalidValue,
    /// A JSON value of another type than the schema's (a number written as a string, a single
    /// object where a list belongs) was accepted.
    TypeMismatch,
    /// An element that should hold a value was empty; it was treated as absent.
    EmptyValue,
    /// A timestamp had no time zone; it was kept as written.
    NoTimeZone,
    /// An element the schema requires was absent; the field is empty.
    MissingElement,
    /// An attribute the schema requires was absent; the field is empty.
    MissingAttribute,
    /// One payload of a message container cannot hold records of the type a stream was asked for; it was skipped.
    UnexpectedPublication,
    /// An element that may occur once occurred again; the first was kept.
    Repeated,
}

impl WarningKind {
    fn text(self) -> &'static str {
        match self {
            Self::LevelCNamespace => "Level C namespace read as standard",
            Self::WrongNamespace => "element in another DATEX II namespace",
            Self::MissingXsiType => "publication type inferred",
            Self::UnknownElement => "unknown element skipped",
            Self::UnknownAttribute => "unknown attribute skipped",
            Self::UnknownType => "unknown type skipped",
            Self::UnexpectedType => "unexpected type ignored",
            Self::UnknownEnumValue => "unknown enumeration value kept as extended",
            Self::InvalidValue => "invalid value dropped",
            Self::TypeMismatch => "value of another JSON type accepted",
            Self::EmptyValue => "empty value treated as absent",
            Self::NoTimeZone => "timestamp without time zone",
            Self::MissingElement => "missing element",
            Self::MissingAttribute => "missing attribute",
            Self::UnexpectedPublication => "publication without such records skipped",
            Self::Repeated => "repeated element ignored",
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.text(), self.detail)?;
        write_place(f, &self.path, self.offset)
    }
}

fn write_place(f: &mut fmt::Formatter<'_>, path: &str, offset: u64) -> fmt::Result {
    if path.is_empty() {
        write!(f, " at document start (byte {offset})")
    } else {
        write!(f, " at {path} (byte {offset})")
    }
}

/// Longest stretch of input text quoted in an error or warning.
const QUOTE_MAX: usize = 80;

/// Makes input text safe to put on one line of a log: at most [`QUOTE_MAX`] characters, control
/// characters escaped, in double quotes.
pub(crate) fn quote(text: &str) -> String {
    format!("\"{}\"", clean(text, QUOTE_MAX))
}

/// A message from the XML parser. Such messages repeat pieces of the input, so they get the same
/// treatment as quoted text, with a longer bound.
pub(crate) fn parser_message(text: &str) -> String {
    clean(text, 2 * QUOTE_MAX)
}

fn clean(text: &str, max: usize) -> String {
    let mut out = String::with_capacity(text.len().min(max) + 2);
    for (n, c) in text.chars().enumerate() {
        if n == max {
            out.push('…');
            break;
        }
        if c.is_control() || c == '\u{2028}' || c == '\u{2029}' {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_escapes_and_shortens() {
        assert_eq!(quote("a\nb\u{1b}[31m"), "\"a\\u{a}b\\u{1b}[31m\"");
        let long = quote(&"x".repeat(10_000));
        assert_eq!(long.chars().count(), QUOTE_MAX + 3);
    }

    #[test]
    fn display_is_one_line() {
        let e = Error {
            kind: ErrorKind::Syntax,
            path: "payload/situation".into(),
            offset: 42,
            expected: "well-formed XML".into(),
            found: quote("abc"),
        };
        assert_eq!(
            e.to_string(),
            "malformed XML: expected well-formed XML, found \"abc\" at payload/situation (byte 42)"
        );
    }
}
