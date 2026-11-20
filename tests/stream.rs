//! Streaming: one record at a time, the same data and the same tolerance as a whole read.

mod common;

use common::*;
use datex2::v3::road_traffic_data::MeasurementSite;
use datex2::v3::situation::{Situation, SituationRecord};
use datex2::v3::{self, Publication};
use datex2::{ErrorKind, Limits, WarningKind as W};

/// Collects a stream, failing the test on an error.
fn all<T: v3::Record>(xml: &str) -> (Vec<T>, Vec<datex2::Warning>) {
    let mut stream = v3::stream::<T>(xml.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    let records = stream
        .by_ref()
        .map(|r| r.unwrap_or_else(|e| panic!("{e}")))
        .collect();
    (records, stream.take_warnings())
}

/// A situation with the given id around the given records.
fn situation(id: &str, records: &str) -> String {
    format!(
        "<sit:situation id=\"{id}\"><sit:situationVersionTime>2026-11-19T11:58:00Z</sit:situationVersionTime>\
         <sit:headerInformation><com:confidentiality>noRestriction</com:confidentiality><com:informationStatus>real</com:informationStatus></sit:headerInformation>\
         {records}</sit:situation>"
    )
}

const HEADER: &str = "<com:publicationTime>2026-11-19T12:00:00Z</com:publicationTime>\
    <com:publicationCreator><com:country>NL</com:country><com:nationalIdentifier>EXAMPLE</com:nationalIdentifier></com:publicationCreator>";

/// A situation publication as a `d2:payload` around the given situations.
fn situations_doc(situations: &str) -> String {
    format!(
        r#"<d2:payload {NAMESPACES} xsi:type="sit:SituationPublication" lang="en" modelBaseVersion="3">{HEADER}{situations}</d2:payload>"#
    )
}

fn three_situations() -> String {
    situations_doc(&format!(
        "{}{}{}",
        situation("A", &accident("")),
        situation("B", &format!("{}{}", accident(""), accident(""))),
        situation("C", "")
    ))
}

fn whole_situations(xml: &str) -> Vec<Situation> {
    match read(xml).publications.into_iter().next() {
        Some(Publication::Situation(p)) => p.situation,
        other => panic!("{other:?}"),
    }
}

// ---- records, one at a time -----------------------------------------------------------------------------

#[test]
fn situations_arrive_in_order_and_equal_the_whole_read() {
    let xml = three_situations();
    let (streamed, warnings) = all::<Situation>(&xml);
    assert_eq!(
        streamed
            .iter()
            .map(|s| s.id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["A", "B", "C"]
    );
    assert_eq!(streamed, whole_situations(&xml));
    assert_eq!(warnings, []);
}

#[test]
fn every_document_shape_streams() {
    let payload = three_situations();
    let in_container = payload
        .replace("<d2:payload", "<mc:payload")
        .replace("</d2:payload>", "</mc:payload>");
    let container = format!(
        r#"<mc:messageContainer xmlns:mc="http://datex2.eu/schema/3/messageContainer" xmlns:ex="http://datex2.eu/schema/3/exchangeInformation">{in_container}<mc:exchangeInformation><ex:exchangeContext><ex:codedExchangeProtocol>snapshotPull</ex:codedExchangeProtocol><ex:exchangeSpecificationVersion>3.0</ex:exchangeSpecificationVersion><ex:supplierOrCisRequester/></ex:exchangeContext><ex:dynamicInformation><ex:exchangeStatus>online</ex:exchangeStatus><ex:messageGenerationTimestamp>2026-11-19T12:00:00Z</ex:messageGenerationTimestamp></ex:dynamicInformation></mc:exchangeInformation>{in_container}</mc:messageContainer>"#
    );
    let bare = format!(
        r#"<sit:SituationPublication {NAMESPACES} lang="en">{HEADER}{}</sit:SituationPublication>"#,
        situation("A", "")
    );
    let soap = format!(
        r#"<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope"><s:Header><h xmlns="urn:x"/></s:Header><s:Body><op xmlns="urn:x">{payload}</op></s:Body></s:Envelope>"#
    );
    for (xml, count) in [
        (payload.as_str(), 3),
        (container.as_str(), 6),
        (bare.as_str(), 1),
        (soap.as_str(), 3),
    ] {
        let (records, warnings) = all::<Situation>(xml);
        assert_eq!(records.len(), count);
        assert_eq!(warnings, []);
    }
}

#[test]
fn publication_without_records_ends_without_an_item() {
    let (records, warnings) = all::<Situation>(&situations_doc(""));
    assert_eq!(records.len(), 0);
    assert_eq!(warnings, []);
}

/// A source that counts what was asked of it.
struct Counting<'a>(&'a [u8], usize);

impl std::io::Read for Counting<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = self.0.read(out)?;
        self.1 += n;
        Ok(n)
    }
}

#[test]
fn stopping_early_reads_no_further() {
    let one = situation("A", &accident(""));
    let xml = situations_doc(&one.repeat(5000));
    let mut source = Counting(xml.as_bytes(), 0);
    {
        let mut stream =
            v3::stream::<Situation>(std::io::BufReader::with_capacity(4096, &mut source)).unwrap();
        assert_eq!(stream.next().unwrap().unwrap().id.as_deref(), Some("A"));
    }
    assert!(xml.len() > 5_000_000);
    assert!(
        source.1 <= one.len() + 3 * 4096,
        "read {} bytes for one record",
        source.1
    );
}

// ---- choosing the record type -----------------------------------------------------------------------------

#[test]
fn situation_records_are_streamed_across_situations() {
    let xml = three_situations();
    let (streamed, _) = all::<SituationRecord>(&xml);
    let whole: Vec<SituationRecord> = whole_situations(&xml)
        .into_iter()
        .flat_map(|s| s.situation_record)
        .collect();
    assert_eq!(streamed.len(), 3);
    assert_eq!(streamed, whole);
}

fn site_table_doc() -> String {
    let site = |id: &str| {
        format!(
            r#"<roa:measurementSite id="{id}" version="1"><roa:measurementSiteNumberOfLanes>2</roa:measurementSiteNumberOfLanes></roa:measurementSite>"#
        )
    };
    let table = |id: &str, sites: &str| {
        format!(
            r#"<roa:measurementSiteTable id="{id}" version="1">{sites}</roa:measurementSiteTable>"#
        )
    };
    format!(
        r#"<d2:payload {NAMESPACES} xmlns:roa="http://datex2.eu/schema/3/roadTrafficData" xsi:type="roa:MeasurementSiteTablePublication" lang="en">{HEADER}<roa:headerInformation><com:confidentiality>noRestriction</com:confidentiality><com:informationStatus>real</com:informationStatus></roa:headerInformation>{}{}</d2:payload>"#,
        table("T1", &format!("{}{}", site("a"), site("b"))),
        table("T2", &site("c"))
    )
}

#[test]
fn measurement_sites_are_streamed_out_of_their_tables() {
    let xml = site_table_doc();
    let (sites, _) = all::<MeasurementSite>(&xml);
    assert_eq!(
        sites
            .iter()
            .map(|s| s.id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert_eq!(sites[0].measurement_site_number_of_lanes, Some(2));
    let Some(Publication::MeasurementSiteTable(whole)) = read(&xml).publications.into_iter().next()
    else {
        panic!()
    };
    let whole: Vec<MeasurementSite> = whole
        .measurement_site_table
        .into_iter()
        .flat_map(|t| t.measurement_site)
        .collect();
    assert_eq!(sites, whole);
}

#[test]
fn publications_are_records_too() {
    let (publications, _) = all::<Publication>(&three_situations());
    assert_eq!(publications, read(&three_situations()).publications);
}

#[test]
fn root_publication_that_cannot_hold_the_record_type_fails_at_once() {
    let e = v3::stream::<Situation>(site_table_doc().as_bytes()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnexpectedPublication);
    assert_eq!(
        e.to_string(),
        "publication without such records: expected a publication that holds Situation records, found MeasurementSiteTablePublication at payload (byte 375)"
    );
}

#[test]
fn payload_in_a_container_that_cannot_hold_the_record_type_is_skipped_with_a_warning() {
    let situations = three_situations()
        .replace("<d2:payload", "<mc:payload")
        .replace("</d2:payload>", "</mc:payload>");
    let sites = site_table_doc()
        .replace("<d2:payload", "<mc:payload")
        .replace("</d2:payload>", "</mc:payload>");
    let xml = format!(
        r#"<mc:messageContainer xmlns:mc="http://datex2.eu/schema/3/messageContainer">{sites}{situations}</mc:messageContainer>"#
    );
    let (records, warnings) = all::<Situation>(&xml);
    assert_eq!(records.len(), 3);
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        (warnings[0].kind, warnings[0].detail.as_str()),
        (
            W::UnexpectedPublication,
            "MeasurementSiteTablePublication holds no Situation records"
        )
    );
}

/// stream-records/SR-003: only elements on the route are records.
#[test]
fn stream_sr003_elements_named_like_records_elsewhere_are_not_records() {
    let decoy = situation("DECOY", "");
    let in_extension =
        format!("<com:_payloadPublicationExtension>{decoy}</com:_payloadPublicationExtension>");
    let in_record = situation(
        "REAL",
        &accident(&format!(
            "<sit:_situationRecordExtension>{decoy}</sit:_situationRecordExtension>"
        )),
    );
    let foreign = decoy.replace("<sit:situation ", "<sit:situation xmlns:sit=\"urn:evil\" ");
    let unknown_wrapper = format!("<sit:wrapper>{decoy}</sit:wrapper>");
    let xml = situations_doc(&format!(
        "{in_extension}{in_record}{foreign}{unknown_wrapper}"
    ));
    let (records, warnings) = all::<Situation>(&xml);
    assert_eq!(
        records
            .iter()
            .map(|s| s.id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["REAL"]
    );
    assert_eq!(warnings, [], "content outside records is passed over");
    // The streamed records are the objects at the route's place in the whole read.
    assert_eq!(records, whole_situations(&xml));
}

/// stream-records/SR-003: the reported namespace fallback of the whole read applies to routes.
#[test]
fn stream_sr003_record_in_another_datex_namespace_is_delivered_and_reported() {
    let xml = three_situations()
        .replace("<sit:situation ", "<d2:situation ")
        .replace("</sit:situation>", "</d2:situation>");
    let (records, warnings) = all::<Situation>(&xml);
    assert_eq!(records.len(), 3);
    assert_eq!(
        warnings.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [W::WrongNamespace; 3]
    );
    let (nested, _) = all::<SituationRecord>(&xml);
    assert_eq!(nested.len(), 3);
}

// ---- tolerance and warnings -----------------------------------------------------------------------------

#[test]
fn deviations_give_the_same_records_and_warnings_as_the_whole_read() {
    let messy = situation("A", &accident("<sit:accidentType/><sit:unknown/><sit:totalNumberOfVehiclesInvolved>x</sit:totalNumberOfVehiclesInvolved>").replace(" id=\"R1\"", ""));
    for xml in [
        situations_doc(&messy),
        situations_doc(&messy).replace("http://datex2.eu/schema/3/", "http://levelC/schema/3/"),
        situations_doc(&messy).replace(r#" xsi:type="sit:SituationPublication""#, ""),
        situations_doc(&messy)
            .replace("<d2:payload", "<payload")
            .replace("</d2:payload>", "</payload>"),
    ] {
        let whole = read(&xml);
        let (records, warnings) = all::<Situation>(&xml);
        assert_eq!(records, whole_situations(&xml));
        assert_eq!(
            warnings, whole.warnings,
            "the deviations are all in the envelope or inside records"
        );
        assert!(warnings.len() >= 4);
    }
}

#[test]
fn warnings_taken_along_the_way_are_never_suppressed() {
    let noisy = situation("A", "<sit:unknown/>");
    let xml = situations_doc(&noisy.repeat(3000));
    let mut stream = v3::stream::<Situation>(xml.as_bytes()).unwrap();
    let mut total = 0;
    while let Some(record) = stream.next() {
        record.unwrap();
        let warnings = stream.take_warnings();
        assert_eq!(warnings.len(), 1);
        total += warnings.len();
    }
    assert_eq!((total, stream.suppressed_warnings()), (3000, 0));
    // Without taking them, the cap of the whole read applies.
    let mut stream = v3::stream::<Situation>(xml.as_bytes()).unwrap();
    assert_eq!(stream.by_ref().count(), 3000);
    assert_eq!(
        (stream.take_warnings().len(), stream.suppressed_warnings()),
        (1000, 2000)
    );
}

// ---- failing in the middle -----------------------------------------------------------------------------

/// stream-records/SR-002: one error, then the end, for ever.
#[test]
fn stream_sr002_truncated_document_gives_the_records_before_the_cut_then_one_error_then_nothing() {
    let one = situation("A", &accident(""));
    let xml = situations_doc(&one.repeat(6));
    let fourth = xml.match_indices("<sit:situation ").nth(3).unwrap().0;
    let cut = &xml[..fourth + one.len() / 2];
    let mut stream = v3::stream::<Situation>(cut.as_bytes()).unwrap();
    for _ in 0..3 {
        assert!(stream.next().unwrap().is_ok());
    }
    let e = stream.next().unwrap().unwrap_err();
    assert_eq!(e.kind, ErrorKind::Syntax);
    assert!(e.path.starts_with("payload/situation/"), "{e}");
    assert!(e.offset as usize > fourth && e.offset as usize <= cut.len());
    for _ in 0..1000 {
        assert!(stream.next().is_none());
    }
}

/// stream-records/SR-002
#[test]
fn stream_sr002_finished_stream_stays_finished() {
    let xml = three_situations();
    let mut stream = v3::stream::<Situation>(xml.as_bytes()).unwrap();
    assert_eq!(stream.by_ref().count(), 3);
    for _ in 0..1000 {
        assert!(stream.next().is_none());
    }
}

/// stream-records/SR-001: the limits hold for content the stream passes over, not only in records.
#[test]
fn stream_sr001_limits_apply_between_and_inside_records() {
    let deep = format!("{}{}", "<sit:x>".repeat(200), "</sit:x>".repeat(200));
    for xml in [
        situations_doc(&format!("{deep}{}", situation("A", ""))),
        situations_doc(&situation("A", &accident(&deep))),
    ] {
        let mut stream = v3::stream::<Situation>(xml.as_bytes()).unwrap();
        let e = stream.next().unwrap().unwrap_err();
        assert_eq!(
            (e.kind, e.expected.as_str()),
            (
                ErrorKind::LimitExceeded,
                "nesting of at most 100 elements (max_depth)"
            )
        );
        assert!(stream.next().is_none());
    }
    let mut limits = Limits::default();
    limits.max_depth = 500;
    let xml = situations_doc(&format!("{deep}{}", situation("A", "")));
    assert_eq!(
        v3::stream_with::<Situation>(xml.as_bytes(), &limits)
            .unwrap()
            .count(),
        1
    );
    // A text that is passed over is never kept, but the parser must not buffer it without bound either.
    let long = situations_doc(&format!(
        "<sit:x>{}</sit:x>{}",
        "y".repeat(20 << 20),
        situation("A", "")
    ));
    let e = v3::stream::<Situation>(long.as_bytes())
        .unwrap()
        .next()
        .unwrap()
        .unwrap_err();
    assert_eq!(
        (e.kind, e.expected.as_str()),
        (
            ErrorKind::LimitExceeded,
            "a tag or text of at most 8388608 bytes (max_text_len)"
        )
    );
    limits.max_text_len = 32 << 20;
    assert_eq!(
        v3::stream_with::<Situation>(long.as_bytes(), &limits)
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn creation_fails_for_input_that_is_not_datex() {
    for (xml, kind) in [
        ("", ErrorKind::Syntax),
        ("<html/>", ErrorKind::NotDatex2),
        ("<!DOCTYPE x><d2:payload/>", ErrorKind::DoctypeNotAllowed),
        (
            r#"<d2LogicalModel xmlns="http://datex2.eu/schema/2/2_0"/>"#,
            ErrorKind::NotDatex2,
        ),
    ] {
        assert_eq!(
            v3::stream::<Situation>(xml.as_bytes()).unwrap_err().kind,
            kind,
            "{xml}"
        );
    }
}

#[test]
fn record_with_an_unknown_type_is_skipped_and_the_stream_goes_on() {
    let xml = situations_doc(&situation(
        "A",
        &format!(
            "{}{}",
            accident("").replace("sit:Accident", "sit:Nope"),
            accident("")
        ),
    ));
    let (records, warnings) = all::<SituationRecord>(&xml);
    assert_eq!(records.len(), 1);
    assert_eq!(
        warnings.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [W::UnknownType]
    );
}
