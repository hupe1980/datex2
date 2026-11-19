//! The trait that marks a type as streamable.

use crate::Error;
use crate::xml::Xml;

/// What the stream needs to know about a record type. Implemented by generated code only.
pub trait Sealed: Sized {
    /// The type's name, for messages.
    const NAME: &'static str;
    /// Publication type key and the chain of element keys from that publication to this type.
    const ROUTES: &'static [(u32, &'static [u32])];
    /// Reads the open element. `kind` is the publication type when the record is the payload itself.
    fn read_record(xml: &mut Xml<'_>, kind: Option<u32>) -> Result<Option<Self>, Error>;
}

/// A type that [`stream`](super::stream) can deliver one at a time.
///
/// These are the types the standard holds as repeated objects under a publication, directly or
/// inside up to two containers: a [`Situation`](super::situation::Situation) or a
/// [`SituationRecord`](super::situation::SituationRecord) of a situation publication, a
/// [`MeasurementSite`](super::road_traffic_data::MeasurementSite) of a site table, the
/// [`SiteMeasurements`](super::road_traffic_data::SiteMeasurements) of measured data, a
/// [`VmsController`](super::vms::VmsController), an
/// [`EnergyInfrastructureSite`](super::energy_infrastructure::EnergyInfrastructureSite), and so on.
/// [`Publication`](super::Publication) is a record too: streaming it delivers the payloads of a
/// message container one by one. The full list is under "Implementors" below.
///
/// A type that is not held as a repeated record cannot be streamed; the program does not compile:
///
/// ```compile_fail
/// let xml: &[u8] = b"";
/// // A validity is part of a record, never a record of its own.
/// let _ = datex2::v3::stream::<datex2::v3::common::Validity>(xml);
/// ```
///
/// The trait cannot be implemented outside this crate.
pub trait Record: Sealed {}
