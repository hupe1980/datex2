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
use super::{Document, Envelope, Publication, Root};
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

fn write_root(
    sink: &mut dyn Write,
    root: Root<'_>,
    profile: Option<&'static Profile>,
) -> Result<(), Error> {
    let mut w = JsonWriter::new(sink, &JSON, JSON_DEFS, profile);
    w.document_start()?;
    match root {
        Root::Publication(publication) => {
            w.root_key("payload")?;
            publication.write_json(&mut w)?;
        }
        Root::Container(container) => {
            w.root_key("messageContainer")?;
            container.write_json(&mut w, None)?;
        }
    }
    w.document_end()
}

fn root_to_string(root: Root<'_>, profile: Option<&'static Profile>) -> Result<String, Error> {
    let mut bytes = Vec::new();
    write_root(&mut bytes, root, profile)?;
    Ok(String::from_utf8(bytes).expect("the writer produces UTF-8"))
}

/// Writes a publication (`{"payload": …}`) or a message container (`{"messageContainer": …}`) in
/// the official DATEX II 3.7 JSON encoding.
///
/// When the write succeeds, the official 3.7 JSON schemas accept each publication. The rules are
/// those of the XML writer, plus two that only JSON has: a language code is two lowercase letters,
/// and a date and time carries a time zone. For the exchange part of a container, which has no
/// JSON schema, the writer applies the XML schema's rules.
///
/// # Errors
///
/// When the publication or container breaks a rule of the schema, holds extension content read
/// from XML, or the sink fails. The error names the member and the rule.
pub fn write<'a>(mut sink: impl Write, root: impl Into<Root<'a>>) -> Result<(), Error> {
    write_root(&mut sink, root.into(), None)
}

/// Writes a publication or a message container as official DATEX II 3.7 JSON in memory. See
/// [`write()`].
///
/// # Errors
///
/// As for [`write()`].
pub fn to_string<'a>(root: impl Into<Root<'a>>) -> Result<String, Error> {
    root_to_string(root.into(), None)
}

/// The German AFIR Recharging profile, as the Mobilithek requires it.
///
/// DATEX II profile "AFIR Energy Infrastructure" 01-00-00, revision 2 of 10 April 2026, published
/// by the Mobilithek. It has two deliveries:
///
/// - **Static data** (sites, stations, charging points, prices): a table publication, written
///   with [`write()`](mobilithek::write) or [`to_string()`](mobilithek::to_string).
/// - **Dynamic data** (status, current prices): a status publication in a message container.
///   [`snapshot()`](mobilithek::snapshot) and [`delta()`](mobilithek::delta) build the container,
///   the same functions write it.
///
/// ```
/// use datex2::v3::{Publication, json::mobilithek};
///
/// fn deliver(table: &Publication, status: Publication) -> Result<(String, String), datex2::Error> {
///     let generated = "2026-04-14T14:00:00+02:00".parse().expect("a date and time");
///     let static_data = mobilithek::to_string(table)?;
///     let dynamic_data = mobilithek::to_string(&mobilithek::snapshot(status, generated))?;
///     Ok((static_data, dynamic_data))
/// }
/// ```
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
/// and names the place and the rule. When the publication does not set its profile name and
/// version, the profile's are written.
///
/// # Rules the profile's schemas do not show
///
/// The writer applies these value rules of the model although the profile's JSON schemas omit them:
///
/// - **`AmountOfMoney`** (every price, fee and cap): at most 8 digits, 2 after the point. `0.590`
///   is written as `0.590`; `0.589` is refused, never rounded.
/// - **Time zones**: an offset such as `+01:00`, not `Europe/Berlin`.
///
/// One member exists only in the profile and cannot be held by the 3.7 model:
/// `refillPointByReference` on a station. It is read as an unknown member, with a warning.
pub mod mobilithek {
    use std::io::Write;

    use super::super::Publication;
    use super::super::exchange_information::{
        Agent, DynamicInformation, ExchangeContext, ExchangeInformation, ExchangeStatusEnum,
        ProtocolTypeEnum,
    };
    use super::super::generated::MOBILITHEK;
    use super::{MessageContainer, Root};
    use crate::{DateTime, Error};

    /// The profile name the writer puts on a publication that sets none.
    pub const PROFILE_NAME: &str = "AFIR Energy Infrastructure";
    /// The profile version the writer puts on a publication that sets none.
    pub const PROFILE_VERSION: &str = "01-00-00";

    /// Writes a table publication or a status container for the Mobilithek.
    ///
    /// When the write succeeds, the profile's JSON schemas accept the output: the table schema for
    /// a publication, the container schema for a message container.
    ///
    /// # Errors
    ///
    /// When the publication or container holds something the profile does not allow (including a
    /// publication type outside the profile), breaks a rule of the schema, or the sink fails. The
    /// error names the member and the rule.
    pub fn write<'a>(mut sink: impl Write, root: impl Into<Root<'a>>) -> Result<(), Error> {
        super::write_root(&mut sink, root.into(), Some(&MOBILITHEK))
    }

    /// Writes a table publication or a status container for the Mobilithek in memory. See
    /// [`write()`].
    ///
    /// # Errors
    ///
    /// As for [`write()`].
    pub fn to_string<'a>(root: impl Into<Root<'a>>) -> Result<String, Error> {
        super::root_to_string(root.into(), Some(&MOBILITHEK))
    }

    /// Wraps a status publication as a **complete snapshot** (`snapshotPush`) generated at
    /// `generated`.
    ///
    /// A snapshot must hold every charging point of the table; consumers take missing ones as gone.
    /// For only the points that changed, use [`delta()`].
    ///
    /// The exchange information is the profile sample's: specification version `3.0`, an empty
    /// supplier, status `online`. No clock is read. The supplier is the one thing you may add.
    pub fn snapshot(publication: Publication, generated: DateTime) -> MessageContainer {
        container(ProtocolTypeEnum::SnapshotPush, publication, generated)
    }

    /// Wraps a status publication as a **delta** (`deltaPush`): only the points that changed since
    /// the last delivery. Otherwise as [`snapshot()`].
    pub fn delta(publication: Publication, generated: DateTime) -> MessageContainer {
        container(ProtocolTypeEnum::DeltaPush, publication, generated)
    }

    fn container(
        protocol: ProtocolTypeEnum,
        publication: Publication,
        generated: DateTime,
    ) -> MessageContainer {
        MessageContainer {
            payload: vec![publication],
            exchange_information: Some(Box::new(ExchangeInformation {
                exchange_context: Some(Box::new(ExchangeContext {
                    coded_exchange_protocol: Some(protocol),
                    exchange_specification_version: Some("3.0".to_owned()),
                    supplier_or_cis_requester: Some(Box::new(Agent::default())),
                    ..ExchangeContext::default()
                })),
                dynamic_information: Some(Box::new(DynamicInformation {
                    exchange_status: Some(ExchangeStatusEnum::Online),
                    message_generation_timestamp: Some(generated),
                    ..DynamicInformation::default()
                })),
                ..ExchangeInformation::default()
            })),
            ..MessageContainer::default()
        }
    }
}
