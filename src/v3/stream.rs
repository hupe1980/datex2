//! Reading a document one record at a time. The whole-document read is built on this too, so both
//! accept the same envelopes and report the same deviations.

use std::io::{BufRead, Cursor, Read};
use std::marker::PhantomData;

use super::Envelope;
use super::generated::{
    NS_CIS_INFORMATION, NS_D2_PAYLOAD, NS_EXCHANGE_INFORMATION, NS_INFORMATION_MANAGEMENT,
    NS_MESSAGE_CONTAINER, PUBLICATION_CHILDREN, PUBLICATION_TYPES, TABLES,
};
use super::record::Record;
use crate::error::{Error, ErrorKind, Warning, WarningKind, quote};
use crate::xml::{Limits, NS_FOREIGN, NS_NONE, NS_SOAP11, NS_SOAP12, Recorder, Xml};

/// How deep a DATEX II element may sit inside a SOAP body.
const SOAP_WRAPPER_DEPTH: u32 = 4;

/// One open element the stream is inside of.
enum Level {
    /// A SOAP element at the given depth: looking for DATEX II content among its children.
    Soap(u32),
    /// A `messageContainer`: looking for payloads.
    Container,
    /// Inside a publication, `depth` elements down the routes that are still possible.
    Route {
        routes: Vec<&'static [u32]>,
        depth: usize,
    },
}

/// The records of one document, read on demand.
///
/// Created by [`stream`](super::stream). It is an [`Iterator`] over `Result<T, Error>`: each step
/// reads as far as the next record and no further, so memory stays at one record however large
/// the document is. After an error, and after the last record, it yields nothing more.
///
/// Content of the publication outside the records (its time, its header, table identifiers) is
/// passed over. Deviations the reader tolerates are collected as in a whole-document read; take
/// them with [`take_warnings`](Stream::take_warnings).
pub struct Stream<'a, T> {
    xml: Xml<'a>,
    stack: Vec<Level>,
    inferred: Option<u32>,
    done: bool,
    pub(super) envelope: Envelope,
    pub(super) soap: bool,
    /// Whether any payload was seen; a SOAP message without one is not DATEX II.
    pub(super) payloads: usize,
    exchange_information: Option<Box<super::exchange_information::ExchangeInformation>>,
    record: PhantomData<fn() -> T>,
}

impl<T> std::fmt::Debug for Stream<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stream")
            .field("envelope", &self.envelope)
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

fn is_payload(xml: &Xml<'_>, key: u32, ns: u32) -> bool {
    key >> 16 == ns && xml.local() == "payload"
}

/// Looks at the start of the document; if its root is a payload without `xsi:type`, returns the
/// publication type that its first telling child belongs to.
fn infer_publication(input: &mut dyn BufRead, limits: &Limits) -> Option<u32> {
    let mut xml = Xml::new(Box::new(input), &TABLES, limits);
    let key = xml.next_child().ok()??;
    if !is_payload(&xml, key, NS_D2_PAYLOAD) || xml.xsi_type().is_some() {
        return None;
    }
    while let Some(child) = xml.next_child().ok()? {
        if let Some((_, publication)) = PUBLICATION_CHILDREN.iter().find(|(c, _)| *c == child) {
            return Some(*publication);
        }
        // The same local name in another namespace still tells, if only one publication has it.
        let mut owners = PUBLICATION_CHILDREN
            .iter()
            .filter(|(c, _)| c & 0xFFFF == child & 0xFFFF)
            .map(|(_, p)| *p);
        if let Some(first) = owners.next() {
            return owners.all(|p| p == first).then_some(first);
        }
        xml.skip().ok()?;
    }
    None
}

impl<'a, T: Record> Stream<'a, T> {
    pub(super) fn new(input: impl BufRead + 'a, limits: &Limits) -> Result<Self, Error> {
        // A payload without `xsi:type` can only be typed from its content, which comes after the
        // point where the type is needed. So the beginning of the input is read twice: once to
        // look, then again, from a copy, to read.
        let mut input: Box<dyn BufRead + 'a> = Box::new(input);
        let mut recorder = Recorder {
            inner: &mut *input,
            seen: Vec::new(),
        };
        let inferred = infer_publication(&mut recorder, limits);
        let seen = recorder.seen;
        let xml = Xml::new(Box::new(Cursor::new(seen).chain(input)), &TABLES, limits);

        let mut stream = Stream {
            xml,
            stack: Vec::new(),
            inferred,
            done: false,
            envelope: Envelope::Payload,
            soap: false,
            payloads: 0,
            exchange_information: None,
            record: PhantomData,
        };
        // The root element is read now, so that input which is not DATEX II fails here.
        match stream.xml.next_child()? {
            Some(key) => stream.open_root(key)?,
            None => {
                return Err(stream
                    .xml
                    .error(ErrorKind::Syntax, "a root element", "an end tag"));
            }
        }
        Ok(stream)
    }

    /// The warnings collected since the last call, or since the stream was created.
    ///
    /// Taking them regularly keeps a long stream from reaching [`Limits::max_warnings`], beyond
    /// which warnings are only counted.
    pub fn take_warnings(&mut self) -> Vec<Warning> {
        self.xml.take_warnings()
    }

    /// The exchange information of a message container, once the stream has read it. It says,
    /// among other things, whether a delivery is a complete snapshot or a delta
    /// (`exchange_context.coded_exchange_protocol`). In a container that puts it after its payloads,
    /// it is available after the last record.
    pub fn exchange_information(
        &self,
    ) -> Option<&super::exchange_information::ExchangeInformation> {
        self.exchange_information.as_deref()
    }

    pub(super) fn take_exchange_information(
        &mut self,
    ) -> Option<Box<super::exchange_information::ExchangeInformation>> {
        self.exchange_information.take()
    }

    /// How many warnings were not kept because the list was full.
    pub fn suppressed_warnings(&self) -> usize {
        self.xml.suppressed()
    }

    /// Decides what the root element is. A record that is the root itself is left open for `next`.
    fn open_root(&mut self, key: u32) -> Result<(), Error> {
        let ns = key >> 16;
        let unqualified = is_payload(&self.xml, key, NS_NONE)
            && self
                .xml
                .xsi_type()
                .is_some_and(|t| PUBLICATION_TYPES.contains(&t));
        if is_payload(&self.xml, key, NS_D2_PAYLOAD) || unqualified {
            if unqualified {
                self.xml.warn(
                    WarningKind::WrongNamespace,
                    "\"payload\" in no namespace, expected in d2Payload".to_owned(),
                );
            }
            self.envelope = Envelope::Payload;
            let inferred = self.inferred;
            return self.open_publication(inferred, true);
        }
        if ns == NS_MESSAGE_CONTAINER && self.xml.local() == "messageContainer" {
            self.envelope = Envelope::MessageContainer;
            self.stack.push(Level::Container);
            return Ok(());
        }
        if matches!(ns, NS_SOAP11 | NS_SOAP12) && self.xml.local() == "Envelope" {
            self.soap = true;
            self.stack.push(Level::Soap(0));
            return Ok(());
        }
        // An element named after a publication type.
        let named = PUBLICATION_TYPES
            .iter()
            .copied()
            .filter(|t| TABLES.names[(t & 0xFFFF) as usize] == self.xml.local());
        let kind = named.clone().find(|t| *t == key).or_else(|| {
            let mut named = named;
            let first = named.next()?;
            named.next().is_none().then_some(first)
        });
        if let Some(kind) = kind.filter(|_| ns != NS_FOREIGN) {
            self.envelope = Envelope::Publication;
            return self.enter_publication(kind, true);
        }
        let hint = if self.xml.local() == "d2LogicalModel" {
            " (DATEX II version 1 or 2, which this reader does not support)"
        } else {
            ""
        };
        Err(self.xml.error(
            ErrorKind::NotDatex2,
            "a DATEX II v3 payload, messageContainer or publication",
            format!("element {}{hint}", self.xml.describe_current(key)),
        ))
    }

    /// The open element is a payload: finds its publication type and enters it.
    fn open_publication(&mut self, inferred: Option<u32>, at_root: bool) -> Result<(), Error> {
        let kind = match self.xml.xsi_type() {
            Some(kind) if PUBLICATION_TYPES.contains(&kind) => kind,
            Some(_) => {
                return Err(self.xml.error(
                    ErrorKind::UnknownPublication,
                    "a DATEX II v3 publication type in xsi:type",
                    quote(self.xml.xsi_raw()),
                ));
            }
            None => match inferred {
                Some(kind) => {
                    self.xml.warn(
                        WarningKind::MissingXsiType,
                        format!(
                            "no xsi:type on the payload, read as {}",
                            TABLES.names[(kind & 0xFFFF) as usize]
                        ),
                    );
                    kind
                }
                None => {
                    return Err(self.xml.error(
                        ErrorKind::UnknownPublication,
                        "an xsi:type attribute naming the publication type",
                        "none, and the content does not identify one",
                    ));
                }
            },
        };
        self.enter_publication(kind, at_root)
    }

    /// The open element is a publication of type `kind`: selects the routes to `T` in it.
    fn enter_publication(&mut self, kind: u32, at_root: bool) -> Result<(), Error> {
        self.payloads += 1;
        let routes: Vec<&'static [u32]> = T::ROUTES
            .iter()
            .filter(|(publication, _)| *publication == kind)
            .map(|(_, route)| *route)
            .collect();
        if routes.is_empty() {
            let name = TABLES.names[(kind & 0xFFFF) as usize];
            if at_root {
                return Err(self.xml.error(
                    ErrorKind::UnexpectedPublication,
                    format!("a publication that holds {} records", T::NAME),
                    name,
                ));
            }
            // One payload among several: say so and go on with the others.
            self.xml.warn(
                WarningKind::UnexpectedPublication,
                format!("{name} holds no {} records", T::NAME),
            );
            return self.xml.skip();
        }
        // An empty route means the publication itself is the record; `kind` is kept for reading it.
        self.stack.push(Level::Route { routes, depth: 0 });
        self.inferred = Some(kind);
        Ok(())
    }

    /// Reads on to the next record. `Ok(None)` is the end of the document.
    fn advance(&mut self) -> Result<Option<T>, Error> {
        loop {
            let Some(level) = self.stack.last() else {
                return Ok(None);
            };
            // A publication that is itself the record is read as soon as it has been entered.
            if let Level::Route { routes, depth: 0 } = level {
                if routes.iter().any(|r| r.is_empty()) {
                    self.stack.pop();
                    match T::read_record(&mut self.xml, self.inferred)? {
                        Some(record) => return Ok(Some(record)),
                        None => continue,
                    }
                }
            }
            let Some(key) = self.xml.next_child()? else {
                self.xml.close();
                self.stack.pop();
                continue;
            };
            let ns = key >> 16;
            match self.stack.last().expect("checked above") {
                Level::Soap(depth) => {
                    let depth = *depth;
                    if is_payload(&self.xml, key, NS_D2_PAYLOAD) {
                        self.open_publication(None, false)?;
                    } else if ns == NS_MESSAGE_CONTAINER && self.xml.local() == "messageContainer" {
                        self.envelope = Envelope::MessageContainer;
                        self.stack.push(Level::Container);
                    } else if depth < SOAP_WRAPPER_DEPTH {
                        // Header, Body and whatever operation element wraps the content.
                        self.stack.push(Level::Soap(depth + 1));
                    } else {
                        self.xml.skip_silently()?;
                    }
                }
                Level::Container => {
                    if is_payload(&self.xml, key, NS_MESSAGE_CONTAINER) {
                        self.open_publication(None, false)?;
                    } else if ns == NS_MESSAGE_CONTAINER
                        && self.xml.local() == "exchangeInformation"
                    {
                        // Kept for the caller: it says whether a delivery is complete or a delta.
                        let info =
                            super::exchange_information::ExchangeInformation::read(&mut self.xml)?;
                        self.exchange_information = Some(Box::new(info));
                    } else if matches!(
                        ns,
                        NS_MESSAGE_CONTAINER
                            | NS_EXCHANGE_INFORMATION
                            | NS_INFORMATION_MANAGEMENT
                            | NS_CIS_INFORMATION
                    ) {
                        // Other exchange metadata is not interpreted.
                        self.xml.skip_silently()?;
                    } else {
                        self.xml.skip_unknown(key)?;
                    }
                }
                Level::Route { routes, depth } => {
                    let depth = *depth;
                    let mut next: Vec<&'static [u32]> = routes
                        .iter()
                        .copied()
                        .filter(|r| r.get(depth) == Some(&key))
                        .collect();
                    if next.is_empty() {
                        // The same tolerance as inside a record: a known name in another DATEX II namespace.
                        let expected: Vec<u32> = routes
                            .iter()
                            .filter_map(|r| r.get(depth).copied())
                            .collect();
                        if let Some(found) = self.xml.fallback(key, &expected) {
                            next = routes
                                .iter()
                                .copied()
                                .filter(|r| r.get(depth) == Some(&found))
                                .collect();
                        }
                    }
                    if next.iter().any(|r| r.len() == depth + 1) {
                        if let Some(record) = T::read_record(&mut self.xml, None)? {
                            return Ok(Some(record));
                        }
                    } else if next.is_empty() {
                        // Content of the publication that is not on the way to a record.
                        self.xml.skip_silently()?;
                    } else {
                        self.stack.push(Level::Route {
                            routes: next,
                            depth: depth + 1,
                        });
                    }
                }
            }
        }
    }
}

impl<T: Record> Iterator for Stream<'_, T> {
    type Item = Result<T, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.advance() {
            Ok(Some(record)) => Some(Ok(record)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

impl<T: Record> std::iter::FusedIterator for Stream<'_, T> {}
