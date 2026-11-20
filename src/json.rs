//! The JSON core: everything about JSON that the generated per-type readers and writers do not
//! need to know. It mirrors the XML core: generated code asks for "the next member" as an integer
//! key and reads or writes values by type; the core keeps the path and the byte offset, enforces
//! the limits, collects warnings, formats numbers exactly and applies the Mobilithek profile.

use std::fmt::Write as _;
use std::io::{BufRead, Write};

use crate::error::{Error, ErrorKind, Warning, WarningKind, parser_message, quote};
use crate::out::Rules;
use crate::types::{
    Date, DateTime, Decimal, Extension, FromText, LangString, MultilingualString, Time, base64,
};
use crate::xml::{Enumeration, Limits};

/// Key of a member name the model does not know.
pub(crate) const UNKNOWN: u32 = u32::MAX;

/// The JSON vocabulary of one major version, produced by the generator.
pub(crate) struct JsonTables {
    /// Every member name, sorted; the index is the member id.
    pub names: &'static [&'static str],
    /// Ids of the names the core itself needs.
    pub values: u32,
    pub value: u32,
    pub lang: u32,
    pub extended_value: u32,
    pub target_class: u32,
    pub header: [u32; 5],
}

impl JsonTables {
    fn id(&self, name: &str) -> u32 {
        self.names
            .binary_search(&name)
            .map_or(UNKNOWN, |i| i as u32)
    }

    fn name(&self, id: u32) -> &'static str {
        self.names.get(id as usize).copied().unwrap_or("?")
    }
}

/// What a profile allows, and where its spelling differs from DATEX II 3.7. Generated from the
/// profile's published schemas.
pub(crate) struct Profile {
    pub name: &'static str,
    pub version: &'static str,
    /// Per definition id: whether other members are allowed, the allowed members, the required
    /// members. Sorted by definition id.
    pub defs: &'static [(u32, bool, &'static [u32], &'static [u32])],
    /// Per enumeration definition id: the allowed literals, in DATEX II 3.7 spelling.
    pub literals: &'static [(u32, &'static [&'static str])],
    /// (definition, member) written under another name.
    pub renames: &'static [(u32, u32, u32)],
    /// (definition, member) whose polymorphic value is written without its wrapper; only the
    /// given variant is allowed.
    pub unwraps: &'static [(u32, u32, u32)],
    /// (enumeration definition, 3.7 literal, profile literal).
    pub literal_renames: &'static [(u32, &'static str, &'static str)],
}

/// Names a definition in messages.
pub(crate) type DefNames = &'static [&'static str];

// ---- reading ------------------------------------------------------------------------------------

enum Frame {
    Object { first: bool, member_open: bool },
    Array { first: bool },
}

/// A list being read: a JSON array, or a single value where an array belongs.
pub(crate) struct List {
    array: bool,
    done: bool,
}

/// The reader that generated code drives.
pub(crate) struct JsonReader<'a> {
    src: Box<dyn BufRead + 'a>,
    offset: u64,
    tables: &'static JsonTables,
    limits: Limits,
    frames: Vec<Frame>,
    path: String,
    marks: Vec<usize>,
    warnings: Vec<Warning>,
    suppressed: usize,
    /// The member name read last, as written.
    key: String,
    /// Raw bytes of the value being kept, while one is.
    recording: Option<Vec<u8>>,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

impl<'a> JsonReader<'a> {
    pub(crate) fn new(
        input: Box<dyn BufRead + 'a>,
        tables: &'static JsonTables,
        limits: &Limits,
    ) -> Self {
        Self {
            src: input,
            offset: 0,
            tables,
            limits: limits.clone(),
            frames: Vec::new(),
            path: String::new(),
            marks: Vec::new(),
            warnings: Vec::new(),
            suppressed: 0,
            key: String::new(),
            recording: None,
        }
    }

    pub(crate) fn take_warnings(&mut self) -> Vec<Warning> {
        std::mem::take(&mut self.warnings)
    }

    pub(crate) fn suppressed(&self) -> usize {
        self.suppressed
    }

    pub(crate) fn error(
        &self,
        kind: ErrorKind,
        expected: impl Into<String>,
        found: impl Into<String>,
    ) -> Error {
        Error {
            kind,
            path: self.path.clone(),
            offset: self.offset,
            expected: expected.into(),
            found: found.into(),
        }
    }

    pub(crate) fn warn(&mut self, kind: WarningKind, detail: String) {
        if self.warnings.len() < self.limits.max_warnings {
            self.warnings.push(Warning {
                kind,
                path: self.path.clone(),
                offset: self.offset,
                detail,
            });
        } else {
            self.suppressed += 1;
        }
    }

    fn syntax(&mut self, expected: &str) -> Error {
        let found = match self.peek() {
            Ok(Some(b)) => quote(&char::from(b).to_string()),
            _ => "the end of the input".to_owned(),
        };
        self.error(ErrorKind::Syntax, expected, found)
    }

    // -- bytes --

    fn peek(&mut self) -> Result<Option<u8>, Error> {
        match self.src.fill_buf() {
            Ok(buf) => Ok(buf.first().copied()),
            Err(e) => Err(self.error(
                ErrorKind::Io,
                "readable input",
                parser_message(&e.to_string()),
            )),
        }
    }

    fn bump(&mut self) -> Result<Option<u8>, Error> {
        let b = self.peek()?;
        if let Some(b) = b {
            self.src.consume(1);
            self.offset += 1;
            if let Some(r) = &mut self.recording {
                r.push(b);
            }
        }
        Ok(b)
    }

    fn ws(&mut self) -> Result<Option<u8>, Error> {
        loop {
            match self.peek()? {
                Some(b) if is_ws(b) => {
                    self.bump()?;
                }
                other => return Ok(other),
            }
        }
    }

    fn expect(&mut self, want: u8, what: &str) -> Result<(), Error> {
        match self.ws()? {
            Some(b) if b == want => {
                self.bump()?;
                Ok(())
            }
            Some(b) => Err(self.error(ErrorKind::Syntax, what, quote(&char::from(b).to_string()))),
            None => Err(self.error(ErrorKind::Syntax, what, "the end of the input")),
        }
    }

    fn too_long(&self, what: &str) -> Error {
        self.error(
            ErrorKind::LimitExceeded,
            format!(
                "{what} of at most {} bytes (max_text_len)",
                self.limits.max_text_len
            ),
            "a longer one",
        )
    }

    /// Reads a string after its opening quote.
    fn string_body(&mut self) -> Result<String, Error> {
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(b) = self.bump()? else {
                return Err(self.error(
                    ErrorKind::Syntax,
                    "the end of a string",
                    "the end of the input",
                ));
            };
            match b {
                b'"' => break,
                b'\\' => {
                    let Some(e) = self.bump()? else {
                        return Err(self.error(
                            ErrorKind::Syntax,
                            "an escape",
                            "the end of the input",
                        ));
                    };
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let high = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&high) {
                                if self.bump()? != Some(b'\\') || self.bump()? != Some(b'u') {
                                    return Err(self.error(
                                        ErrorKind::Syntax,
                                        "the second half of a surrogate pair",
                                        "something else",
                                    ));
                                }
                                let low = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(self.error(
                                        ErrorKind::Syntax,
                                        "the second half of a surrogate pair",
                                        format!("\\u{low:04x}"),
                                    ));
                                }
                                0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                            } else {
                                high
                            };
                            char::from_u32(code).ok_or_else(|| {
                                self.error(
                                    ErrorKind::Syntax,
                                    "a character",
                                    format!("\\u{code:04x}"),
                                )
                            })?
                        }
                        other => {
                            return Err(self.error(
                                ErrorKind::Syntax,
                                "a valid escape",
                                quote(&format!("\\{}", char::from(other))),
                            ));
                        }
                    };
                    let mut tmp = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
                }
                b if b < 0x20 => {
                    return Err(self.error(
                        ErrorKind::Syntax,
                        "a control character written as an escape",
                        format!("\\u{{{b:x}}}"),
                    ));
                }
                b => out.push(b),
            }
            if out.len() > self.limits.max_text_len {
                return Err(self.too_long("strings"));
            }
        }
        String::from_utf8(out).map_err(|_| {
            self.error(
                ErrorKind::UnsupportedEncoding,
                "UTF-8 text",
                "bytes that are not valid UTF-8",
            )
        })
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let mut v = 0;
        for _ in 0..4 {
            let b = self.bump()?.unwrap_or(0);
            let d = char::from(b).to_digit(16).ok_or_else(|| {
                self.error(
                    ErrorKind::Syntax,
                    "four hexadecimal digits",
                    quote(&char::from(b).to_string()),
                )
            })?;
            v = v * 16 + d;
        }
        Ok(v)
    }

    /// Reads a number token as written.
    fn number_body(&mut self) -> Result<String, Error> {
        let mut out = String::new();
        while let Some(b) = self.peek()? {
            if !(b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E')) {
                break;
            }
            self.bump()?;
            out.push(char::from(b));
            if out.len() > self.limits.max_text_len {
                return Err(self.too_long("numbers"));
            }
        }
        if !is_json_number(&out) {
            return Err(self.error(ErrorKind::Syntax, "a number", quote(&out)));
        }
        Ok(out)
    }

    fn literal(&mut self, word: &str) -> Result<(), Error> {
        for want in word.bytes() {
            if self.bump()? != Some(want) {
                return Err(self.error(ErrorKind::Syntax, format!("`{word}`"), "something else"));
            }
        }
        Ok(())
    }

    // -- structure --

    fn push_path(&mut self, segment: &str) {
        self.marks.push(self.path.len());
        if !self.path.is_empty() {
            self.path.push('/');
        }
        for (n, c) in segment.chars().enumerate() {
            if n == 80 {
                self.path.push('…');
                break;
            }
            if c.is_control() || c == '/' || c == '\u{2028}' || c == '\u{2029}' {
                self.path.extend(c.escape_unicode());
            } else {
                self.path.push(c);
            }
        }
    }

    fn pop_path(&mut self) {
        if let Some(mark) = self.marks.pop() {
            self.path.truncate(mark);
        }
    }

    fn enter(&mut self, frame: Frame) -> Result<(), Error> {
        if self.frames.len() >= self.limits.max_depth {
            return Err(self.error(
                ErrorKind::LimitExceeded,
                format!(
                    "nesting of at most {} objects and arrays (max_depth)",
                    self.limits.max_depth
                ),
                "a deeper one",
            ));
        }
        self.frames.push(frame);
        Ok(())
    }

    /// Starts reading an object. `false` when the value is `null` or not an object (both are
    /// reported and the value is skipped).
    pub(crate) fn begin_object(&mut self) -> Result<bool, Error> {
        match self.ws()? {
            Some(b'{') => {
                self.bump()?;
                self.enter(Frame::Object {
                    first: true,
                    member_open: false,
                })?;
                Ok(true)
            }
            Some(b'n') => {
                self.literal("null")?;
                self.warn(WarningKind::EmptyValue, "null treated as absent".to_owned());
                Ok(false)
            }
            _ => {
                self.warn(
                    WarningKind::TypeMismatch,
                    "expected an object; the value was skipped".to_owned(),
                );
                self.skip_value()?;
                Ok(false)
            }
        }
    }

    /// The next member of the open object, as a key, with the reader positioned at its value;
    /// `None` at the end of the object.
    pub(crate) fn next_key(&mut self) -> Result<Option<u32>, Error> {
        let Some(Frame::Object { first, member_open }) = self.frames.last_mut() else {
            return Err(self.error(ErrorKind::Syntax, "an object", "another value"));
        };
        let (was_first, was_open) = (*first, *member_open);
        if was_open {
            self.pop_path();
        }
        if let Some(Frame::Object { member_open, .. }) = self.frames.last_mut() {
            *member_open = false;
        }
        match self.ws()? {
            Some(b'}') => {
                self.bump()?;
                self.frames.pop();
                return Ok(None);
            }
            Some(b',') if !was_first => {
                self.bump()?;
            }
            Some(_) if was_first => {}
            Some(b) => {
                return Err(self.error(
                    ErrorKind::Syntax,
                    "`,` or `}`",
                    quote(&char::from(b).to_string()),
                ));
            }
            None => {
                return Err(self.error(ErrorKind::Syntax, "`,` or `}`", "the end of the input"));
            }
        }
        self.expect(b'"', "a member name")?;
        let key = self.string_body()?;
        self.expect(b':', "`:`")?;
        if let Some(Frame::Object { first, member_open }) = self.frames.last_mut() {
            *first = false;
            *member_open = true;
        }
        self.push_path(&key);
        let id = self.tables.id(&key);
        self.key = key;
        Ok(Some(id))
    }

    /// `true` the first time a field is filled in an object; after that the member is reported
    /// and skipped, so that a later duplicate cannot replace the first value unseen.
    pub(crate) fn first(&mut self, seen: &mut bool) -> Result<bool, Error> {
        if !*seen {
            *seen = true;
            return Ok(true);
        }
        let detail = quote(&self.key);
        self.warn(WarningKind::Repeated, detail);
        self.skip_value()?;
        Ok(false)
    }

    pub(crate) fn list(&mut self) -> Result<List, Error> {
        match self.ws()? {
            Some(b'[') => {
                self.bump()?;
                self.enter(Frame::Array { first: true })?;
                Ok(List {
                    array: true,
                    done: false,
                })
            }
            Some(b'n') => {
                self.literal("null")?;
                self.warn(
                    WarningKind::EmptyValue,
                    "null treated as an empty list".to_owned(),
                );
                Ok(List {
                    array: false,
                    done: true,
                })
            }
            _ => {
                self.warn(
                    WarningKind::TypeMismatch,
                    "expected an array; read as a list of one".to_owned(),
                );
                Ok(List {
                    array: false,
                    done: false,
                })
            }
        }
    }

    /// Like [`list`](Self::list), for a place where one value and an array of them are both
    /// standard (the root `payload`).
    pub(crate) fn list_quietly(&mut self) -> Result<List, Error> {
        if self.ws()? == Some(b'[') {
            return self.list();
        }
        Ok(List {
            array: false,
            done: false,
        })
    }

    /// Positions the reader at the next item of a list; `false` at its end.
    pub(crate) fn item(&mut self, list: &mut List) -> Result<bool, Error> {
        if !list.array {
            let more = !list.done;
            list.done = true;
            return Ok(more);
        }
        let Some(Frame::Array { first }) = self.frames.last_mut() else {
            return Err(self.error(ErrorKind::Syntax, "an array", "another value"));
        };
        let was_first = *first;
        *first = false;
        match self.ws()? {
            Some(b']') => {
                self.bump()?;
                self.frames.pop();
                Ok(false)
            }
            Some(b',') if !was_first => {
                self.bump()?;
                Ok(true)
            }
            Some(_) if was_first => Ok(true),
            Some(b) => Err(self.error(
                ErrorKind::Syntax,
                "`,` or `]`",
                quote(&char::from(b).to_string()),
            )),
            None => Err(self.error(ErrorKind::Syntax, "`,` or `]`", "the end of the input")),
        }
    }

    /// Skips one value, whatever it is, within the limits.
    pub(crate) fn skip_value(&mut self) -> Result<(), Error> {
        let base = self.frames.len();
        loop {
            match self.ws()? {
                None => {
                    return Err(self.error(ErrorKind::Syntax, "a value", "the end of the input"));
                }
                Some(b'{') => {
                    self.bump()?;
                    self.enter(Frame::Object {
                        first: true,
                        member_open: false,
                    })?;
                }
                Some(b'[') => {
                    self.bump()?;
                    self.enter(Frame::Array { first: true })?;
                }
                Some(b'"') => {
                    self.bump()?;
                    self.string_body()?;
                }
                Some(b't') => self.literal("true")?,
                Some(b'f') => self.literal("false")?,
                Some(b'n') => self.literal("null")?,
                Some(b) if b == b'-' || b.is_ascii_digit() => {
                    self.number_body()?;
                }
                Some(b) => {
                    return Err(self.error(
                        ErrorKind::Syntax,
                        "a value",
                        quote(&char::from(b).to_string()),
                    ));
                }
            }
            // After a value: close containers, or step to the next member or item.
            loop {
                if self.frames.len() == base {
                    return Ok(());
                }
                match self.frames.last_mut() {
                    Some(Frame::Object { first, .. }) => {
                        let was_first = *first;
                        match self.ws()? {
                            Some(b'}') => {
                                self.bump()?;
                                self.frames.pop();
                                continue;
                            }
                            Some(b',') if !was_first => {
                                self.bump()?;
                            }
                            Some(b'"') if was_first => {}
                            _ => return Err(self.syntax("`,` or `}`")),
                        }
                        self.expect(b'"', "a member name")?;
                        self.string_body()?;
                        self.expect(b':', "`:`")?;
                        if let Some(Frame::Object { first, .. }) = self.frames.last_mut() {
                            *first = false;
                        }
                        break;
                    }
                    Some(Frame::Array { first }) => {
                        let was_first = *first;
                        *first = false;
                        match self.ws()? {
                            Some(b']') => {
                                self.bump()?;
                                self.frames.pop();
                                continue;
                            }
                            Some(b',') if !was_first => {
                                self.bump()?;
                            }
                            Some(_) if was_first => {}
                            _ => return Err(self.syntax("`,` or `]`")),
                        }
                        break;
                    }
                    None => return Ok(()),
                }
            }
        }
    }

    /// Skips the value of a member the model does not know, and reports it.
    pub(crate) fn skip_unknown(&mut self) -> Result<(), Error> {
        let detail = format!("member {}", quote(&self.key));
        self.warn(WarningKind::UnknownElement, detail);
        self.skip_value()
    }

    /// Skips the value of a member that is known and deliberately not interpreted.
    pub(crate) fn skip_silently(&mut self) -> Result<(), Error> {
        self.skip_value()
    }

    /// Reports a field the schema requires when the object had no member for it.
    pub(crate) fn required<T>(&mut self, slot: &Option<T>, name: u32) {
        if slot.is_none() {
            let detail = format!("required member {}", quote(self.tables.name(name)));
            self.warn(WarningKind::MissingElement, detail);
        }
    }

    // -- values --

    /// A string value. Numbers and booleans are accepted as their text, with a warning.
    pub(crate) fn string(&mut self) -> Result<Option<String>, Error> {
        match self.ws()? {
            Some(b'"') => {
                self.bump()?;
                Ok(Some(self.string_body()?))
            }
            Some(b'n') => {
                self.literal("null")?;
                self.warn(WarningKind::EmptyValue, "null treated as absent".to_owned());
                Ok(None)
            }
            Some(b) if b == b'-' || b.is_ascii_digit() => {
                let text = self.number_body()?;
                self.warn(
                    WarningKind::TypeMismatch,
                    format!("expected a string, found the number {text}"),
                );
                Ok(Some(text))
            }
            Some(b't') | Some(b'f') => {
                let text = if self.peek()? == Some(b't') {
                    "true"
                } else {
                    "false"
                };
                self.literal(text)?;
                self.warn(
                    WarningKind::TypeMismatch,
                    format!("expected a string, found {text}"),
                );
                Ok(Some(text.to_owned()))
            }
            _ => {
                self.warn(
                    WarningKind::TypeMismatch,
                    "expected a string; the value was skipped".to_owned(),
                );
                self.skip_value()?;
                Ok(None)
            }
        }
    }

    /// A value written as a JSON string and parsed: dates, times, durations.
    pub(crate) fn text<T: FromText>(&mut self) -> Result<Option<T>, Error> {
        let Some(text) = self.string()? else {
            return Ok(None);
        };
        let value = T::from_text(text.trim());
        if value.is_none() {
            self.warn(
                WarningKind::InvalidValue,
                format!("expected {}, found {}", T::EXPECTED, quote(&text)),
            );
        }
        Ok(value)
    }

    pub(crate) fn date_time(&mut self) -> Result<Option<DateTime>, Error> {
        let value = self.text::<DateTime>()?;
        if let Some(DateTime {
            offset_minutes: None,
            ..
        }) = value
        {
            let detail = value.map(|v| quote(&v.to_string())).unwrap_or_default();
            self.warn(WarningKind::NoTimeZone, detail);
        }
        Ok(value)
    }

    pub(crate) fn base64(&mut self) -> Result<Option<Vec<u8>>, Error> {
        let Some(text) = self.string()? else {
            return Ok(None);
        };
        let value = base64(&text);
        if value.is_none() {
            self.warn(
                WarningKind::InvalidValue,
                format!("expected base64 text, found {}", quote(&text)),
            );
        }
        Ok(value)
    }

    /// A number, kept as its text until the type parses it, so no digit is lost on the way.
    pub(crate) fn number<T: FromText>(&mut self) -> Result<Option<T>, Error> {
        let text = match self.ws()? {
            Some(b) if b == b'-' || b.is_ascii_digit() => self.number_body()?,
            Some(b'"') => {
                self.bump()?;
                let text = self.string_body()?;
                self.warn(
                    WarningKind::TypeMismatch,
                    format!("expected a number, found the string {}", quote(&text)),
                );
                text
            }
            Some(b'n') => {
                self.literal("null")?;
                self.warn(WarningKind::EmptyValue, "null treated as absent".to_owned());
                return Ok(None);
            }
            _ => {
                self.warn(
                    WarningKind::TypeMismatch,
                    "expected a number; the value was skipped".to_owned(),
                );
                self.skip_value()?;
                return Ok(None);
            }
        };
        if let Some(v) = T::from_text(text.trim()) {
            return Ok(Some(v));
        }
        // A number with an exponent, such as 1.5E+2, is the same value in plain digits.
        if let Some(v) = without_exponent(&text).and_then(|plain| T::from_text(&plain)) {
            return Ok(Some(v));
        }
        // An integer written with a zero fraction, such as 22000.0.
        if let Some((int, frac)) = text.split_once('.') {
            if !frac.is_empty() && frac.bytes().all(|b| b == b'0') {
                if let Some(v) = T::from_text(int) {
                    self.warn(
                        WarningKind::TypeMismatch,
                        format!("expected an integer, found {text}"),
                    );
                    return Ok(Some(v));
                }
            }
        }
        self.warn(
            WarningKind::InvalidValue,
            format!("expected {}, found {}", T::EXPECTED, quote(&text)),
        );
        Ok(None)
    }

    pub(crate) fn boolean(&mut self) -> Result<Option<bool>, Error> {
        match self.ws()? {
            Some(b't') => self.literal("true").map(|()| Some(true)),
            Some(b'f') => self.literal("false").map(|()| Some(false)),
            _ => {
                let Some(text) = self.string()? else {
                    return Ok(None);
                };
                let value = bool::from_text(text.trim());
                if value.is_some() {
                    self.warn(
                        WarningKind::TypeMismatch,
                        format!("expected true or false, found the string {}", quote(&text)),
                    );
                } else {
                    self.warn(
                        WarningKind::InvalidValue,
                        format!("expected true or false, found {}", quote(&text)),
                    );
                }
                Ok(value)
            }
        }
    }

    fn enum_from<E: Enumeration>(&mut self, literal: &str, extended: Option<String>) -> E {
        if literal == "extendedG" {
            if extended.is_none() {
                self.warn(
                    WarningKind::MissingAttribute,
                    "\"extendedValueG\" on an extended enumeration value".to_owned(),
                );
            }
            return E::extended(extended.unwrap_or_default());
        }
        if let Some(v) = E::from_literal(literal).or_else(|| E::from_alias(literal)) {
            if let Some(extra) = extended {
                self.warn(
                    WarningKind::UnknownAttribute,
                    format!(
                        "\"extendedValueG\" {} beside the standard value {}",
                        quote(&extra),
                        quote(literal)
                    ),
                );
            }
            return v;
        }
        self.warn(WarningKind::UnknownEnumValue, quote(literal));
        E::extended(literal.to_owned())
    }

    /// An enumeration value: `{"value": …, "extendedValueG": …}`, or a plain string with a warning.
    pub(crate) fn enumeration<E: Enumeration>(&mut self) -> Result<Option<E>, Error> {
        if self.ws()? == Some(b'"') {
            let Some(text) = self.string()? else {
                return Ok(None);
            };
            self.warn(
                WarningKind::TypeMismatch,
                "expected an enumeration object {\"value\": …}, found a string".to_owned(),
            );
            return Ok(Some(self.enum_from(text.trim(), None)));
        }
        if !self.begin_object()? {
            return Ok(None);
        }
        let (mut value, mut extended) = (None, None);
        let (mut seen_value, mut seen_extended) = (false, false);
        while let Some(key) = self.next_key()? {
            if key == self.tables.value {
                if self.first(&mut seen_value)? {
                    value = self.string()?;
                }
            } else if key == self.tables.extended_value {
                if self.first(&mut seen_extended)? {
                    extended = self.string()?;
                }
            } else {
                self.skip_unknown()?;
            }
        }
        match value {
            Some(literal) if !literal.trim().is_empty() => {
                Ok(Some(self.enum_from(literal.trim(), extended)))
            }
            _ => {
                self.warn(
                    WarningKind::EmptyValue,
                    "an enumeration object without a value".to_owned(),
                );
                Ok(None)
            }
        }
    }

    /// `{"values": [{"lang": …, "value": …}]}`.
    pub(crate) fn multilingual(&mut self) -> Result<Option<MultilingualString>, Error> {
        if !self.begin_object()? {
            return Ok(None);
        }
        let mut out = MultilingualString::default();
        let mut seen = false;
        while let Some(key) = self.next_key()? {
            if key != self.tables.values {
                self.skip_unknown()?;
                continue;
            }
            if !self.first(&mut seen)? {
                continue;
            }
            let mut list = self.list()?;
            while self.item(&mut list)? {
                if !self.begin_object()? {
                    continue;
                }
                let (mut lang, mut text) = (None, None);
                let (mut seen_lang, mut seen_text) = (false, false);
                while let Some(key) = self.next_key()? {
                    if key == self.tables.lang {
                        if self.first(&mut seen_lang)? {
                            lang = self.string()?;
                        }
                    } else if key == self.tables.value {
                        if self.first(&mut seen_text)? {
                            text = self.string()?;
                        }
                    } else {
                        self.skip_unknown()?;
                    }
                }
                if lang.is_none() {
                    self.warn(
                        WarningKind::MissingAttribute,
                        "required member \"lang\"".to_owned(),
                    );
                }
                out.values.push(LangString {
                    lang: lang.unwrap_or_default(),
                    text: text.unwrap_or_default(),
                });
            }
        }
        Ok(Some(out))
    }

    fn record<T>(
        &mut self,
        read: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<(T, String), Error> {
        let outer = self.recording.replace(Vec::new());
        let result = read(self);
        let bytes = std::mem::replace(&mut self.recording, outer).unwrap_or_default();
        if let Some(outer) = &mut self.recording {
            outer.extend_from_slice(&bytes);
        }
        let value = result?;
        if bytes.len() > self.limits.max_text_len {
            return Err(self.too_long("extension content"));
        }
        let text = String::from_utf8(bytes).map_err(|_| {
            self.error(
                ErrorKind::UnsupportedEncoding,
                "UTF-8 text",
                "bytes that are not valid UTF-8",
            )
        })?;
        Ok((value, text.trim().to_owned()))
    }

    /// Keeps an extension object as it was written.
    pub(crate) fn raw_object(&mut self) -> Result<Option<Extension>, Error> {
        if self.ws()? != Some(b'{') {
            if self.peek()? == Some(b'n') {
                self.literal("null")?;
                self.warn(WarningKind::EmptyValue, "null treated as absent".to_owned());
            } else {
                self.warn(
                    WarningKind::TypeMismatch,
                    "expected an extension object; the value was skipped".to_owned(),
                );
                self.skip_value()?;
            }
            return Ok(None);
        }
        let ((), json) = self.record(|r| r.skip_value())?;
        Ok(Some(Extension {
            json,
            ..Extension::default()
        }))
    }

    /// Keeps a member of an extension object that the model does not know, as a one-member object.
    pub(crate) fn raw_member(&mut self) -> Result<Extension, Error> {
        let key = std::mem::take(&mut self.key);
        let ((), value) = self.record(|r| r.skip_value())?;
        let mut json = String::from("{");
        write_string(&mut json, &key);
        json.push(':');
        json.push_str(&value);
        json.push('}');
        self.key = key;
        Ok(Extension {
            json,
            ..Extension::default()
        })
    }

    /// Checks that only white space follows the document.
    pub(crate) fn end(&mut self) -> Result<(), Error> {
        match self.ws()? {
            None => Ok(()),
            Some(b) => Err(self.error(
                ErrorKind::Syntax,
                "the end of the document",
                quote(&char::from(b).to_string()),
            )),
        }
    }

    /// The member name read last, for messages.
    pub(crate) fn key_text(&self) -> &str {
        &self.key
    }
}

/// The JSON number grammar: `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`.
/// A JSON number with an exponent in plain decimal digits, exactly: `1.5E+2` is `150`, `25e-3`
/// is `0.025`. `None` without an exponent, or with one beyond 64 either way, which no DATEX II
/// value needs and which would otherwise let a few bytes ask for a very long text.
fn without_exponent(text: &str) -> Option<String> {
    let (mantissa, exponent) = text.trim().split_once(['e', 'E'])?;
    let exponent: i32 = exponent.parse().ok()?;
    if exponent.unsigned_abs() > 64 {
        return None;
    }
    let (sign, mantissa) = mantissa
        .strip_prefix('-')
        .map_or(("", mantissa), |m| ("-", m));
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if !(int.bytes().chain(frac.bytes())).all(|b| b.is_ascii_digit()) || int.is_empty() {
        return None;
    }
    let digits = format!("{int}{frac}");
    let point = int.len() as i32 + exponent;
    let plain = if point <= 0 {
        format!("0.{}{digits}", "0".repeat(point.unsigned_abs() as usize))
    } else if point as usize >= digits.len() {
        format!("{digits}{}", "0".repeat(point as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    };
    let trimmed = plain.trim_start_matches('0');
    let plain = if trimmed.is_empty() || trimmed.starts_with('.') {
        format!("0{trimmed}")
    } else {
        trimmed.to_owned()
    };
    Some(format!("{sign}{plain}"))
}

fn is_json_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    let digits = |i: &mut usize| {
        let start = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    };
    let int_start = i;
    let n = digits(&mut i);
    if n == 0 || (n > 1 && b[int_start] == b'0') {
        return false;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        if digits(&mut i) == 0 {
            return false;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

// ---- writing ------------------------------------------------------------------------------------

/// Appends `s` as a JSON string. Quotes, backslashes and control characters are escaped, and so are
/// the line and paragraph separators, which some JavaScript parsers treat as line ends.
pub(crate) fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' || c == '\u{7f}' || c == '\u{2028}' || c == '\u{2029}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A decimal as a JSON number with exactly its digits; only what JSON syntax demands changes: no
/// leading `+`, a digit before and after the point.
fn json_decimal(d: &Decimal) -> String {
    let text = d.as_str().trim();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text.strip_prefix('+').unwrap_or(text)),
    };
    let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
    let int = int.trim_start_matches('0');
    let int = if int.is_empty() { "0" } else { int };
    if frac.is_empty() {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}.{frac}")
    }
}

struct WFrame {
    object: bool,
    first: bool,
    def: u32,
    /// The member written last in this object, by id.
    member: u32,
    seen: Vec<u32>,
}

/// The writer that generated code drives.
pub(crate) struct JsonWriter<'a> {
    sink: &'a mut dyn Write,
    written: u64,
    tables: &'static JsonTables,
    defs: DefNames,
    profile: Option<&'static Profile>,
    frames: Vec<WFrame>,
    path: String,
    marks: Vec<usize>,
    buf: String,
}

impl Rules for JsonWriter<'_> {
    fn fail(&self, kind: ErrorKind, expected: String, found: String) -> Error {
        Error {
            kind,
            path: self.path.clone(),
            offset: self.written,
            expected,
            found,
        }
    }

    fn noun(&self) -> &'static str {
        "member"
    }

    // A required member is found absent after the members before it: the place is the object.
    fn missing(&self, name: &'static str) -> Error {
        self.absent_member(name)
    }

    fn missing_attr(&self, name: &'static str) -> Error {
        self.absent_member(name)
    }
}

impl JsonWriter<'_> {
    fn absent_member(&self, name: &'static str) -> Error {
        let mut error = self.fail(
            ErrorKind::MissingRequired,
            format!("required member {}", quote(name)),
            "none".to_owned(),
        );
        if self.frames.last().is_some_and(|f| f.object && !f.first) {
            if let Some(&mark) = self.marks.last() {
                error.path.truncate(mark);
            }
        }
        error
    }
}

impl<'a> JsonWriter<'a> {
    pub(crate) fn new(
        sink: &'a mut dyn Write,
        tables: &'static JsonTables,
        defs: DefNames,
        profile: Option<&'static Profile>,
    ) -> Self {
        Self {
            sink,
            written: 0,
            tables,
            defs,
            profile,
            frames: Vec::new(),
            path: String::new(),
            marks: Vec::new(),
            buf: String::new(),
        }
    }

    fn put(&mut self, text: &str) -> Result<(), Error> {
        match self.sink.write_all(text.as_bytes()) {
            Ok(()) => {
                self.written += text.len() as u64;
                Ok(())
            }
            Err(e) => Err(self.fail(
                ErrorKind::Io,
                "a sink that accepts the output".to_owned(),
                parser_message(&e.to_string()),
            )),
        }
    }

    fn put_buf(&mut self) -> Result<(), Error> {
        let buf = std::mem::take(&mut self.buf);
        let result = self.put(&buf);
        self.buf = buf;
        self.buf.clear();
        result
    }

    /// Whether the output follows a profile.
    pub(crate) fn profile_active(&self) -> bool {
        self.profile.is_some()
    }

    /// A list the profile can only express as one member of its single entry holds more.
    pub(crate) fn lift_refused(&self, list: &str, member: &str) -> Error {
        self.not_in_profile(format!(
            "member {} with more than {} in one entry, which the profile cannot express",
            quote(list),
            quote(member)
        ))
    }

    fn not_in_profile(&self, what: String) -> Error {
        let profile = self.profile.map_or("", |p| p.name);
        self.fail(
            ErrorKind::InvalidValue,
            format!("content the {profile} profile allows here"),
            what,
        )
    }

    fn profile_def(
        &self,
        def: u32,
    ) -> Option<&'static (u32, bool, &'static [u32], &'static [u32])> {
        let profile = self.profile?;
        profile
            .defs
            .binary_search_by_key(&def, |d| d.0)
            .ok()
            .map(|i| &profile.defs[i])
    }

    /// Starts the document's root object, which belongs to no definition.
    pub(crate) fn document_start(&mut self) -> Result<(), Error> {
        self.put("{")?;
        self.frames.push(WFrame {
            object: true,
            first: true,
            def: UNKNOWN,
            member: UNKNOWN,
            seen: Vec::new(),
        });
        Ok(())
    }

    /// The one member of the root object: `payload` or `messageContainer`.
    pub(crate) fn root_key(&mut self, name: &'static str) -> Result<(), Error> {
        self.buf.clear();
        write_string(&mut self.buf, name);
        self.buf.push(':');
        self.put_buf()?;
        self.push_path(name);
        if let Some(frame) = self.frames.last_mut() {
            frame.first = false;
        }
        Ok(())
    }

    pub(crate) fn document_end(&mut self) -> Result<(), Error> {
        self.frames.pop();
        self.pop_path();
        self.put("}")
    }

    /// Starts an object of the given definition. The profile must contain the definition.
    pub(crate) fn begin_object(&mut self, def: u32) -> Result<(), Error> {
        if self.profile.is_some() && self.profile_def(def).is_none() {
            let name = self.defs.get(def as usize).copied().unwrap_or("?");
            return Err(
                self.not_in_profile(format!("{}, which is not part of the profile", quote(name)))
            );
        }
        self.put("{")?;
        self.frames.push(WFrame {
            object: true,
            first: true,
            def,
            member: UNKNOWN,
            seen: Vec::new(),
        });
        Ok(())
    }

    /// Writes a member name. The profile must allow the member; it may spell it differently.
    pub(crate) fn key(&mut self, member: u32) -> Result<(), Error> {
        let Some(frame) = self.frames.last() else {
            return Err(self.fail(
                ErrorKind::InvalidValue,
                "an object".to_owned(),
                "a member outside one".to_owned(),
            ));
        };
        let (def, first) = (frame.def, frame.first);
        let mut name = self.tables.name(member);
        if let Some(profile) = self.profile {
            let allowed = self
                .profile_def(def)
                .is_some_and(|d| d.1 || d.2.contains(&member));
            if !allowed {
                let place = format!("member {}", quote(name));
                if !first {
                    self.pop_path();
                }
                self.push_path(name);
                return Err(
                    self.not_in_profile(format!("{place}, which the profile does not allow here"))
                );
            }
            if let Some(r) = profile.renames.iter().find(|r| r.0 == def && r.1 == member) {
                name = self.tables.name(r.2);
            }
        }
        if !first {
            self.pop_path();
        }
        self.buf.clear();
        if !first {
            self.buf.push(',');
        }
        write_string(&mut self.buf, name);
        self.buf.push(':');
        self.put_buf()?;
        self.push_path(name);
        let frame = self.frames.last_mut().expect("checked above");
        frame.first = false;
        frame.member = member;
        frame.seen.push(member);
        Ok(())
    }

    /// Required content is absent: an error, unless the profile leaves the member out here.
    pub(crate) fn absent(&self, member: u32, error: Error) -> Result<(), Error> {
        let (def, _) = self.current();
        if self.profile.is_some()
            && self
                .profile_def(def)
                .is_some_and(|d| !d.1 && !d.2.contains(&member))
        {
            return Ok(());
        }
        Err(error)
    }

    /// Ends the open object. The profile's required members must have been written.
    pub(crate) fn end_object(&mut self) -> Result<(), Error> {
        let frame = self.frames.pop().expect("an object is open");
        if !frame.first {
            self.pop_path();
        }
        if self.profile.is_some() {
            if let Some(&(_, _, _, required)) = self.profile_def(frame.def) {
                if let Some(missing) = required.iter().find(|m| !frame.seen.contains(m)) {
                    let name = self.tables.name(*missing);
                    let profile = self.profile.map_or("", |p| p.name);
                    return Err(self.fail(
                        ErrorKind::MissingRequired,
                        format!("member {} that the {profile} profile requires", quote(name)),
                        "none".to_owned(),
                    ));
                }
            }
        }
        self.put("}")
    }

    pub(crate) fn begin_array(&mut self) -> Result<(), Error> {
        self.put("[")?;
        self.frames.push(WFrame {
            object: false,
            first: true,
            def: UNKNOWN,
            member: UNKNOWN,
            seen: Vec::new(),
        });
        Ok(())
    }

    pub(crate) fn item(&mut self) -> Result<(), Error> {
        let frame = self.frames.last_mut().expect("an array is open");
        if !frame.first {
            self.put(",")?;
        }
        self.frames.last_mut().expect("an array is open").first = false;
        Ok(())
    }

    pub(crate) fn end_array(&mut self) -> Result<(), Error> {
        self.frames.pop();
        self.put("]")
    }

    fn push_path(&mut self, segment: &str) {
        self.marks.push(self.path.len());
        if !self.path.is_empty() {
            self.path.push('/');
        }
        self.path.push_str(segment);
    }

    fn pop_path(&mut self) {
        if let Some(mark) = self.marks.pop() {
            self.path.truncate(mark);
        }
    }

    /// The (definition, member) of the nearest object.
    fn current(&self) -> (u32, u32) {
        self.frames
            .iter()
            .rev()
            .find(|f| f.object)
            .map_or((UNKNOWN, UNKNOWN), |f| (f.def, f.member))
    }

    /// Whether a polymorphic value is written inside its wrapper object. The profile writes some
    /// without one and then allows only one kind.
    pub(crate) fn wrapper(&mut self, variant: u32) -> Result<bool, Error> {
        let Some(profile) = self.profile else {
            return Ok(true);
        };
        let (def, member) = self.current();
        let Some(rule) = profile.unwraps.iter().find(|u| u.0 == def && u.1 == member) else {
            return Ok(true);
        };
        if rule.2 == variant {
            return Ok(false);
        }
        let only = self.tables.name(rule.2);
        let found = self.tables.name(variant);
        Err(self.not_in_profile(format!(
            "{} where only {} is allowed",
            quote(found),
            quote(only)
        )))
    }

    // -- values --

    pub(crate) fn string(&mut self, value: &str) -> Result<(), Error> {
        if let Some(c) = value.chars().find(|c| *c == '\u{fffe}' || *c == '\u{ffff}') {
            return Err(self.fail(
                ErrorKind::InvalidValue,
                "characters JSON can carry".to_owned(),
                format!("\\u{{{:x}}}", c as u32),
            ));
        }
        self.buf.clear();
        write_string(&mut self.buf, value);
        self.put_buf()
    }

    pub(crate) fn decimal(&mut self, value: &Decimal) -> Result<(), Error> {
        let text = json_decimal(value);
        if !is_json_number(&text) {
            return Err(self.fail(
                ErrorKind::InvalidValue,
                "a decimal number".to_owned(),
                quote(value.as_str()),
            ));
        }
        self.put(&text)
    }

    pub(crate) fn float32(&mut self, value: f32) -> Result<(), Error> {
        if !value.is_finite() {
            return Err(self.fail(
                ErrorKind::InvalidValue,
                "a finite number".to_owned(),
                value.to_string(),
            ));
        }
        // Rust writes the shortest text that reads back to the same f32, never with an exponent.
        self.put(&value.to_string())
    }

    pub(crate) fn integer(&mut self, value: impl std::fmt::Display) -> Result<(), Error> {
        self.put(&value.to_string())
    }

    pub(crate) fn boolean(&mut self, value: bool) -> Result<(), Error> {
        self.put(if value { "true" } else { "false" })
    }

    /// JSON date-times carry a time zone; one read without it cannot be written.
    pub(crate) fn date_time(&mut self, value: &DateTime) -> Result<(), Error> {
        if value.offset_minutes.is_none() {
            return Err(self.fail(
                ErrorKind::InvalidValue,
                "a date and time with a time zone".to_owned(),
                quote(&value.to_string()),
            ));
        }
        self.string(&value.to_string())
    }

    pub(crate) fn date(&mut self, value: &Date) -> Result<(), Error> {
        self.string(&value.to_string())
    }

    /// The profile requires a `Z` on times of day, which it says not to read as UTC.
    pub(crate) fn time(&mut self, value: &Time) -> Result<(), Error> {
        let mut text = value.to_string();
        if self.profile.is_some() {
            text.push('Z');
        }
        self.string(&text)
    }

    pub(crate) fn base64(&mut self, value: &[u8]) -> Result<(), Error> {
        let mut text = String::new();
        crate::out::Text::put(&value.to_vec(), &mut text);
        self.string(&text)
    }

    /// `{"value": …, "extendedValueG": …}`, with the profile's literal where it differs.
    pub(crate) fn enumeration<'v>(
        &mut self,
        def: u32,
        (literal, mut extended): (&'v str, Option<&'v str>),
    ) -> Result<(), Error> {
        let mut literal = if literal == "_extended" {
            "extendedG"
        } else {
            literal
        };
        if let Some(profile) = self.profile {
            // A value 3.7 lacks but the profile lists, held as an extended value, is its literal.
            if let (Some(value), Ok(i)) = (
                extended,
                profile.literals.binary_search_by_key(&def, |l| l.0),
            ) {
                if literal == "extendedG" && profile.literals[i].1.contains(&value) {
                    (literal, extended) = (value, None);
                }
            }
            if literal != "extendedG" {
                if let Ok(i) = profile.literals.binary_search_by_key(&def, |l| l.0) {
                    if !profile.literals[i].1.contains(&literal) {
                        let name = self.defs.get(def as usize).copied().unwrap_or("?");
                        let listed = profile.literals[i]
                            .1
                            .iter()
                            .filter(|l| **l != "_extended")
                            .copied()
                            .collect::<Vec<_>>();
                        let mut allowed = listed[..listed.len().min(8)].join(", ");
                        if listed.len() > 8 {
                            allowed.push_str(", …");
                        }
                        return Err(self.not_in_profile(format!(
                            "{} of {}, which the profile does not list (it lists {allowed})",
                            quote(literal),
                            quote(name)
                        )));
                    }
                }
                if let Some(r) = profile
                    .literal_renames
                    .iter()
                    .find(|r| r.0 == def && r.1 == literal)
                {
                    literal = r.2;
                }
            }
        }
        self.buf.clear();
        self.buf.push_str("{\"value\":");
        write_string(&mut self.buf, literal);
        if let Some(extended) = extended {
            self.buf.push_str(",\"extendedValueG\":");
            write_string(&mut self.buf, extended);
        }
        self.buf.push('}');
        self.put_buf()
    }

    /// `{"values": [{"lang": …, "value": …}]}`. JSON allows only two lowercase letters as a language.
    pub(crate) fn multilingual(&mut self, value: &MultilingualString) -> Result<(), Error> {
        self.buf.clear();
        self.buf.push_str("{\"values\":[");
        for (i, entry) in value.values.iter().enumerate() {
            let lang = entry.lang.as_bytes();
            if lang.len() != 2 || !lang.iter().all(u8::is_ascii_lowercase) {
                return Err(self.fail(
                    ErrorKind::InvalidValue,
                    "a language code of two lowercase letters, such as \"de\"".to_owned(),
                    quote(&entry.lang),
                ));
            }
            if entry.text.chars().count() > 1024 {
                return Err(self.fail(
                    ErrorKind::InvalidValue,
                    "a text of at most 1024 characters".to_owned(),
                    quote(&entry.text),
                ));
            }
            if i > 0 {
                self.buf.push(',');
            }
            self.buf.push_str("{\"lang\":");
            write_string(&mut self.buf, &entry.lang);
            self.buf.push_str(",\"value\":");
            write_string(&mut self.buf, &entry.text);
            self.buf.push('}');
        }
        self.buf.push_str("]}");
        self.put_buf()
    }

    /// Checks that extension content is one well-formed JSON object and returns its text.
    fn checked_object<'e>(&self, ext: &'e Extension) -> Result<&'e str, Error> {
        if ext.json.is_empty() && !ext.xml.is_empty() {
            return Err(self.fail(
                ErrorKind::InvalidExtension,
                "extension content read from JSON".to_owned(),
                "content read from XML, which cannot be written as JSON".to_owned(),
            ));
        }
        let text = ext.json.trim();
        if text.is_empty() {
            return Ok("{}");
        }
        let mut reader =
            JsonReader::new(Box::new(text.as_bytes()), self.tables, &Limits::default());
        let well_formed =
            text.starts_with('{') && reader.skip_value().is_ok() && reader.end().is_ok();
        if !well_formed {
            return Err(self.fail(
                ErrorKind::InvalidExtension,
                "exactly one well-formed JSON object".to_owned(),
                quote(text),
            ));
        }
        Ok(text)
    }

    pub(crate) fn extension(&mut self, ext: &Extension) -> Result<(), Error> {
        let text = self.checked_object(ext)?.to_owned();
        self.put(&text)
    }

    /// Writes members kept from an extension object as members of the open object.
    pub(crate) fn foreign_members(&mut self, members: &[Extension]) -> Result<(), Error> {
        for ext in members {
            let text = self.checked_object(ext)?;
            let inner = text[1..text.len() - 1].trim().to_owned();
            if inner.is_empty() {
                continue;
            }
            let frame = self.frames.last_mut().expect("an object is open");
            let first = frame.first;
            frame.first = false;
            if first {
                self.put(&inner)?;
            } else {
                // Leave the previous member, as `key` does.
                self.pop_path();
                self.put(",")?;
                self.put(&inner)?;
            }
            // Every written member holds one path segment until the next member or the end.
            self.push_path("(extension)");
        }
        Ok(())
    }

    /// The members of the payload wrapper that the payload's attributes become:
    /// `modelBaseVersionG`, then extension and profile name and version where set. The profile
    /// writes its own name and version when the publication does not set them.
    pub(crate) fn payload_header(&mut self, values: [Option<&str>; 4]) -> Result<(), Error> {
        let [
            model,
            extension_name,
            extension_version,
            profile_name,
            profile_version,
        ] = self.tables.header;
        self.key(model)?;
        self.string("3")?;
        let [ext_name, ext_version, mut prof_name, mut prof_version] = values;
        if let Some(profile) = self.profile {
            prof_name = prof_name.or(Some(profile.name));
            prof_version = prof_version.or(Some(profile.version));
        }
        for (member, value) in [
            (extension_name, ext_name),
            (extension_version, ext_version),
            (profile_name, prof_name),
            (profile_version, prof_version),
        ] {
            if let Some(value) = value {
                self.key(member)?;
                self.string(value)?;
            }
        }
        Ok(())
    }

    /// The target class of a typed reference, which JSON requires and XML fixes.
    pub(crate) fn target_class(&mut self, class: &'static str) -> Result<(), Error> {
        let member = self.tables.target_class;
        self.key(member)?;
        self.string(class)
    }

    /// The error for a value the model holds that JSON has no member for.
    pub(crate) fn no_place(&self, name: &'static str) -> Error {
        self.fail(
            ErrorKind::InvalidValue,
            format!("no {}, which DATEX II JSON has no member for", quote(name)),
            "a value".to_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponents_become_plain_digits() {
        for (text, plain) in [
            ("1.5E+2", "150"),
            ("2.5e2", "250"),
            ("25e-3", "0.025"),
            ("-1.25E1", "-12.5"),
            ("1.50E1", "15.0"),
            ("0e0", "0"),
            ("5E-1", "0.5"),
            ("123E0", "123"),
        ] {
            assert_eq!(without_exponent(text).as_deref(), Some(plain), "{text}");
        }
        assert_eq!(without_exponent("1E65"), None);
        assert_eq!(without_exponent("1E-999999999"), None);
        assert_eq!(without_exponent("150"), None);
    }

    #[test]
    fn number_grammar() {
        for good in ["0", "-0", "12", "0.59", "1e3", "-2.5E-3", "22000"] {
            assert!(is_json_number(good), "{good}");
        }
        for bad in [
            "", "-", "01", "1.", ".5", "+1", "1e", "0x10", "1.2.3", "NaN",
        ] {
            assert!(!is_json_number(bad), "{bad}");
        }
    }

    #[test]
    fn decimals_keep_their_digits() {
        for (input, output) in [
            ("0.59", "0.59"),
            ("0.590", "0.590"),
            ("+12.5", "12.5"),
            (".5", "0.5"),
            ("007.10", "7.10"),
            ("-0.0", "-0.0"),
            ("999999.99", "999999.99"),
            ("12.", "12"),
        ] {
            let d: Decimal = input.parse().unwrap();
            assert_eq!(json_decimal(&d), output, "{input}");
        }
    }

    #[test]
    fn floats_are_shortest_round_trip() {
        for (value, text) in [
            (22000.0f32, "22000"),
            (52.520008, "52.520008"),
            (0.1, "0.1"),
            (230.0, "230"),
            (-13.404954, "-13.404954"),
        ] {
            assert_eq!(value.to_string(), text);
            assert_eq!(text.parse::<f32>().unwrap(), value);
        }
    }

    #[test]
    fn strings_are_escaped() {
        let mut out = String::new();
        write_string(&mut out, "a\"b\\c\nd\u{1}e\u{2028}f/é");
        // U+2028 and U+2029 are escaped too, so that the output can be embedded in JavaScript.
        assert_eq!(out, r#""a\"b\\c\nd\u0001e\u2028f/é""#);
    }
}
