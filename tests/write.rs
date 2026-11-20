//! Writing: valid when it succeeds, an exact error when the schema would reject the document.

mod common;

use common::*;
use datex2::v3::common::{
    ConfidentialityValueEnum, HeaderInformation, InformationStatusEnum, InternationalIdentifier,
    VersionedReference,
};
use datex2::v3::situation::{
    Accident, AccidentTypeEnum, Situation, SituationPublication, SituationRecord,
};
use datex2::v3::{
    self, Publication, common as com, energy_infrastructure as egi, location_referencing as loc,
    parking as prk, road_traffic_data as roa, vms,
};
use datex2::{DateTime, ErrorKind, Extension, LangString, MultilingualString};

fn time(text: &str) -> Option<DateTime> {
    Some(text.parse().unwrap())
}

fn creator() -> Option<Box<InternationalIdentifier>> {
    Some(Box::new(InternationalIdentifier {
        country: Some("NL".into()),
        national_identifier: Some("EXAMPLE".into()),
        ..Default::default()
    }))
}

fn header() -> Option<Box<HeaderInformation>> {
    Some(Box::new(HeaderInformation {
        confidentiality: Some(ConfidentialityValueEnum::NoRestriction),
        information_status: Some(InformationStatusEnum::Real),
        ..Default::default()
    }))
}

fn reference(id: &str) -> VersionedReference {
    VersionedReference {
        id: Some(id.into()),
        version: Some("1".into()),
    }
}

fn text(lang: &str, text: &str) -> MultilingualString {
    MultilingualString {
        values: vec![LangString {
            lang: lang.into(),
            text: text.into(),
        }],
    }
}

fn point() -> loc::LocationReference {
    loc::LocationReference::PointLocation(Box::new(loc::PointLocation {
        point_by_coordinates: Some(Box::new(loc::PointByCoordinates {
            point_coordinates: Some(Box::new(loc::PointCoordinates {
                latitude: Some(52.0907),
                longitude: Some(5.1214),
                ..Default::default()
            })),
            ..Default::default()
        })),
        ..Default::default()
    }))
}

/// An accident record with everything the schema requires.
fn accident_record() -> Accident {
    Accident {
        id: Some("R1".into()),
        version: Some("1".into()),
        situation_record_creation_time: time("2026-11-19T11:40:00Z"),
        situation_record_version_time: time("2026-11-19T11:58:00Z"),
        probability_of_occurrence: Some(
            datex2::v3::situation::ProbabilityOfOccurrenceEnum::Certain,
        ),
        validity: Some(Box::new(com::Validity {
            validity_status: Some(com::ValidityStatusEnum::Active),
            validity_time_specification: Some(Box::new(com::OverallPeriod {
                overall_start_time: time("2026-11-19T11:40:00Z"),
                ..Default::default()
            })),
            ..Default::default()
        })),
        location_reference: Some(point()),
        accident_type: vec![AccidentTypeEnum::Collision],
        ..Default::default()
    }
}

fn situation_publication(record: Accident) -> SituationPublication {
    SituationPublication {
        lang: Some("en".into()),
        publication_time: time("2026-11-19T12:00:00Z"),
        publication_creator: creator(),
        situation: vec![Situation {
            id: Some("S1".into()),
            situation_version_time: time("2026-11-19T11:58:00Z"),
            header_information: header(),
            situation_record: vec![SituationRecord::Accident(Box::new(record))],
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn written(record: Accident) -> String {
    v3::to_string(&Publication::Situation(Box::new(situation_publication(
        record,
    ))))
    .unwrap_or_else(|e| panic!("{e}"))
}

fn refused(record: Accident) -> datex2::Error {
    v3::to_string(&Publication::Situation(Box::new(situation_publication(
        record,
    ))))
    .expect_err("the write should fail")
}

// ---- valid documents ---------------------------------------------------------------

/// write-v3/SR-003: what the writer accepts, the official schema accepts.
#[test]
fn write_sr003_publications_built_in_code_are_accepted_by_the_official_schema() {
    let base = |lang: &str| {
        (
            Some(lang.to_owned()),
            time("2026-11-19T12:00:00Z"),
            creator(),
        )
    };
    let situation = Publication::Situation(Box::new(situation_publication(accident_record())));

    let (lang, publication_time, publication_creator) = base("en");
    let measured = Publication::MeasuredData(Box::new(roa::MeasuredDataPublication {
        lang,
        publication_time,
        publication_creator,
        measurement_site_table_reference: vec![reference("TABLE")],
        header_information: header(),
        site_measurements: vec![roa::SiteMeasurements {
            measurement_site_reference: Some(Box::new(reference("SITE-1"))),
            measurement_time_default: Some(Box::new(roa::MeasurementOrCalculationTime {
                time_value: time("2026-11-19T11:59:00Z"),
                ..Default::default()
            })),
            ..Default::default()
        }],
        ..Default::default()
    }));

    let (lang, publication_time, publication_creator) = base("en");
    let signs = Publication::Vms(Box::new(vms::VmsPublication {
        lang,
        publication_time,
        publication_creator,
        header_information: header(),
        vms_controller_status: vec![vms::VmsControllerStatus {
            vms_controller_table_reference: Some(Box::new(reference("TABLE"))),
            vms_controller_reference: Some(Box::new(reference("CONTROLLER-1"))),
            ..Default::default()
        }],
        ..Default::default()
    }));

    let (lang, publication_time, publication_creator) = base("en");
    let parking = Publication::ParkingTable(Box::new(prk::ParkingTablePublication {
        lang,
        publication_time,
        publication_creator,
        header_information: header(),
        parking_table: vec![prk::ParkingTable {
            id: Some("P".into()),
            version: Some("1".into()),
            version_time: time("2026-11-01T00:00:00Z"),
            hierarchy_element_general: vec![prk::HierarchyElementGeneral::Place(Box::new(
                prk::Place {
                    id: Some("PLACE-1".into()),
                    layer: Some(0),
                    r#type: Some(prk::HierarchyElementTypeEnum::Campus),
                    version: Some("1".into()),
                    ..Default::default()
                },
            ))],
            ..Default::default()
        }],
        ..Default::default()
    }));

    let (lang, publication_time, publication_creator) = base("de");
    let charging = Publication::EnergyInfrastructureTable(Box::new(
        egi::EnergyInfrastructureTablePublication {
            lang,
            publication_time,
            publication_creator,
            header_information: header(),
            energy_infrastructure_table: vec![egi::EnergyInfrastructureTable {
                id: Some("T".into()),
                version: Some("1".into()),
                energy_infrastructure_site: vec![egi::EnergyInfrastructureSite {
                    id: Some("SITE".into()),
                    version: Some("1".into()),
                    name: Some(text("de", "Ladepark Nord")),
                    location_reference: Some(point()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        },
    ));

    for (name, publication) in [
        ("situation", situation),
        ("measured data", measured),
        ("VMS", signs),
        ("parking", parking),
        ("energy infrastructure", charging),
    ] {
        let xml = v3::to_string(&publication).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_valid(&xml);
        // Deterministic, and the sink and the string agree.
        let mut sink = Vec::new();
        v3::write(&mut sink, &publication).unwrap();
        assert_eq!(sink, xml.as_bytes(), "{name}");
        assert_eq!(v3::to_string(&publication).unwrap(), xml, "{name}");
        // What was written reads back as what was built.
        let doc = read(&xml);
        assert_eq!(doc.warnings, [], "{name}");
        assert_eq!(doc.publications, [publication], "{name}");
    }
}

#[test]
fn form_of_the_document() {
    let xml = written(accident_record());
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?><d2:payload xsi:type=\"sit:SituationPublication\""), "{}", &xml[..120]);
    assert!(xml.contains(" modelBaseVersion=\"3\""));
    assert!(
        xml.contains("<sit:situationRecord xsi:type=\"sit:Accident\" id=\"R1\" version=\"1\">")
    );
    assert!(xml.contains("<sit:locationReference xsi:type=\"loc:PointLocation\">"));
    assert!(xml.ends_with("</sit:situation></d2:payload>"));
    assert!(!xml.contains('\n'), "compact");
}

#[test]
fn references_carry_their_fixed_target_class() {
    let publication = Publication::MeasuredData(Box::new(roa::MeasuredDataPublication {
        lang: Some("en".into()),
        publication_time: time("2026-11-19T12:00:00Z"),
        publication_creator: creator(),
        measurement_site_table_reference: vec![reference("TABLE")],
        header_information: header(),
        site_measurements: vec![roa::SiteMeasurements {
            measurement_site_reference: Some(Box::new(reference("SITE-1"))),
            measurement_time_default: Some(Box::new(roa::MeasurementOrCalculationTime {
                time_value: time("2026-11-19T11:59:00Z"),
                ..Default::default()
            })),
            ..Default::default()
        }],
        ..Default::default()
    }));
    let xml = v3::to_string(&publication).unwrap();
    assert!(xml.contains(r#"<roa:measurementSiteTableReference targetClass="roa:MeasurementSiteTable" id="TABLE" version="1"/>"#), "{xml}");
    assert_valid(&xml);
}

// ---- the rules -----------------------------------------------------------------------

/// write-v3/SR-003: each kind of rule, with the exact error.
#[test]
fn write_sr003_each_rule_is_named_in_the_error() {
    let message = |change: &dyn Fn(&mut Accident)| {
        let mut record = accident_record();
        change(&mut record);
        refused(record).to_string()
    };
    let at = "payload/situation/situationRecord";
    // Required element and required attribute.
    let e = message(&|r| r.probability_of_occurrence = None);
    assert!(e.starts_with(&format!("required content missing: expected required element \"sit:probabilityOfOccurrence\", found none at {at} (byte ")), "{e}");
    let e = message(&|r| r.id = None);
    assert!(e.starts_with(&format!("required content missing: expected required attribute \"id\", found none at {at} (byte ")), "{e}");
    // A text that is too long (com:String allows 1024 characters).
    let e = message(&|r| r.situation_record_creation_reference = Some("x".repeat(1025)));
    assert!(e.starts_with("value not allowed by the schema: expected at most 1024 characters in \"situationRecordCreationReference\", found \"xxxx"), "{e}");
    // A language code of the wrong form.
    let e = message(&|r| {
        r.general_public_comment = vec![datex2::v3::situation::Comment {
            comment: Some(text("english!", "x")),
            ..Default::default()
        }]
    });
    assert!(e.starts_with("value not allowed by the schema: expected a language code such as en or de-AT in \"lang\", found \"english!\" at payload/situation/situationRecord/generalPublicComment/comment/values/value"), "{e}");

    // Counts, ranges, digits and the remaining patterns sit on other types.
    let mut publication = situation_publication(accident_record());
    publication.situation[0].situation_record.clear();
    let e = v3::to_string(&Publication::Situation(Box::new(publication))).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidCount);
    assert!(e.to_string().starts_with("wrong number of elements: expected at least 1 of element \"sit:situationRecord\", found 0 at payload/situation (byte "), "{e}");

    let location = |change: &dyn Fn(&mut loc::PointLocation)| {
        let mut record = accident_record();
        let mut point = loc::PointLocation::default();
        change(&mut point);
        record.location_reference = Some(loc::LocationReference::PointLocation(Box::new(point)));
        refused(record)
    };
    // A bearing is an angle from 0 to 359.
    let e = location(&|p| {
        p.point_by_coordinates = Some(Box::new(loc::PointByCoordinates {
            bearing: Some(360),
            ..Default::default()
        }))
    });
    assert_eq!(e.kind, ErrorKind::InvalidValue);
    assert_eq!(
        (e.expected.as_str(), e.found.as_str()),
        ("0 to 359 in \"bearing\"", "360")
    );
}

#[test]
fn list_with_more_entries_than_allowed_is_refused() {
    // A recurring period names at most seven days of the week.
    let mut record = accident_record();
    let days = com::DayWeekMonth {
        applicable_day: vec![com::DayEnum::Monday; 8],
        ..Default::default()
    };
    let period = com::Period {
        recurring_day_week_month_period: vec![com::AnyDayWeekMonth::DayWeekMonth(Box::new(days))],
        ..Default::default()
    };
    record
        .validity
        .as_mut()
        .unwrap()
        .validity_time_specification
        .as_mut()
        .unwrap()
        .valid_period = vec![period];
    let e = refused(record);
    assert_eq!(e.kind, ErrorKind::InvalidCount);
    assert_eq!(
        (e.expected.as_str(), e.found.as_str()),
        ("0 to 7 of element \"com:applicableDay\"", "8")
    );
    assert!(
        e.path.ends_with(
            "validity/validityTimeSpecification/validPeriod/recurringDayWeekMonthPeriod"
        ),
        "{}",
        e.path
    );
}

#[test]
fn document_read_with_missing_required_content_is_not_written_back() {
    let xml = document(&accident("")).replace(
        "<sit:probabilityOfOccurrence>certain</sit:probabilityOfOccurrence>",
        "",
    );
    let doc = read(&xml);
    assert_eq!(kinds(&doc), [datex2::WarningKind::MissingElement]);
    let e = v3::to_string(&doc.publications[0]).unwrap_err();
    assert_eq!(e.kind, ErrorKind::MissingRequired);
    assert_eq!(
        e.expected,
        "required element \"sit:probabilityOfOccurrence\""
    );
}

// ---- read, change, write ---------------------------------------------------------------

#[test]
fn conforming_fixture_survives_read_write_read() {
    let first = read(FIXTURE);
    let xml = v3::to_string(&first.publications[0]).unwrap();
    assert_valid(&xml);
    let second = read(&xml);
    assert_eq!(second.publications, first.publications);
    assert_eq!(second.warnings, []);
}

#[test]
fn values_are_written_as_they_were_read() {
    let ext = r#"<nl:urgency xmlns:nl="urn:example:nl" level="2">high &amp; rising</nl:urgency><other xmlns="urn:example:other"/><sit:x/>"#;
    let xml = document(&accident(&format!(
        "<sit:_situationRecordExtension>{ext}</sit:_situationRecordExtension>\
         <sit:accidentType _extendedValue=\"droneCrash\">_extended</sit:accidentType><sit:accidentType>fromANewerRelease</sit:accidentType>"
    )))
    .replace("2026-11-19T12:00:00Z", "2026-11-19T13:00:00.729813995+01:00");
    let first = read(&xml);
    let out = v3::to_string(&first.publications[0]).unwrap();
    assert!(
        out.contains(
            "<com:publicationTime>2026-11-19T13:00:00.729813995+01:00</com:publicationTime>"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            r#"<sit:accidentType _extendedValue="droneCrash">_extended</sit:accidentType>"#
        )
    );
    // A value outside the standard list is written in the standard's form for such values.
    assert!(out.contains(
        r#"<sit:accidentType _extendedValue="fromANewerRelease">_extended</sit:accidentType>"#
    ));
    assert!(out.contains(&format!(r#"<sit:_situationRecordExtension xmlns:sit="http://datex2.eu/schema/3/situation">{}</sit:_situationRecordExtension>"#, ext.replace("<other xmlns=\"urn:example:other\"/>", "<other xmlns=\"urn:example:other\"></other>").replace("<sit:x/>", "<sit:x></sit:x>"))), "{out}");
    assert_valid(&out);
    let second = read(&out);
    assert_eq!(
        first_accident(&second).situation_record_extension,
        first_accident(&first).situation_record_extension
    );
    assert_eq!(
        first_accident(&second).accident_type[0],
        AccidentTypeEnum::Extended("droneCrash".into())
    );
}

// ---- untrusted values ------------------------------------------------------------------

/// write-v3/SR-001: values arrive as values, whatever they contain.
#[test]
fn write_sr001_markup_in_values_is_escaped_and_reads_back_identical() {
    let hostile = "</sit:situationRecord><sit:situationRecord xsi:type=\"sit:Accident\" id='x'> & ]]> <!-- \" ' \r\n\t end";
    let mut record = accident_record();
    record.id = Some(hostile.into());
    record.situation_record_creation_reference = Some(hostile.into());
    record.general_public_comment = vec![datex2::v3::situation::Comment {
        comment: Some(text("en", hostile)),
        ..Default::default()
    }];
    record.accident_type = vec![AccidentTypeEnum::Extended(hostile.into())];
    let xml = written(record.clone());
    assert_valid(&xml);
    let doc = read(&xml);
    assert_eq!(doc.warnings, []);
    assert_eq!(records(&doc).len(), 1, "no record was added");
    assert_eq!(*first_accident(&doc), record);
}

/// write-v3/SR-001
#[test]
fn write_sr001_characters_xml_cannot_carry_are_refused() {
    for bad in ["a\u{0}b", "a\u{1b}[31m", "a\u{8}", "a\u{ffff}"] {
        let mut record = accident_record();
        record.situation_record_creation_reference = Some(bad.into());
        let e = refused(record);
        assert_eq!(e.kind, ErrorKind::InvalidValue, "{bad:?}");
        assert_eq!(
            e.path,
            "payload/situation/situationRecord/situationRecordCreationReference"
        );
        assert!(!e.to_string().chars().any(char::is_control));
        let mut record = accident_record();
        record.id = Some(bad.into());
        assert_eq!(
            refused(record).kind,
            ErrorKind::InvalidValue,
            "{bad:?} in an attribute"
        );
    }
}

fn with_extension(xml: &str, namespaces: &[(&str, &str)]) -> Accident {
    let mut record = accident_record();
    record.situation_record_extension = Some(Box::new(Extension {
        xml: xml.into(),
        namespaces: namespaces
            .iter()
            .map(|(p, u)| ((*p).to_owned(), (*u).to_owned()))
            .collect(),
        ..Default::default()
    }));
    record
}

/// write-v3/SR-002: raw content cannot leave its slot or bring entities.
#[test]
fn write_sr002_extension_content_that_is_not_safe_is_refused() {
    let breakout = "</sit:_situationRecordExtension><sit:accidentType>collision</sit:accidentType><sit:_situationRecordExtension>";
    for (xml, why) in [
        (breakout, "closes its slot"),
        ("</x><evil/><x>", "closes the checking wrapper"),
        (
            "<!DOCTYPE a [<!ENTITY e \"x\">]><a/>",
            "document type declaration",
        ),
        ("<a>&e;</a>", "custom entity"),
        ("<a><b></a>", "unbalanced"),
        ("<a>", "unclosed"),
        ("<p:a/>", "undeclared prefix"),
        ("<?xml version=\"1.0\"?><a/>", "XML declaration"),
        (
            "<a/>text",
            "text beside the elements, which the schema does not allow",
        ),
        ("<a/>&amp;", "an entity beside the elements"),
    ] {
        let e = refused(with_extension(xml, &[]));
        assert_eq!(e.kind, ErrorKind::InvalidExtension, "{why}");
        assert_eq!(
            e.path, "payload/situation/situationRecord/_situationRecordExtension",
            "{why}"
        );
    }
    // Declarations are attribute text: a binding cannot smuggle markup either.
    let e = refused(with_extension("<p:a/>", &[("p", "urn:x\"><evil/><y a=\"")]));
    assert_eq!(e.kind, ErrorKind::InvalidExtension);
}

/// write-v3/SR-002
#[test]
fn write_sr002_well_formed_extension_content_survives() {
    let content = r#"<p:a b="1"><q xmlns="urn:q">t &amp; u</q><![CDATA[<raw>]]></p:a> <p:b></p:b>"#;
    let record = with_extension(content, &[("p", "urn:p")]);
    let xml = written(record.clone());
    assert_valid(&xml);
    assert!(xml.contains(&format!(r#"<sit:_situationRecordExtension xmlns:p="urn:p">{content}</sit:_situationRecordExtension>"#)), "{xml}");
    assert_eq!(
        first_accident(&read(&xml)).situation_record_extension,
        record.situation_record_extension
    );
}

#[test]
fn sink_failure_is_reported() {
    struct Full;
    impl std::io::Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let e = v3::write(
        Full,
        &Publication::Situation(Box::new(situation_publication(accident_record()))),
    )
    .unwrap_err();
    assert_eq!((e.kind, e.found.as_str()), (ErrorKind::Io, "disk full"));
}
