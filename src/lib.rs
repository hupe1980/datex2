//! An unofficial Rust implementation of [DATEX II](https://datex2.eu/), the European standard for
//! road traffic and travel information.
//!
//! This release reads DATEX II **version 3 XML** into typed data, and writes it. To read, one call
//! is enough:
//!
//! ```
//! let xml = br#"<d2:payload xmlns:d2="http://datex2.eu/schema/3/d2Payload"
//!     xmlns:com="http://datex2.eu/schema/3/common" xmlns:sit="http://datex2.eu/schema/3/situation"
//!     xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
//!     xsi:type="sit:SituationPublication" lang="en" modelBaseVersion="3">
//!   <com:publicationTime>2026-11-19T12:00:00Z</com:publicationTime>
//!   <com:publicationCreator>
//!     <com:country>NL</com:country>
//!     <com:nationalIdentifier>EXAMPLE</com:nationalIdentifier>
//!   </com:publicationCreator>
//! </d2:payload>"#;
//!
//! let doc = datex2::v3::read(&xml[..])?;
//! for publication in &doc.publications {
//!     println!("published at {:?}", publication.publication_time());
//!     if let datex2::v3::Publication::Situation(p) = publication {
//!         println!("{} situations", p.situation.len());
//!     }
//! }
//! # Ok::<(), datex2::Error>(())
//! ```
//!
//! # What the reader does for you
//!
//! - **No setup.** The envelope (`payload`, `messageContainer`, a bare publication, SOAP around any
//!   of them), namespace prefixes and the publication type are detected.
//! - **Tolerant, and it tells you.** Real feeds deviate from the standard. The reader accepts the
//!   deviations it knows and lists each one in [`v3::Document::warnings`]. Nothing is dropped
//!   silently and no value is invented.
//! - **Errors you can act on.** An [`Error`] names the element path, the byte offset, what was
//!   expected and what was found.
//! - **Safe on untrusted input.** [`Limits`] bound nesting, attributes and text; document type
//!   declarations are refused. The [threat model](threat_model) lists what is defended against
//!   and what is not.
//!
//! # Large files
//!
//! [`v3::stream`] delivers the records of a document one at a time and keeps only one in memory.
//!
//! # Writing
//!
//! [`v3::write`] turns a publication into a document the official schema accepts, or says which
//! rule of the schema the publication breaks.
//!
//! # What it does not do
//!
//! JSON, DATEX II version 2, and any network or file access. The crate reads the bytes you give
//! it and writes to the sink you give it.
//!
//! This crate is not affiliated with or endorsed by the DATEX II organisation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod out;
mod types;
mod xml;

pub mod threat_model;
pub mod v3;

pub use error::{Error, ErrorKind, Warning, WarningKind};
pub use types::{
    Date, DateTime, Decimal, Extension, LangString, MultilingualString, ParseValueError, Time,
};
pub use xml::Limits;

/// The examples in the README and on the documentation site are compiled, and where they can
/// be, run, with the documentation tests.
#[cfg(doctest)]
mod documentation {
    macro_rules! pages {
        ($($name:ident => $path:literal),* $(,)?) => {$(
            #[doc = include_str!($path)]
            #[allow(dead_code)]
            struct $name;
        )*};
    }
    pages! {
        Readme => "../README.md",
        Home => "../site/content/_index.md",
        GettingStarted => "../site/content/docs/getting-started.md",
        Datex2 => "../site/content/docs/datex2.md",
        Reading => "../site/content/docs/reading.md",
        Streaming => "../site/content/docs/streaming.md",
        Writing => "../site/content/docs/writing.md",
        Model => "../site/content/docs/model.md",
        Tolerance => "../site/content/docs/tolerance.md",
        Errors => "../site/content/docs/errors.md",
        Security => "../site/content/docs/security.md",
        Performance => "../site/content/docs/performance.md",
        Codegen => "../site/content/docs/codegen.md",
    }
}
