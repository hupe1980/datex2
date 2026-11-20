//! DATEX II v3 in JSON.
//!
//! The same typed model as XML, in the JSON encoding DATEX II 3.7 defines. Reading takes either
//! spelling in use: the official 3.7 mapping and the German AFIR Recharging profile that the
//! Mobilithek requires. Writing produces one of them: [`write()`] the official mapping,
//! [`mobilithek::write`] the profile.
//!
//! ```
//! use datex2::v3::{self, json};
//!
//! let xml = include_str!("../../tests/fixtures/situation.xml");
//! let doc = v3::read(xml.as_bytes())?;
//! let text = json::to_string(&doc.publications[0])?;
//! assert!(text.starts_with(r#"{"payload":{"modelBaseVersionG":"3","sitSituationPublication":"#));
//!
//! let again = json::read(text.as_bytes())?;
//! assert_eq!(again.publications, doc.publications);
//! # Ok::<(), datex2::Error>(())
//! ```
//!
//! Numbers keep their value exactly: a [`Decimal`](crate::Decimal) is written with its own
//! digits, an `f32` as the shortest text that reads back to the same value. No global JSON setting
//! is involved, and the crate does not depend on a JSON library.

use std::io::{BufRead, Write};

use super::exchange_information::ExchangeInformation;
use super::generated::{JSON, JSON_DEFS};
use super::message_container::MessageContainer;
use super::{Document, Envelope, Publication};
use crate::error::{Error, ErrorKind, WarningKind, quote};
use crate::json::{JsonReader, JsonWriter, Profile};
use crate::xml::Limits;

/// Reads a DATEX II v3 JSON document.
///
/// The root is `{"payload": …}` (one publication, or an array of them) or
/// `{"messageContainer": …}` (publications with exchange information). Both the official 3.7
/// spelling and the Mobilithek profile's spelling are read, without a setting.
///
/// # Errors
///
/// When the input is not well-formed JSON, is not DATEX II v3, names no known publication, or
/// exceeds a [`Limits`] value. Deviations the reader tolerates are listed in
/// [`Document::warnings`].
pub fn read(input: impl BufRead) -> Result<Document, Error> {
    read_with(input, &Limits::default())
}

/// Reads a DATEX II v3 JSON document with the given [`Limits`].
///
/// # Errors
///
/// As for [`read`].
pub fn read_with<'a>(input: impl BufRead + 'a, limits: &Limits) -> Result<Document, Error> {
    let mut r = JsonReader::new(Box::new(input), &JSON, limits);
    if !r.begin_object()? {
        return Err(r.error(ErrorKind::NotDatex2, "a JSON object", "another value"));
    }
    let mut doc = Document {
        publications: Vec::new(),
        envelope: Envelope::Payload,
        soap: false,
        warnings: Vec::new(),
        suppressed_warnings: 0,
        exchange_information: None,
    };
    let mut found = false;
    while let Some(key) = r.next_key()? {
        match r.key_text() {
            "payload" if !found => {
                found = true;
                let mut list = r.list_quietly()?;
                while r.item(&mut list)? {
                    doc.publications.extend(Publication::read_json(&mut r)?);
                }
            }
            "messageContainer" if !found => {
                found = true;
                doc.envelope = Envelope::MessageContainer;
                if let Some(container) = MessageContainer::read_json(&mut r)? {
                    doc.publications = container.payload;
                    doc.exchange_information = container.exchange_information;
                }
            }
            // The profile allows exchange information beside the container, or on its own. Feeds
            // send a copy of the container's; only a different one is worth a warning.
            "exchangeInformation" => {
                doc.envelope = Envelope::MessageContainer;
                let info = ExchangeInformation::read_json(&mut r)?.map(Box::new);
                match &doc.exchange_information {
                    None => doc.exchange_information = info,
                    Some(kept) if info.is_none() || info.as_ref() == Some(kept) => {}
                    Some(_) => r.warn(
                        WarningKind::Repeated,
                        "\"exchangeInformation\" beside the container's, with other content; the first is kept".to_owned(),
                    ),
                }
                found = true;
            }
            _ if found => {
                let _ = key;
                r.skip_unknown()?;
            }
            other => {
                let other = quote(other);
                return Err(r.error(
                    ErrorKind::NotDatex2,
                    "a DATEX II v3 JSON document: {\"payload\": …} or {\"messageContainer\": …}",
                    format!("member {other}"),
                ));
            }
        }
    }
    if !found {
        return Err(r.error(
            ErrorKind::NotDatex2,
            "a member \"payload\" or \"messageContainer\"",
            "none",
        ));
    }
    r.end()?;
    doc.warnings = r.take_warnings();
    doc.suppressed_warnings = r.suppressed();
    Ok(doc)
}

fn write_publication(
    sink: &mut dyn Write,
    publication: &Publication,
    profile: Option<&'static Profile>,
) -> Result<(), Error> {
    let mut w = JsonWriter::new(sink, &JSON, JSON_DEFS, profile);
    w.document_start()?;
    w.root_key("payload")?;
    publication.write_json(&mut w)?;
    w.document_end()
}

fn write_container_with(
    sink: &mut dyn Write,
    container: &MessageContainer,
    profile: Option<&'static Profile>,
) -> Result<(), Error> {
    let mut w = JsonWriter::new(sink, &JSON, JSON_DEFS, profile);
    w.document_start()?;
    w.root_key("messageContainer")?;
    container.write_json(&mut w, None)?;
    w.document_end()
}

/// Writes a publication in the official DATEX II 3.7 JSON encoding: `{"payload": …}`.
///
/// When the write succeeds, the official 3.7 JSON schemas accept the output. The rules are those
/// of the XML writer, plus two that only JSON has: a language code is two lowercase letters, and
/// a date and time carries a time zone.
///
/// # Errors
///
/// When the publication breaks a rule of the schema, holds extension content read from XML, or
/// the sink fails. The error names the member and the rule.
pub fn write(mut sink: impl Write, publication: &Publication) -> Result<(), Error> {
    write_publication(&mut sink, publication, None)
}

/// Writes a publication as official DATEX II 3.7 JSON in memory. See [`write()`].
///
/// # Errors
///
/// As for [`write()`].
pub fn to_string(publication: &Publication) -> Result<String, Error> {
    let mut bytes = Vec::new();
    write(&mut bytes, publication)?;
    Ok(String::from_utf8(bytes).expect("the writer produces UTF-8"))
}

/// Writes a message container in the DATEX II 3.7 JSON mapping: `{"messageContainer": …}`.
///
/// DATEX II publishes no JSON schema for Exchange 2020; each payload follows the 3.7 schemas.
///
/// # Errors
///
/// As for [`write()`].
pub fn write_container(mut sink: impl Write, container: &MessageContainer) -> Result<(), Error> {
    write_container_with(&mut sink, container, None)
}

/// The German AFIR Recharging profile, as the Mobilithek requires it.
///
/// DATEX II profile "AFIR Energy Infrastructure" 01-00-00, revision 2 of 10 April 2026, published
/// by the Mobilithek. Static data (sites, stations, charging points, prices) is a table
/// publication written with [`write()`]; dynamic data is a status publication in a message
/// container written with [`write_container()`], whose exchange information says whether it is a
/// complete snapshot (`snapshotPush`) or a delta (`deltaPush`).
///
/// The profile is DATEX II 3.7 cut down to the AFIR content, but taken from an earlier stage of
/// the model, so a few things are spelled differently. The writer takes care of them:
///
/// | DATEX II 3.7 (the model) | The profile writes |
/// |---|---|
/// | `energyProduct: [{"aegiElectricEnergy": {…}}]` | `electricEnergy: [{…}]` |
/// | `AfirFacilityLocation` in the location extension | `FacilityLocation` |
/// | `fault: {"comFault": {…}}` | `fault: {…}` |
/// | `roamingPlatform` on a site | `roamingPlatfom` |
/// | `regulatedCharacteristics: [{"euVehicleCategory": […]}]` | `euVehicleCategory: […]` |
/// | connector type `domesticFType` | `domesticF` |
/// | a time of day `08:00:00` | `08:00:00Z` |
///
/// Vehicle categories that only the profile lists, such as `l` and `other`, are held as
/// extended values (`EuVehicleCategoryEnum::Extended("l")`) and written back as the profile's
/// literal.
///
/// The profile also allows less than 3.7: fewer members, fewer enumeration values, and some
/// members are required. A write that would produce something the profile's schemas reject fails
/// and names the place and the rule. The 3.7 value rules still apply where the profile's schemas
/// leave them out, so a time zone is an offset such as `+01:00` (not `Europe/Berlin`) and an amount
/// of money has at most two decimal places. When the publication does not set its profile name
/// and version, the profile's are written.
///
/// One member exists only in the profile and cannot be held by the 3.7 model:
/// `refillPointByReference` on a station. It is read as an unknown member, with a warning.
pub mod mobilithek {
    use std::io::Write;

    use super::super::Publication;
    use super::super::generated::MOBILITHEK;
    use super::MessageContainer;
    use crate::Error;

    /// The profile name the writer puts on a publication that sets none.
    pub const PROFILE_NAME: &str = "AFIR Energy Infrastructure";
    /// The profile version the writer puts on a publication that sets none.
    pub const PROFILE_VERSION: &str = "01-00-00";

    /// Writes a publication, normally the table publication of static data, for the Mobilithek.
    ///
    /// # Errors
    ///
    /// When the publication holds something the profile does not allow, breaks a rule of the
    /// schema, or the sink fails. The error names the member and the rule.
    pub fn write(mut sink: impl Write, publication: &Publication) -> Result<(), Error> {
        super::write_publication(&mut sink, publication, Some(&MOBILITHEK))
    }

    /// Writes a publication for the Mobilithek in memory. See [`write()`].
    ///
    /// # Errors
    ///
    /// As for [`write()`].
    pub fn to_string(publication: &Publication) -> Result<String, Error> {
        let mut bytes = Vec::new();
        write(&mut bytes, publication)?;
        Ok(String::from_utf8(bytes).expect("the writer produces UTF-8"))
    }

    /// Writes a message container, normally a status publication of dynamic data, for the
    /// Mobilithek.
    ///
    /// # Errors
    ///
    /// As for [`write()`].
    pub fn write_container(
        mut sink: impl Write,
        container: &MessageContainer,
    ) -> Result<(), Error> {
        super::write_container_with(&mut sink, container, Some(&MOBILITHEK))
    }
}
