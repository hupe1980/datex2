//! The reader core: everything about XML that the generated per-type readers do not need to know.
//!
//! It turns the event stream into "the next child element is X", resolves names to integers,
//! keeps the element path, enforces the limits and collects warnings. Generated code never sees
//! an event or a string name.

use std::io::BufRead;

use quick_xml::events::Event;
use quick_xml::name::{QName, ResolveResult};
use quick_xml::reader::NsReader;

use crate::error::{Error, ErrorKind, Warning, WarningKind, parser_message, quote};
use crate::types::{DateTime, Extension, FromText, LangString, MultilingualString, base64};

/// Bounds applied to input, which is assumed to be untrusted.
///
/// The defaults are far above anything seen in real feeds. Raise one when a legitimate document
/// needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// Deepest allowed element nesting. Default 100.
    pub max_depth: usize,
    /// Most attributes on one element, namespace declarations included. Default 256.
    /// Namespace declarations in scope at any point may number four times this.
    pub max_attributes: usize,
    /// Longest single text value, attribute value or extension content, in bytes. Default 8 MiB.
    /// A single tag or run of text in the input, read or passed over, may be 64 KiB longer.
    pub max_text_len: usize,
    /// Most warnings kept; further ones are only counted. Default 1,000.
    pub max_warnings: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 100,
            max_attributes: 256,
            max_text_len: 8 << 20,
            max_warnings: 1000,
        }
    }
}

/// Namespace id of names that have no namespace.
pub(crate) const NS_NONE: u32 = 0;
/// Namespace id of SOAP 1.1.
pub(crate) const NS_SOAP11: u32 = 0xFD;
/// Namespace id of SOAP 1.2.
pub(crate) const NS_SOAP12: u32 = 0xFE;
/// Namespace id of every namespace the model does not know.
pub(crate) const NS_FOREIGN: u32 = 0xFF;
/// Name id of every local name the model does not know.
pub(crate) const NAME_UNKNOWN: u32 = 0xFFFF;

/// Namespace declarations in scope may number this many times `max_attributes`.
const BINDINGS_PER_ATTRIBUTE: usize = 4;

/// Longest element name kept in a path, in characters.
const PATH_NAME_MAX: usize = 80;

const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
const SOAP11: &str = "http://schemas.xmlsoap.org/soap/envelope/";
const SOAP12: &str = "http://www.w3.org/2003/05/soap-envelope";

/// The fixed vocabulary of one DATEX II major version, produced by the generator.
pub(crate) struct Tables {
    /// Prefix shared by all standard namespaces, for example `http://datex2.eu/schema/3/`.
    pub base: &'static str,
    /// Prefix of the Level C look-alike namespaces.
    pub level_c: &'static str,
    /// Namespace names after the prefix; index + 1 is the namespace id.
    pub namespaces: &'static [&'static str],
    /// Every local name of an element, attribute or type, sorted; the index is the name id.
    pub names: &'static [&'static str],
}

impl Tables {
    fn name_id(&self, local: &str) -> u32 {
        self.names
            .binary_search(&local)
            .map_or(NAME_UNKNOWN, |i| i as u32)
    }

    fn name(&self, id: u32) -> &'static str {
        self.names.get(id as usize).copied().unwrap_or("?")
    }

    /// Namespace id and whether the URI was a Level C look-alike.
    fn ns_id(&self, uri: &str) -> (u32, bool) {
        let lookup = |rest: &str| {
            self.namespaces
                .iter()
                .position(|n| *n == rest)
                .map(|i| i as u32 + 1)
        };
        if let Some(id) = uri.strip_prefix(self.base).and_then(lookup) {
            return (id, false);
        }
        if let Some(id) = uri.strip_prefix(self.level_c).and_then(lookup) {
            return (id, true);
        }
        match uri {
            SOAP11 => (NS_SOAP11, false),
            SOAP12 => (NS_SOAP12, false),
            _ => (NS_FOREIGN, false),
        }
    }

    fn is_standard(&self, ns: u32) -> bool {
        ns >= 1 && ns as usize <= self.namespaces.len()
    }
}

/// An enumeration of the model.
pub(crate) trait Enumeration: Sized {
    /// The variant for a standard literal.
    fn from_literal(literal: &str) -> Option<Self>;
    /// The variant that carries a value outside the standard list.
    fn extended(value: String) -> Self;
}

struct Attr {
    id: u32,
    value: String,
    /// The name as written, kept only for names the model does not know.
    unknown: Option<String>,
    taken: bool,
}

/// Everything except the event source, so that it can be used while an event is borrowed.
struct State {
    tables: &'static Tables,
    limits: Limits,
    path: String,
    marks: Vec<u32>,
    warnings: Vec<Warning>,
    suppressed: usize,
    attrs: Vec<Attr>,
    xsi_type: Option<u32>,
    xsi_raw: String,
    /// Namespace URI of the current element when it is not a standard one.
    foreign_uri: String,
    /// Start tag of the current element as written, without the angle brackets.
    open_tag: String,
    level_c_seen: u64,
    ns_cache: Vec<(String, u32)>,
    ext_value_id: u32,
    lang_id: u32,
    value_id: u32,
}

impl State {
    fn warn(&mut self, kind: WarningKind, offset: u64, detail: String) {
        if self.warnings.len() < self.limits.max_warnings {
            self.warnings.push(Warning {
                kind,
                path: self.path.clone(),
                offset,
                detail,
            });
        } else {
            self.suppressed += 1;
        }
    }

    fn error(
        &self,
        kind: ErrorKind,
        offset: u64,
        expected: impl Into<String>,
        found: impl Into<String>,
    ) -> Error {
        Error {
            kind,
            path: self.path.clone(),
            offset,
            expected: expected.into(),
            found: found.into(),
        }
    }

    fn push(&mut self, local: &str) {
        self.marks.push(self.path.len() as u32);
        if !self.path.is_empty() {
            self.path.push('/');
        }
        if local.len() <= PATH_NAME_MAX && local.bytes().all(|b| b.is_ascii_graphic() && b != b'/')
        {
            self.path.push_str(local);
        } else {
            // The parser accepts almost any bytes as a name. The path is printed in errors and
            // warnings, so a name gets the same treatment as quoted input text.
            for (n, c) in local.chars().enumerate() {
                if n == PATH_NAME_MAX {
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
    }

    fn pop(&mut self) {
        if let Some(mark) = self.marks.pop() {
            self.path.truncate(mark as usize);
        }
    }

    fn local(&self) -> &str {
        match self.marks.last() {
            Some(&0) | None => &self.path,
            Some(&mark) => &self.path[mark as usize + 1..],
        }
    }

    fn resolve_ns(&mut self, uri: &str, offset: u64) -> u32 {
        if let Some((_, id)) = self.ns_cache.iter().find(|(u, _)| u == uri) {
            return *id;
        }
        let (id, level_c) = self.tables.ns_id(uri);
        if level_c && id < 64 && self.level_c_seen & (1 << id) == 0 {
            self.level_c_seen |= 1 << id;
            self.warn(WarningKind::LevelCNamespace, offset, quote(uri));
        }
        if self.ns_cache.len() < 64 {
            self.ns_cache.push((uri.to_owned(), id));
        }
        id
    }

    fn take_attr(&mut self, id: u32) -> Option<String> {
        let attr = self.attrs.iter_mut().find(|a| a.id == id && !a.taken)?;
        attr.taken = true;
        Some(std::mem::take(&mut attr.value))
    }
}

/// The reader that generated code drives.
pub struct Xml<'a> {
    rd: NsReader<Bounded<'a>>,
    /// Most bytes one tag or text run may take from the input.
    token_max: usize,
    buf: Vec<u8>,
    text: String,
    st: State,
}

fn xml_error(st: &State, offset: u64, e: quick_xml::Error) -> Error {
    use quick_xml::Error as E;
    match e {
        E::Io(e) if e.get_ref().is_some_and(|inner| inner.is::<TokenTooLong>()) => st.error(
            ErrorKind::LimitExceeded,
            offset,
            format!(
                "a tag or text of at most {} bytes (max_text_len)",
                st.limits.max_text_len
            ),
            "a longer one",
        ),
        E::Io(e) => st.error(
            ErrorKind::Io,
            offset,
            "readable input",
            parser_message(&e.to_string()),
        ),
        E::Encoding(_) => st.error(
            ErrorKind::UnsupportedEncoding,
            offset,
            "UTF-8 text",
            "bytes that are not valid UTF-8",
        ),
        E::Escape(quick_xml::escape::EscapeError::UnrecognizedEntity(_, name)) => st.error(
            ErrorKind::DoctypeNotAllowed,
            offset,
            "a predefined entity or a character reference",
            quote(&format!("&{name};")),
        ),
        E::Namespace(quick_xml::name::NamespaceError::TooManyBindings { .. }) => {
            let max = st
                .limits
                .max_attributes
                .saturating_mul(BINDINGS_PER_ATTRIBUTE);
            st.error(
                ErrorKind::LimitExceeded,
                offset,
                format!("at most {max} namespace declarations in scope (4 × max_attributes)"),
                "more",
            )
        }
        other => st.error(
            ErrorKind::Syntax,
            offset,
            "well-formed XML",
            parser_message(&other.to_string()),
        ),
    }
}

/// Where a parser error happened: the start of the offending token when the parser recorded it.
fn error_offset(rd: &NsReader<Bounded<'_>>) -> u64 {
    match rd.error_position() {
        0 => rd.buffer_position(),
        at => at,
    }
}

impl<'a> Xml<'a> {
    pub(crate) fn new(
        input: Box<dyn BufRead + 'a>,
        tables: &'static Tables,
        limits: &Limits,
    ) -> Self {
        let token_max = limits.max_text_len.saturating_add(TOKEN_SLACK);
        let mut rd = NsReader::from_reader(Bounded {
            inner: input,
            left: token_max,
        });
        rd.resolver_mut().set_max_namespace_bindings(
            limits.max_attributes.saturating_mul(BINDINGS_PER_ATTRIBUTE),
        );
        let config = rd.config_mut();
        config.expand_empty_elements = true;
        config.check_end_names = true;
        Self {
            rd,
            token_max,
            buf: Vec::new(),
            text: String::new(),
            st: State {
                tables,
                limits: limits.clone(),
                path: String::new(),
                marks: Vec::new(),
                warnings: Vec::new(),
                suppressed: 0,
                attrs: Vec::new(),
                xsi_type: None,
                xsi_raw: String::new(),
                foreign_uri: String::new(),
                open_tag: String::new(),
                level_c_seen: 0,
                ns_cache: Vec::new(),
                ext_value_id: tables.name_id("_extendedValue"),
                lang_id: tables.name_id("lang"),
                value_id: tables.name_id("value"),
            },
        }
    }

    /// Hands out the warnings collected so far and starts an empty list.
    pub(crate) fn take_warnings(&mut self) -> Vec<Warning> {
        std::mem::take(&mut self.st.warnings)
    }

    pub(crate) fn suppressed(&self) -> usize {
        self.st.suppressed
    }

    pub(crate) fn offset(&self) -> u64 {
        self.rd.buffer_position()
    }

    /// Local name of the element that was opened last and is still open.
    pub(crate) fn local(&self) -> &str {
        self.st.local()
    }

    pub(crate) fn warn(&mut self, kind: WarningKind, detail: String) {
        let offset = self.offset();
        self.st.warn(kind, offset, detail);
    }

    pub(crate) fn error(
        &self,
        kind: ErrorKind,
        expected: impl Into<String>,
        found: impl Into<String>,
    ) -> Error {
        self.st.error(kind, self.offset(), expected, found)
    }

    /// Advances to the next child element of the element that is currently open.
    ///
    /// Returns its key (namespace id in the high half, name id in the low half), with the element
    /// now open: the caller must consume it with one of the value readers, a generated `read`,
    /// [`skip`](Self::skip) or [`raw`](Self::raw). Returns `None` at the end tag of the current
    /// element, which stays on the path until [`close`](Self::close).
    pub(crate) fn next_child(&mut self) -> Result<Option<u32>, Error> {
        loop {
            self.buf.clear();
            self.rd.get_mut().left = self.token_max;
            let event = match self.rd.read_event_into(&mut self.buf) {
                Ok(event) => event,
                Err(e) => return Err(xml_error(&self.st, error_offset(&self.rd), e)),
            };
            let offset = self.rd.buffer_position();
            match event {
                Event::Start(start) => {
                    let (ns, _) = self.rd.resolver().resolve_element(start.name());
                    let st = &mut self.st;
                    if st.marks.len() >= st.limits.max_depth {
                        return Err(st.error(
                            ErrorKind::LimitExceeded,
                            offset,
                            format!(
                                "nesting of at most {} elements (max_depth)",
                                st.limits.max_depth
                            ),
                            "a deeper element",
                        ));
                    }
                    let ns_id = match ns {
                        ResolveResult::Bound(uri) => {
                            let id = st.resolve_ns(uri.0, offset);
                            st.foreign_uri.clear();
                            if !st.tables.is_standard(id) {
                                st.foreign_uri.push_str(uri.0);
                            }
                            id
                        }
                        ResolveResult::Unbound => {
                            st.foreign_uri.clear();
                            NS_NONE
                        }
                        ResolveResult::Unknown(prefix) => {
                            return Err(st.error(
                                ErrorKind::Syntax,
                                offset,
                                "a declared namespace prefix",
                                quote(&prefix),
                            ));
                        }
                    };
                    let local = start.local_name();
                    let name_id = st.tables.name_id(local.as_ref());
                    st.push(local.as_ref());
                    st.open_tag.clear();
                    st.open_tag.push_str(&start);

                    st.attrs.clear();
                    st.xsi_type = None;
                    let mut count = 0usize;
                    for attr in start.attributes() {
                        let attr = attr.map_err(|e| {
                            st.error(
                                ErrorKind::Syntax,
                                offset,
                                "well-formed attributes",
                                parser_message(&e.to_string()),
                            )
                        })?;
                        count += 1;
                        if count > st.limits.max_attributes {
                            return Err(st.error(
                                ErrorKind::LimitExceeded,
                                offset,
                                format!(
                                    "at most {} attributes on an element (max_attributes)",
                                    st.limits.max_attributes
                                ),
                                "more attributes",
                            ));
                        }
                        if attr.value.len() > st.limits.max_text_len {
                            return Err(st.error(
                                ErrorKind::LimitExceeded,
                                offset,
                                format!(
                                    "attribute values of at most {} bytes (max_text_len)",
                                    st.limits.max_text_len
                                ),
                                format!("{} bytes", attr.value.len()),
                            ));
                        }
                        if attr.key.as_namespace_binding().is_some() {
                            continue;
                        }
                        let (prefix_ns, attr_local) =
                            self.rd.resolver().resolve_attribute(attr.key);
                        let value = match attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        {
                            Ok(v) => v,
                            Err(e) => return Err(xml_error(st, offset, e)),
                        };
                        match prefix_ns {
                            ResolveResult::Unbound => {
                                let id = st.tables.name_id(attr_local.as_ref());
                                let unknown = (id == NAME_UNKNOWN).then(|| attr.key.0.to_owned());
                                st.attrs.push(Attr {
                                    id,
                                    value: value.into_owned(),
                                    unknown,
                                    taken: false,
                                });
                            }
                            ResolveResult::Bound(uri) if uri.0 == XSI => {
                                if attr_local.as_ref() == "type" {
                                    let value = value.trim();
                                    let (type_ns, type_local) =
                                        self.rd.resolver().resolve(QName(value), true);
                                    let type_ns = match type_ns {
                                        ResolveResult::Bound(uri) => st.resolve_ns(uri.0, offset),
                                        ResolveResult::Unbound => NS_NONE,
                                        ResolveResult::Unknown(_) => NS_FOREIGN,
                                    };
                                    st.xsi_type = Some(
                                        type_ns << 16 | st.tables.name_id(type_local.as_ref()),
                                    );
                                    st.xsi_raw.clear();
                                    st.xsi_raw.push_str(value);
                                }
                                // schemaLocation, noNamespaceSchemaLocation and nil are never used.
                            }
                            _ => {
                                st.attrs.push(Attr {
                                    id: NAME_UNKNOWN,
                                    value: value.into_owned(),
                                    unknown: Some(attr.key.0.to_owned()),
                                    taken: false,
                                });
                            }
                        }
                    }
                    return Ok(Some(ns_id << 16 | name_id));
                }
                Event::End(_) => return Ok(None),
                Event::Eof => {
                    let expected = if self.st.marks.is_empty() {
                        "a root element".to_owned()
                    } else {
                        format!("the end tag of {}", quote(self.st.local()))
                    };
                    return Err(self.st.error(
                        ErrorKind::Syntax,
                        offset,
                        expected,
                        "the end of the input",
                    ));
                }
                Event::DocType(_) => {
                    return Err(self.st.error(
                        ErrorKind::DoctypeNotAllowed,
                        offset,
                        "no document type declaration",
                        "<!DOCTYPE",
                    ));
                }
                Event::Decl(decl) => {
                    if let Some(Ok(encoding)) = decl.encoding() {
                        if !encoding.eq_ignore_ascii_case("utf-8")
                            && !encoding.eq_ignore_ascii_case("utf8")
                        {
                            return Err(self.st.error(
                                ErrorKind::UnsupportedEncoding,
                                offset,
                                "UTF-8",
                                quote(&encoding),
                            ));
                        }
                    }
                }
                // Text between elements, comments and processing instructions carry no data here.
                _ => {}
            }
        }
    }

    /// Leaves the element whose end tag [`next_child`](Self::next_child) reported.
    pub(crate) fn close(&mut self) {
        self.st.pop();
    }

    /// Skips the rest of the open element, whatever it contains.
    pub(crate) fn skip(&mut self) -> Result<(), Error> {
        let mut depth = 0usize;
        loop {
            self.buf.clear();
            self.rd.get_mut().left = self.token_max;
            let event = match self.rd.read_event_into(&mut self.buf) {
                Ok(event) => event,
                Err(e) => return Err(xml_error(&self.st, error_offset(&self.rd), e)),
            };
            let offset = self.rd.buffer_position();
            match event {
                Event::Start(_) => {
                    depth += 1;
                    if self.st.marks.len() + depth > self.st.limits.max_depth {
                        return Err(self.st.error(
                            ErrorKind::LimitExceeded,
                            offset,
                            format!(
                                "nesting of at most {} elements (max_depth)",
                                self.st.limits.max_depth
                            ),
                            "a deeper element",
                        ));
                    }
                }
                Event::End(_) if depth == 0 => break,
                Event::End(_) => depth -= 1,
                Event::Eof => {
                    return Err(self.st.error(
                        ErrorKind::Syntax,
                        offset,
                        format!("the end tag of {}", quote(self.st.local())),
                        "the end of the input",
                    ));
                }
                Event::DocType(_) => {
                    return Err(self.st.error(
                        ErrorKind::DoctypeNotAllowed,
                        offset,
                        "no document type declaration",
                        "<!DOCTYPE",
                    ));
                }
                _ => {}
            }
        }
        self.st.pop();
        Ok(())
    }

    pub(crate) fn describe_current(&self, key: u32) -> String {
        let ns = key >> 16;
        if self.st.tables.is_standard(ns) {
            format!(
                "{} in namespace {}",
                quote(self.st.local()),
                self.st.tables.namespaces[ns as usize - 1]
            )
        } else if self.st.foreign_uri.is_empty() {
            format!("{} in no namespace", quote(self.st.local()))
        } else {
            format!(
                "{} in namespace {}",
                quote(self.st.local()),
                quote(&self.st.foreign_uri)
            )
        }
    }

    /// Skips the open element and reports it as unknown.
    pub(crate) fn skip_unknown(&mut self, key: u32) -> Result<(), Error> {
        let detail = self.describe_current(key);
        self.warn(WarningKind::UnknownElement, detail);
        self.skip()
    }

    /// Skips the open element without a warning: it is known but deliberately not interpreted.
    pub(crate) fn skip_silently(&mut self) -> Result<(), Error> {
        self.skip()
    }

    /// Looks for the open element among `fields` by local name alone.
    ///
    /// Used after the exact match failed. A hit is reported, and accepted only when the element is
    /// in a standard namespace and exactly one field has that local name.
    pub(crate) fn fallback(&mut self, key: u32, fields: &[u32]) -> Option<u32> {
        let name = key & 0xFFFF;
        if name == NAME_UNKNOWN || !self.st.tables.is_standard(key >> 16) {
            return None;
        }
        let mut hits = fields
            .iter()
            .copied()
            .filter(|f| f & 0xFFFF == name && *f != key);
        let hit = hits.next()?;
        if hits.next().is_some() {
            return None;
        }
        let detail = format!(
            "{}, expected in {}",
            self.describe_current(key),
            self.st.tables.namespaces[(hit >> 16) as usize - 1]
        );
        self.warn(WarningKind::WrongNamespace, detail);
        Some(hit)
    }

    // ---- attributes of the open element -------------------------------------------------------

    /// The `xsi:type` of the open element as a key, if it has one.
    pub(crate) fn xsi_type(&self) -> Option<u32> {
        self.st.xsi_type
    }

    /// Reports an `xsi:type` that differs from the only type allowed here.
    pub(crate) fn expect_type(&mut self, key: u32) {
        if self.st.xsi_type.is_some_and(|t| t != key) {
            let detail = format!(
                "{}, reading as {}",
                quote(&self.st.xsi_raw),
                self.st.tables.name(key & 0xFFFF)
            );
            self.warn(WarningKind::UnexpectedType, detail);
        }
    }

    /// Skips the open element because its type cannot be determined, and says so.
    pub(crate) fn unknown_type(&mut self, base: &str) -> Result<(), Error> {
        let detail = match self.st.xsi_type {
            Some(_) => format!("{} is not a known kind of {base}", quote(&self.st.xsi_raw)),
            None => format!("no xsi:type given and {base} is abstract"),
        };
        self.warn(WarningKind::UnknownType, detail);
        self.skip()
    }

    /// Reports an `xsi:type` the model does not know where the declared type can be read
    /// instead: the object is read as the declared type and whatever the unknown type adds shows
    /// up as unknown elements.
    pub(crate) fn unknown_type_as(&mut self, base: &str) {
        let detail = format!(
            "{} is not a known kind of {base}, reading as {base}",
            quote(&self.st.xsi_raw)
        );
        self.warn(WarningKind::UnknownType, detail);
        self.st.xsi_type = None;
    }

    /// The type name as written, for error messages.
    pub(crate) fn xsi_raw(&self) -> &str {
        &self.st.xsi_raw
    }

    pub(crate) fn attr(&mut self, id: u32) -> Option<String> {
        self.st.take_attr(id)
    }

    /// An attribute the schema requires. Real feeds omit such attributes, so absence is reported
    /// and the field stays empty.
    pub(crate) fn req_attr(&mut self, id: u32) -> Option<String> {
        let value = self.st.take_attr(id);
        if value.is_none() {
            let detail = format!("required attribute {}", quote(self.st.tables.name(id)));
            self.warn(WarningKind::MissingAttribute, detail);
        }
        value
    }

    pub(crate) fn attr_value<T: FromText>(&mut self, id: u32) -> Result<Option<T>, Error> {
        let Some(text) = self.st.take_attr(id) else {
            return Ok(None);
        };
        let value = T::from_text(text.trim());
        if value.is_none() {
            let detail = format!(
                "expected {} in attribute {}, found {}",
                T::EXPECTED,
                quote(self.st.tables.name(id)),
                quote(&text)
            );
            self.warn(WarningKind::InvalidValue, detail);
        }
        Ok(value)
    }

    pub(crate) fn req_attr_value<T: FromText>(&mut self, id: u32) -> Result<Option<T>, Error> {
        let value = self.attr_value(id)?;
        if value.is_none() {
            let detail = format!("required attribute {}", quote(self.st.tables.name(id)));
            self.warn(WarningKind::MissingAttribute, detail);
        }
        Ok(value)
    }

    pub(crate) fn attr_enum<E: Enumeration>(&mut self, id: u32) -> Option<E> {
        let text = self.st.take_attr(id)?;
        Some(self.enum_from(text.trim(), None))
    }

    /// Reports every attribute of the open element that nobody asked for.
    pub(crate) fn attrs_done(&mut self) {
        if self.st.attrs.iter().all(|a| a.taken) {
            return;
        }
        let offset = self.offset();
        for i in 0..self.st.attrs.len() {
            if !self.st.attrs[i].taken {
                self.st.attrs[i].taken = true;
                let name = match &self.st.attrs[i].unknown {
                    Some(name) => quote(name),
                    None => quote(self.st.tables.name(self.st.attrs[i].id)),
                };
                self.st.warn(WarningKind::UnknownAttribute, offset, name);
            }
        }
    }

    // ---- values of the open element -----------------------------------------------------------

    /// Collects the text of the open element into `self.text`, up to its end tag. The element stays
    /// on the path so that the caller can report on it.
    fn read_text(&mut self) -> Result<(), Error> {
        self.text.clear();
        loop {
            self.buf.clear();
            self.rd.get_mut().left = self.token_max;
            let event = match self.rd.read_event_into(&mut self.buf) {
                Ok(event) => event,
                Err(e) => return Err(xml_error(&self.st, error_offset(&self.rd), e)),
            };
            let offset = self.rd.buffer_position();
            match event {
                Event::Text(t) => self.text.push_str(&t.xml10_content()),
                Event::CData(t) => self.text.push_str(&t.xml10_content()),
                Event::GeneralRef(r) => match r.resolve_char_ref() {
                    Ok(Some(c)) => self.text.push(c),
                    Ok(None) => match &*r {
                        "amp" => self.text.push('&'),
                        "lt" => self.text.push('<'),
                        "gt" => self.text.push('>'),
                        "apos" => self.text.push('\''),
                        "quot" => self.text.push('"'),
                        name => {
                            return Err(self.st.error(
                                ErrorKind::DoctypeNotAllowed,
                                offset,
                                "a predefined entity or a character reference",
                                quote(&format!("&{name};")),
                            ));
                        }
                    },
                    Err(e) => return Err(xml_error(&self.st, offset, e)),
                },
                Event::Start(start) => {
                    // An element where only text is allowed: skip it, keep the text.
                    let name = start.local_name().as_ref().to_owned();
                    self.st.push(&name);
                    self.st.warn(
                        WarningKind::UnknownElement,
                        offset,
                        format!("{} inside a simple value", quote(&name)),
                    );
                    self.skip()?;
                }
                Event::End(_) => return Ok(()),
                Event::Eof => {
                    return Err(self.st.error(
                        ErrorKind::Syntax,
                        offset,
                        format!("the end tag of {}", quote(self.st.local())),
                        "the end of the input",
                    ));
                }
                Event::DocType(_) => {
                    return Err(self.st.error(
                        ErrorKind::DoctypeNotAllowed,
                        offset,
                        "no document type declaration",
                        "<!DOCTYPE",
                    ));
                }
                _ => {}
            }
            if self.text.len() > self.st.limits.max_text_len {
                return Err(self.st.error(
                    ErrorKind::LimitExceeded,
                    offset,
                    format!(
                        "text of at most {} bytes (max_text_len)",
                        self.st.limits.max_text_len
                    ),
                    "a longer text",
                ));
            }
        }
    }

    /// Reads the open element as text. An empty element gives an empty string.
    pub(crate) fn string(&mut self) -> Result<Option<String>, Error> {
        self.attrs_done();
        self.read_text()?;
        self.st.pop();
        Ok(Some(self.text.clone()))
    }

    fn parse_text<T: FromText>(&mut self) -> Result<Option<T>, Error> {
        self.attrs_done();
        self.read_text()?;
        let text = self.text.trim();
        if text.is_empty() {
            let offset = self.offset();
            self.st.warn(
                WarningKind::EmptyValue,
                offset,
                format!("expected {}", T::EXPECTED),
            );
            return Ok(None);
        }
        let value = T::from_text(text);
        if value.is_none() {
            let detail = format!("expected {}, found {}", T::EXPECTED, quote(text));
            let offset = self.offset();
            self.st.warn(WarningKind::InvalidValue, offset, detail);
        }
        Ok(value)
    }

    /// Reads the open element as a value. An empty element is reported and gives `None`.
    pub(crate) fn value<T: FromText>(&mut self) -> Result<Option<T>, Error> {
        let value = self.parse_text::<T>();
        self.st.pop();
        value
    }

    pub(crate) fn date_time(&mut self) -> Result<Option<DateTime>, Error> {
        let value = self.parse_text::<DateTime>();
        if let Ok(Some(DateTime {
            offset_minutes: None,
            ..
        })) = value
        {
            let detail = quote(self.text.trim());
            self.warn(WarningKind::NoTimeZone, detail);
        }
        self.st.pop();
        value
    }

    pub(crate) fn base64(&mut self) -> Result<Option<Vec<u8>>, Error> {
        self.attrs_done();
        self.read_text()?;
        let value = base64(&self.text);
        if value.is_none() {
            let detail = format!("expected base64 text, found {}", quote(&self.text));
            self.warn(WarningKind::InvalidValue, detail);
        }
        self.st.pop();
        Ok(value)
    }

    fn enum_from<E: Enumeration>(&mut self, literal: &str, extended: Option<String>) -> E {
        if literal == "_extended" {
            if extended.is_none() {
                self.warn(
                    WarningKind::MissingAttribute,
                    "\"_extendedValue\" on an extended enumeration value".to_owned(),
                );
            }
            return E::extended(extended.unwrap_or_default());
        }
        if let Some(v) = E::from_literal(literal) {
            if let Some(extra) = extended {
                self.warn(
                    WarningKind::UnknownAttribute,
                    format!(
                        "\"_extendedValue\" {} beside the standard value {}",
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

    /// Reads the open element as an enumeration value.
    pub(crate) fn enumeration<E: Enumeration>(&mut self) -> Result<Option<E>, Error> {
        let extended = self.st.take_attr(self.st.ext_value_id);
        self.attrs_done();
        self.read_text()?;
        let literal = std::mem::take(&mut self.text);
        let literal_trimmed = literal.trim();
        let value = if literal_trimmed.is_empty() {
            self.warn(
                WarningKind::EmptyValue,
                "expected an enumeration value".to_owned(),
            );
            None
        } else {
            Some(self.enum_from(literal_trimmed, extended))
        };
        self.text = literal;
        self.st.pop();
        Ok(value)
    }

    /// Reads the open element as a multilingual string: `values/value[@lang]`.
    pub(crate) fn multilingual(&mut self) -> Result<Option<MultilingualString>, Error> {
        self.attrs_done();
        let mut out = MultilingualString::default();
        while let Some(key) = self.next_child()? {
            if key & 0xFFFF != self.st.tables.name_id("values") {
                self.skip_unknown(key)?;
                continue;
            }
            self.attrs_done();
            while let Some(key) = self.next_child()? {
                if key & 0xFFFF != self.st.value_id {
                    self.skip_unknown(key)?;
                    continue;
                }
                let lang = self.req_attr(self.st.lang_id).unwrap_or_default();
                self.attrs_done();
                self.read_text()?;
                self.st.pop();
                out.values.push(LangString {
                    lang,
                    text: self.text.clone(),
                });
            }
            self.close();
        }
        self.close();
        Ok(Some(out))
    }

    /// Keeps the open element, tags included, as raw XML.
    pub(crate) fn raw_element(&mut self) -> Result<Extension, Error> {
        let tag = self.st.open_tag.clone();
        let name = tag
            .split(|c: char| c.is_ascii_whitespace())
            .next()
            .unwrap_or_default()
            .to_owned();
        // The element's own tag counts when deciding which bindings the content uses.
        let mut ext = self.raw_from(format!("<{tag}>"))?;
        ext.xml.push_str("</");
        ext.xml.push_str(&name);
        ext.xml.push('>');
        // A whole element can carry its own declarations, which makes it independent of the
        // document around it: it can be parsed alone and written back unchanged.
        let declarations = ext.declarations(|name| tag.contains(&format!("{name}=")));
        ext.xml.insert_str(1 + name.len(), &declarations);
        ext.namespaces.clear();
        Ok(ext)
    }

    /// Keeps the content of the open element as raw XML.
    pub(crate) fn raw(&mut self) -> Result<Option<Extension>, Error> {
        self.raw_from(String::new()).map(Some)
    }

    /// Captures the rest of the open element as raw XML after `xml`.
    fn raw_from(&mut self, mut xml: String) -> Result<Extension, Error> {
        let namespaces: Vec<(String, String)> = self
            .rd
            .resolver()
            .bindings()
            .map(|(prefix, ns)| {
                let prefix = match prefix {
                    quick_xml::name::PrefixDeclaration::Default => String::new(),
                    quick_xml::name::PrefixDeclaration::Named(p) => p.to_owned(),
                };
                (prefix, ns.0.to_owned())
            })
            .collect();
        let mut depth = 0usize;
        loop {
            self.buf.clear();
            self.rd.get_mut().left = self.token_max;
            let event = match self.rd.read_event_into(&mut self.buf) {
                Ok(event) => event,
                Err(e) => return Err(xml_error(&self.st, error_offset(&self.rd), e)),
            };
            let offset = self.rd.buffer_position();
            match event {
                Event::Start(start) => {
                    depth += 1;
                    if self.st.marks.len() + depth > self.st.limits.max_depth {
                        return Err(self.st.error(
                            ErrorKind::LimitExceeded,
                            offset,
                            format!(
                                "nesting of at most {} elements (max_depth)",
                                self.st.limits.max_depth
                            ),
                            "a deeper element",
                        ));
                    }
                    xml.push('<');
                    xml.push_str(&start);
                    xml.push('>');
                }
                Event::End(_) if depth == 0 => break,
                Event::End(end) => {
                    depth -= 1;
                    xml.push_str("</");
                    xml.push_str(&end);
                    xml.push('>');
                }
                Event::Text(t) => xml.push_str(&t),
                Event::CData(t) => {
                    xml.push_str("<![CDATA[");
                    xml.push_str(&t);
                    xml.push_str("]]>");
                }
                Event::GeneralRef(r) => {
                    if !r.is_char_ref() && !matches!(&*r, "amp" | "lt" | "gt" | "apos" | "quot") {
                        return Err(self.st.error(
                            ErrorKind::DoctypeNotAllowed,
                            offset,
                            "a predefined entity or a character reference",
                            quote(&format!("&{};", &*r)),
                        ));
                    }
                    xml.push('&');
                    xml.push_str(&r);
                    xml.push(';');
                }
                Event::Eof => {
                    return Err(self.st.error(
                        ErrorKind::Syntax,
                        offset,
                        format!("the end tag of {}", quote(self.st.local())),
                        "the end of the input",
                    ));
                }
                Event::DocType(_) => {
                    return Err(self.st.error(
                        ErrorKind::DoctypeNotAllowed,
                        offset,
                        "no document type declaration",
                        "<!DOCTYPE",
                    ));
                }
                _ => {}
            }
            if xml.len() > self.st.limits.max_text_len {
                return Err(self.st.error(
                    ErrorKind::LimitExceeded,
                    offset,
                    format!(
                        "extension content of at most {} bytes (max_text_len)",
                        self.st.limits.max_text_len
                    ),
                    "more",
                ));
            }
        }
        self.st.pop();
        // Only the bindings the content uses are kept: they are what a later parse or write needs,
        // and they do not change when the surrounding document is written with other prefixes.
        let namespaces: Vec<(String, String)> = namespaces
            .into_iter()
            .filter(|(prefix, uri): &(String, String)| match prefix.as_str() {
                "xml" | "xmlns" => false,
                "" => !uri.is_empty(),
                p => xml.contains(&format!("{p}:")),
            })
            .collect();
        Ok(Extension { xml, namespaces })
    }

    // ---- assembling a struct ------------------------------------------------------------------

    /// Stores a value read for a field that may occur once; a second occurrence is reported.
    pub(crate) fn set<T>(&mut self, slot: &mut Option<T>, value: Option<T>, name: u32) {
        if value.is_none() {
            return;
        }
        if slot.is_some() {
            let detail = quote(self.st.tables.name(name));
            self.warn(WarningKind::Repeated, detail);
        } else {
            *slot = value;
        }
    }

    /// Reports a field the schema requires when the element that holds it had none. Real feeds
    /// omit required content, so this is not an error and no value is made up.
    pub(crate) fn required<T>(&mut self, slot: &Option<T>, name: u32) {
        if slot.is_none() {
            let detail = format!("required element {}", quote(self.st.tables.name(name)));
            self.warn(WarningKind::MissingElement, detail);
        }
    }
}

/// Room beyond `max_text_len` for the markup around a text: a start tag and its attributes.
const TOKEN_SLACK: usize = 64 * 1024;

/// The parser reads a whole tag or text run into its buffer before anyone can look at it. This
/// reader stops handing out bytes when one such token grows past its bound, so that the buffer is
/// bounded too.
struct Bounded<'a> {
    inner: Box<dyn BufRead + 'a>,
    /// Bytes the current token may still take; reset before each event.
    left: usize,
}

#[derive(Debug)]
struct TokenTooLong;

impl std::fmt::Display for TokenTooLong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a tag or text exceeds the length limit")
    }
}

impl std::error::Error for TokenTooLong {}

impl std::io::Read for Bounded<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(out.len());
        out[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl BufRead for Bounded<'_> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.left == 0 {
            return Err(std::io::Error::other(TokenTooLong));
        }
        let available = self.inner.fill_buf()?;
        Ok(&available[..available.len().min(self.left)])
    }

    fn consume(&mut self, amount: usize) {
        self.left = self.left.saturating_sub(amount);
        self.inner.consume(amount);
    }
}

/// Copies every byte the parser consumes, so that the beginning of a stream can be read twice.
pub(crate) struct Recorder<'a> {
    pub(crate) inner: &'a mut dyn BufRead,
    pub(crate) seen: Vec<u8>,
}

impl std::io::Read for Recorder<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(out)?;
        self.seen.extend_from_slice(&out[..n]);
        Ok(n)
    }
}

impl BufRead for Recorder<'_> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.inner.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        if let Ok(buf) = self.inner.fill_buf() {
            self.seen.extend_from_slice(&buf[..amount.min(buf.len())]);
        }
        self.inner.consume(amount);
    }
}
