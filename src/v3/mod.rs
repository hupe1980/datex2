//! DATEX II version 3.
//!
//! [`read`] takes any DATEX II v3 XML document and returns a [`Document`]. The types are generated
//! from the official 3.7 schemas, one module per namespace; they read documents of every 3.x
//! release, because a document does not say which minor release it follows and stable namespaces
//! only grow. [`json`] reads and writes the same types as JSON.

use std::io::BufRead;

use crate::error::{Error, ErrorKind, Warning};
use crate::xml::Limits;

#[rustfmt::skip]
mod generated;
pub mod json;
mod record;
mod stream;

pub use generated::*;
pub use record::Record;
pub use stream::Stream;

/// A publication of any kind: the payload of a DATEX II v3 document.
pub use generated::common::PayloadPublication as Publication;

/// What a writer puts at the top of a document: a publication, written as a `payload`, or a
/// message container, written as a `messageContainer`.
///
/// Every writer takes `impl Into<Root>`, so you pass `&publication` or `&container` and never
/// name this type:
///
/// ```
/// use datex2::v3::{self, json, Publication, message_container::MessageContainer};
///
/// fn both(publication: &Publication, container: &MessageContainer) -> Result<(), datex2::Error> {
///     let xml = v3::to_string(publication)?;
///     let json = json::to_string(container)?;
///     # let _ = (xml, json);
///     Ok(())
/// }
/// ```
///
/// Nothing else is a root:
///
/// ```compile_fail
/// let site = datex2::v3::energy_infrastructure::EnergyInfrastructureSite::default();
/// datex2::v3::to_string(&site);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Root<'a> {
    /// A publication, written as `payload`.
    Publication(&'a Publication),
    /// A message container: publications with exchange information.
    Container(&'a message_container::MessageContainer),
}

impl<'a> From<&'a Publication> for Root<'a> {
    fn from(publication: &'a Publication) -> Self {
        Self::Publication(publication)
    }
}

impl<'a> From<&'a message_container::MessageContainer> for Root<'a> {
    fn from(container: &'a message_container::MessageContainer) -> Self {
        Self::Container(container)
    }
}

/// The namespaces of a message container and its exchange parts. Each payload declares its own.
const CONTAINER_NAMESPACES: &str = concat!(
    " xmlns:con=\"http://datex2.eu/schema/3/messageContainer\"",
    " xmlns:ex=\"http://datex2.eu/schema/3/exchangeInformation\"",
    " xmlns:cis=\"http://datex2.eu/schema/3/cisInformation\"",
    " xmlns:inf=\"http://datex2.eu/schema/3/informationManagement\"",
    " xmlns:com=\"http://datex2.eu/schema/3/common\"",
    " xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"",
);

/// The outermost DATEX II element of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Envelope {
    /// A `payload` element: one publication.
    Payload,
    /// A `messageContainer` of Exchange 2020: any number of publications.
    MessageContainer,
    /// An element named after the publication type, as some portals publish.
    Publication,
}

/// The result of reading one document.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Document {
    /// The publications found, in document order. A `messageContainer` may hold none or several.
    pub publications: Vec<Publication>,
    /// The shape of the document.
    pub envelope: Envelope,
    /// Whether the document was wrapped in a SOAP envelope.
    pub soap: bool,
    /// Every deviation the reader tolerated, up to [`Limits::max_warnings`].
    pub warnings: Vec<Warning>,
    /// How many further warnings were not kept.
    pub suppressed_warnings: usize,
    /// The exchange information of a message container: among other things, whether the
    /// delivery is a complete snapshot or a delta (`exchange_context.coded_exchange_protocol`).
    pub exchange_information: Option<Box<exchange_information::ExchangeInformation>>,
}

/// Reads a DATEX II v3 XML document.
///
/// `input` is anything buffered: a byte slice, or a file in a [`BufReader`](std::io::BufReader).
/// No version, namespace or envelope has to be stated.
///
/// ```no_run
/// let file = std::io::BufReader::new(std::fs::File::open("incidents.xml")?);
/// let doc = datex2::v3::read(file)?;
/// println!("{} publications, {} warnings", doc.publications.len(), doc.warnings.len());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
///
/// Fails when the input is not well-formed XML, is not DATEX II v3, lacks required content,
/// holds a value that cannot be read, or exceeds a default [`Limits`] value. Deviations the reader
/// tolerates are not errors; they are listed in [`Document::warnings`].
pub fn read(input: impl BufRead) -> Result<Document, Error> {
    read_with(input, &Limits::default())
}

/// Reads a DATEX II v3 XML document with the given [`Limits`].
///
/// ```
/// let mut limits = datex2::Limits::default();
/// limits.max_depth = 20;
/// let result = datex2::v3::read_with(&b"<a/>"[..], &limits);
/// assert!(result.is_err()); // not DATEX II
/// ```
///
/// # Errors
///
/// As for [`read`], with the limits given.
pub fn read_with(input: impl BufRead, limits: &Limits) -> Result<Document, Error> {
    // A whole read is a stream of publications, collected.
    let mut stream = Stream::<Publication>::new(input, limits)?;
    let publications = stream.by_ref().collect::<Result<Vec<_>, _>>()?;
    if stream.soap && stream.payloads == 0 {
        return Err(Error {
            kind: ErrorKind::NotDatex2,
            path: String::new(),
            offset: 0,
            expected: "a DATEX II v3 payload or messageContainer in the SOAP body".to_owned(),
            found: "none".to_owned(),
        });
    }
    Ok(Document {
        publications,
        envelope: stream.envelope,
        soap: stream.soap,
        warnings: stream.take_warnings(),
        suppressed_warnings: stream.suppressed_warnings(),
        exchange_information: stream.take_exchange_information(),
    })
}

/// Reads the records of a DATEX II v3 XML document one at a time.
///
/// Name the record type you want; the [`Stream`] delivers every object of that type the
/// publication holds, in document order, and keeps only one in memory. Any type that implements
/// [`Record`] can be asked for.
///
/// ```no_run
/// use datex2::v3::{self, situation::Situation};
///
/// let file = std::io::BufReader::new(std::fs::File::open("roadworks.xml")?);
/// for situation in v3::stream::<Situation>(file)? {
///     let situation = situation?;
///     println!("{:?}: {} records", situation.id, situation.situation_record.len());
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// To see what the reader tolerated, keep the stream and take its warnings:
///
/// ```
/// # let xml = include_str!("../../tests/fixtures/situation.xml");
/// use datex2::v3::{self, situation::SituationRecord};
///
/// let mut records = v3::stream::<SituationRecord>(xml.as_bytes())?;
/// for record in records.by_ref() {
///     println!("{:?}", record?.id());
/// }
/// assert!(records.take_warnings().is_empty());
/// # Ok::<(), datex2::Error>(())
/// ```
///
/// # Errors
///
/// Fails at once when the input is not DATEX II v3, or when its publication cannot hold records
/// of type `T` (situations asked of a VMS publication). Failures further into the document arrive
/// as an item of the stream, after the records before them; the stream then ends.
pub fn stream<'a, T: Record>(input: impl BufRead + 'a) -> Result<Stream<'a, T>, Error> {
    Stream::new(input, &Limits::default())
}

/// Like [`stream`], with the given [`Limits`].
///
/// # Errors
///
/// As for [`stream`].
pub fn stream_with<'a, T: Record>(
    input: impl BufRead + 'a,
    limits: &Limits,
) -> Result<Stream<'a, T>, Error> {
    Stream::new(input, limits)
}

/// Writes a publication or a message container as a DATEX II v3 XML document.
///
/// A publication becomes a `d2:payload` element, a [`MessageContainer`](message_container::MessageContainer)
/// a `con:messageContainer` with exchange information and a `con:payload` per publication, each
/// with the namespace declarations, type attributes and element order the standard requires. When
/// the write succeeds the official schemas (3.7, and Exchange 2020 for a container) accept the
/// output, except for their uniqueness constraints on identifiers, which are not checked.
///
/// ```
/// use datex2::v3::{self, common, situation::SituationPublication, Publication};
///
/// let publication = Publication::Situation(Box::new(SituationPublication {
///     lang: Some("en".into()),
///     publication_time: "2026-11-19T12:00:00Z".parse().ok(),
///     publication_creator: Some(Box::new(common::InternationalIdentifier {
///         country: Some("NL".into()),
///         national_identifier: Some("EXAMPLE".into()),
///         ..Default::default()
///     })),
///     ..Default::default()
/// }));
/// let xml = v3::to_string(&publication)?;
/// assert!(xml.contains("<com:country>NL</com:country>"));
/// # Ok::<(), datex2::Error>(())
/// ```
///
/// # Errors
///
/// The writer is strict. It fails, naming the element and the rule, when content the schema
/// requires is absent, when a list has too few or too many entries, when a value breaks a length,
/// range, digit or pattern rule, when a text holds a character XML cannot carry, when extension
/// content is not well-formed, or when the sink fails. Bytes already written stay in the sink.
pub fn write<'a>(mut sink: impl std::io::Write, root: impl Into<Root<'a>>) -> Result<(), Error> {
    let mut out = crate::out::Out::new(&mut sink);
    out.declaration()?;
    match root.into() {
        Root::Publication(publication) => publication.write(&mut out, "d2:payload"),
        Root::Container(container) => {
            container.write(&mut out, "con:messageContainer", "", CONTAINER_NAMESPACES)
        }
    }
}

/// Writes a publication or a message container as a DATEX II v3 XML document in memory. See
/// [`write()`].
///
/// # Errors
///
/// As for [`write()`].
pub fn to_string<'a>(root: impl Into<Root<'a>>) -> Result<String, Error> {
    let mut bytes = Vec::new();
    write(&mut bytes, root)?;
    // The writer emits only `str` data.
    Ok(String::from_utf8(bytes).expect("the writer produces UTF-8"))
}
