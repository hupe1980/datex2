//! DATEX II version 3.
//!
//! [`read`] takes any DATEX II v3 XML document and returns a [`Document`]. The types are generated
//! from the official 3.7 schemas, one module per namespace; they read documents of every 3.x
//! release, because a document does not say which minor release it follows and stable namespaces
//! only grow.

use std::io::BufRead;

use crate::error::{Error, ErrorKind, Warning};
use crate::xml::Limits;

#[rustfmt::skip]
mod generated;
mod record;
mod stream;

pub use generated::*;
pub use record::Record;
pub use stream::Stream;

/// A publication of any kind: the payload of a DATEX II v3 document.
pub use generated::common::PayloadPublication as Publication;

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

/// Writes a publication as a DATEX II v3 XML document.
///
/// The document is a `d2:payload` element holding the publication, with the namespace
/// declarations, type attributes and element order the standard requires. When the write succeeds
/// the official 3.7 schema accepts the output, except for the schema's uniqueness constraints on
/// identifiers, which are not checked.
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
pub fn write(mut sink: impl std::io::Write, publication: &Publication) -> Result<(), Error> {
    let mut out = crate::out::Out::new(&mut sink);
    out.declaration()?;
    publication.write(&mut out, "d2:payload")
}

/// Writes a publication as a DATEX II v3 XML document in memory. See [`write()`].
///
/// # Errors
///
/// As for [`write()`].
pub fn to_string(publication: &Publication) -> Result<String, Error> {
    let mut bytes = Vec::new();
    write(&mut bytes, publication)?;
    // The writer emits only `str` data.
    Ok(String::from_utf8(bytes).expect("the writer produces UTF-8"))
}
