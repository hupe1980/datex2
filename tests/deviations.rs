//! The reader survives what national feeds send, and says what it tolerated.
//!
//! One test per deviation observed in a national feed, numbered in the order they were found.
//! The documentation lists them under "Tolerance and warnings".

mod common;

use common::*;
use datex2::WarningKind as W;
use datex2::v3::{self, Publication, situation::AccidentTypeEnum, situation::SituationRecord};

#[test]
fn q01_level_c_namespace_is_read_as_standard_and_reported_once_per_namespace() {
    let xml = document(&accident("<sit:accidentType>collision</sit:accidentType>"))
        .replace("http://datex2.eu/schema/3/", "http://levelC/schema/3/");
    let doc = read(&xml);
    assert_eq!(
        first_accident(&doc).accident_type,
        [AccidentTypeEnum::Collision]
    );
    // d2Payload, situation, common and locationReferencing each once, however often they occur.
    assert_eq!(kinds(&doc), [W::LevelCNamespace; 4]);
}

#[test]
fn q02_prefixes_and_default_namespaces_do_not_matter() {
    let xml = r#"<payload xmlns="http://datex2.eu/schema/3/d2Payload" xmlns:a="http://datex2.eu/schema/3/situation"
        xmlns:i="http://www.w3.org/2001/XMLSchema-instance" i:type="a:SituationPublication" lang="en" modelBaseVersion="3">
      <publicationTime xmlns="http://datex2.eu/schema/3/common">2026-11-19T12:00:00Z</publicationTime>
      <q1:publicationCreator xmlns:q1="http://datex2.eu/schema/3/common"><q1:country>NL</q1:country><q1:nationalIdentifier>X</q1:nationalIdentifier></q1:publicationCreator>
      <situation xmlns="http://datex2.eu/schema/3/situation" id="S1">
        <a:situationVersionTime>2026-11-19T11:58:00Z</a:situationVersionTime>
        <headerInformation><c:confidentiality xmlns:c="http://datex2.eu/schema/3/common">noRestriction</c:confidentiality>
          <informationStatus xmlns="http://datex2.eu/schema/3/common">real</informationStatus></headerInformation>
      </situation>
    </payload>"#;
    let doc = read(xml);
    assert_eq!(doc.warnings, []);
    let Publication::Situation(p) = &doc.publications[0] else {
        panic!()
    };
    assert_eq!(
        p.situation[0]
            .header_information
            .as_ref()
            .unwrap()
            .confidentiality,
        Some(v3::common::ConfidentialityValueEnum::NoRestriction)
    );
}

#[test]
fn q03_xml_schema_instance_namespace_under_another_prefix() {
    let xml = document(&accident(""))
        .replace("xmlns:xsi=", "xmlns:d1p1=")
        .replace("xsi:type", "d1p1:type");
    let doc = read(&xml);
    assert_eq!(doc.warnings, []);
    assert_eq!(records(&doc).len(), 1);
}

#[test]
fn q04_unprefixed_type_name_resolves_through_the_default_namespace() {
    let xml = format!(
        r#"<d2:payload {NAMESPACES} xmlns="http://datex2.eu/schema/3/situation" xsi:type="SituationPublication" lang="en" modelBaseVersion="3">{}</d2:payload>"#,
        publication_body(&accident("").replace("sit:Accident", "Accident"))
    );
    let doc = read(&xml);
    assert_eq!(doc.warnings, []);
    assert!(matches!(records(&doc)[0], SituationRecord::Accident(_)));
}

#[test]
fn q05_missing_type_on_the_payload_is_inferred_from_its_content() {
    let xml = document(&accident("")).replace(r#" xsi:type="sit:SituationPublication""#, "");
    let doc = read(&xml);
    assert_eq!(records(&doc).len(), 1);
    let w = only_warning(&doc);
    assert_eq!(w.kind, W::MissingXsiType);
    assert_eq!(
        w.to_string(),
        "publication type inferred: no xsi:type on the payload, read as SituationPublication at payload (byte 295)"
    );
}

#[test]
fn q05_missing_type_without_telling_content_is_an_error() {
    let xml = format!(
        r#"<d2:payload {NAMESPACES} lang="en" modelBaseVersion="3"><com:publicationTime>2026-11-19T12:00:00Z</com:publicationTime></d2:payload>"#
    );
    let e = v3::read(xml.as_bytes()).unwrap_err();
    assert_eq!(e.kind, datex2::ErrorKind::UnknownPublication);
}

#[test]
fn q06_target_class_on_references_is_accepted_silently() {
    let related = r#"<sit:relatedSituation><sit:objectReference targetClass="par:NotDeclared" id="S0"/></sit:relatedSituation>"#;
    let xml = document(&accident("")).replace(
        "<sit:situationRecord xsi",
        &format!("{related}<sit:situationRecord xsi"),
    );
    let doc = read(&xml);
    assert_eq!(
        doc.warnings,
        [],
        "a prefix inside the attribute text needs no declaration"
    );
    let Publication::Situation(p) = &doc.publications[0] else {
        panic!()
    };
    assert_eq!(
        p.situation[0].related_situation[0]
            .object_reference
            .as_ref()
            .unwrap()
            .id
            .as_deref(),
        Some("S0")
    );
}

#[test]
fn q07_byte_order_mark_line_ends_and_declarations() {
    let body = document(&accident(""));
    let plain = read(&body);
    for prefix in [
        "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n",
        "<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n",
        "",
    ] {
        let xml = format!("{prefix}{}", body.replace("><", ">\r\n<"));
        let doc = read(&xml);
        assert_eq!(doc.warnings, []);
        assert_eq!(doc.publications, plain.publications);
    }
}

#[test]
fn q08_message_container_with_payload_first_and_several_payloads() {
    let payload = format!(
        r#"<mc:payload xsi:type="sit:SituationPublication" lang="en" modelBaseVersion="3">{}</mc:payload>"#,
        publication_body(&accident(""))
    );
    let xml = format!(
        r#"<mc:messageContainer xmlns:mc="http://datex2.eu/schema/3/messageContainer" xmlns:ex="http://datex2.eu/schema/3/exchangeInformation" {NAMESPACES}>{payload}<mc:exchangeInformation><ex:dynamicInformation/></mc:exchangeInformation>{payload}</mc:messageContainer>"#
    );
    let doc = read(&xml);
    assert_eq!(doc.publications.len(), 2);
    assert_eq!(doc.warnings, []);
}

#[test]
fn q09_timestamps_keep_their_fraction_and_offset() {
    for (text, nanos, offset) in [
        ("2026-11-19T18:22:38Z", 0, 0),
        ("2026-11-19T18:22:38.7Z", 700_000_000, 0),
        ("2026-11-19T18:22:38.729813995Z", 729_813_995, 0),
        ("2026-11-19T19:27:02.410+01:00", 410_000_000, 60),
        ("2026-11-19T18:22:38.1234567+00:00", 123_456_700, 0),
    ] {
        let doc = read(&document(&accident("")).replace("2026-11-19T12:00:00Z", text));
        assert_eq!(doc.warnings, [], "{text}");
        let time = doc.publications[0].publication_time().unwrap();
        assert_eq!(
            (time.time.nanosecond, time.offset_minutes),
            (nanos, Some(offset)),
            "{text}"
        );
    }
}

#[test]
fn q10_timestamp_without_time_zone_is_kept_and_reported() {
    let doc = read(&document(&accident("")).replace("2026-11-19T12:00:00Z", "2026-10-19T00:00:00"));
    let time = doc.publications[0].publication_time().unwrap();
    assert_eq!(time.offset_minutes, None);
    assert_eq!(time.to_string(), "2026-10-19T00:00:00");
    assert_eq!(time.unix_timestamp(), None, "no instant is made up");
    assert_eq!(only_warning(&doc).kind, W::NoTimeZone);
}

#[test]
fn q11_empty_element_where_a_value_belongs_is_absent_and_reported() {
    let doc = read(&document(&accident(
        "<sit:accidentType/><sit:totalNumberOfVehiclesInvolved></sit:totalNumberOfVehiclesInvolved>",
    )));
    let accident = first_accident(&doc);
    assert_eq!(accident.accident_type, []);
    assert_eq!(accident.total_number_of_vehicles_involved, None);
    assert_eq!(kinds(&doc), [W::EmptyValue, W::EmptyValue]);
}

#[test]
fn q11_empty_text_is_a_value_not_a_deviation() {
    let comment = "<sit:generalPublicComment><sit:comment><com:values><com:value lang=\"nl\"/></com:values></sit:comment></sit:generalPublicComment>";
    let doc = read(&document(&accident("")).replace(
        "<sit:locationReference",
        &format!("{comment}<sit:locationReference"),
    ));
    assert_eq!(doc.warnings, []);
    assert_eq!(
        first_accident(&doc).general_public_comment[0]
            .comment
            .as_ref()
            .unwrap()
            .get("nl"),
        Some("")
    );
}

#[test]
fn q12_schema_location_is_never_used() {
    for value in [
        "https://datex2.eu/schema/3/situation",
        "/MDM-Profile.xsd",
        "a b c",
    ] {
        let xml = document(&accident("")).replace(
            "lang=\"en\"",
            &format!(
                "lang=\"en\" xsi:schemaLocation=\"{value}\" xsi:noNamespaceSchemaLocation=\"x.xsd\""
            ),
        );
        assert_eq!(read(&xml).warnings, []);
    }
}

#[test]
fn q13_language_codes_are_kept_as_written_and_a_missing_one_is_reported() {
    for lang in ["deu", "De", "FR", "en-us", "de-at"] {
        let doc =
            read(&document(&accident("")).replace("lang=\"en\"", &format!("lang=\"{lang}\"")));
        assert_eq!(doc.publications[0].lang(), Some(lang));
        assert_eq!(doc.warnings, []);
    }
    let doc = read(&document(&accident("")).replace(" lang=\"en\"", ""));
    assert_eq!(doc.publications[0].lang(), None);
    let w = only_warning(&doc);
    assert_eq!(
        (w.kind, w.detail.as_str()),
        (W::MissingAttribute, "required attribute \"lang\"")
    );
}

#[test]
fn q14_extension_content_is_kept_raw_without_a_warning() {
    let ext = r#"<sit:_situationRecordExtension><nl:urgency xmlns:nl="urn:example:nl" level="2">high &amp; rising</nl:urgency><other xmlns="urn:example:other"/><loc:x/></sit:_situationRecordExtension>"#;
    let doc = read(&document(&accident(ext)));
    assert_eq!(doc.warnings, []);
    let kept = first_accident(&doc)
        .situation_record_extension
        .as_ref()
        .unwrap();
    assert_eq!(
        kept.xml,
        r#"<nl:urgency xmlns:nl="urn:example:nl" level="2">high &amp; rising</nl:urgency><other xmlns="urn:example:other"></other><loc:x></loc:x>"#
    );
    // The bindings from the surrounding document that the content uses are kept, so that it can
    // be parsed on its own later; the others are not.
    assert_eq!(
        kept.namespaces,
        [(
            "loc".to_owned(),
            "http://datex2.eu/schema/3/locationReferencing".to_owned()
        )]
    );
}

#[test]
fn q15_unknown_element_is_skipped_and_reported_and_the_rest_is_read() {
    let doc = read(&document(&accident(
        "<sit:notInThisRelease><sit:deep>1</sit:deep></sit:notInThisRelease><sit:accidentType>collision</sit:accidentType>",
    )));
    assert_eq!(
        first_accident(&doc).accident_type,
        [AccidentTypeEnum::Collision]
    );
    let w = only_warning(&doc);
    assert_eq!(w.kind, W::UnknownElement);
    assert_eq!(w.detail, "\"notInThisRelease\" in namespace situation");
    assert_eq!(w.path, "payload/situation/situationRecord/notInThisRelease");
}

#[test]
fn q15_unknown_attribute_is_reported() {
    let doc = read(&document(&accident("")).replace(
        "version=\"1\"",
        "version=\"1\" colour=\"red\" x:y=\"z\" xmlns:x=\"urn:x\"",
    ));
    assert_eq!(kinds(&doc), [W::UnknownAttribute, W::UnknownAttribute]);
    assert_eq!(doc.warnings[0].detail, "\"colour\"");
}

#[test]
fn q16_identifiers_with_spaces_semicolons_and_non_ascii_text() {
    for id in ["48020_ZBE Bilbao", "DARS;602351;a-b", "Straßenverkehr – Ω"] {
        let doc = read(&document(&accident("")).replace("id=\"R1\"", &format!("id=\"{id}\"")));
        assert_eq!(records(&doc)[0].id(), Some(id));
        assert_eq!(doc.warnings, []);
    }
}

#[test]
fn q17_large_single_line_document_reads_and_offsets_stay_exact() {
    let good = accident("<sit:accidentType>collision</sit:accidentType>");
    let bad = accident("<sit:unknownHere/>");
    let xml = document(&format!("{}{bad}", good.repeat(20_000)));
    assert!(!xml.contains('\n') && xml.len() > 15_000_000);
    let doc = read(&xml);
    assert_eq!(records(&doc).len(), 20_001);
    let w = only_warning(&doc);
    let at = w.offset as usize;
    assert_eq!(
        &xml[at - "<sit:unknownHere/>".len()..at],
        "<sit:unknownHere/>",
        "the offset points just behind the element's start tag"
    );
}

#[test]
fn q18_known_element_in_another_datex_namespace_is_accepted_and_reported() {
    // Belgium writes `situation` in the payload namespace.
    let xml = document(&accident(""))
        .replace("<sit:situation ", "<d2:situation ")
        .replace("</sit:situation>", "</d2:situation>");
    let doc = read(&xml);
    assert_eq!(records(&doc).len(), 1);
    let w = only_warning(&doc);
    assert_eq!(
        (w.kind, w.detail.as_str()),
        (
            W::WrongNamespace,
            "\"situation\" in namespace d2Payload, expected in situation"
        )
    );
}

#[test]
fn q18_payload_root_in_no_namespace_with_a_datex_type() {
    let xml = document(&accident(""))
        .replace("<d2:payload", "<payload")
        .replace("</d2:payload>", "</payload>");
    let doc = read(&xml);
    assert_eq!(records(&doc).len(), 1);
    assert_eq!(kinds(&doc), [W::WrongNamespace]);
}

#[test]
fn q19_required_content_that_is_absent_is_reported_not_invented() {
    let xml = document(&accident(""))
        .replace(
            "<sit:probabilityOfOccurrence>certain</sit:probabilityOfOccurrence>",
            "",
        )
        .replace(" id=\"R1\"", "");
    let doc = read(&xml);
    let record = records(&doc)[0];
    assert_eq!(record.probability_of_occurrence(), None);
    assert_eq!(record.id(), None);
    assert_eq!(kinds(&doc), [W::MissingAttribute, W::MissingElement]);
    assert_eq!(
        doc.warnings[1].to_string(),
        "missing element: required element \"probabilityOfOccurrence\" at payload/situation/situationRecord (byte 1476)"
    );
}

#[test]
fn q20_value_of_the_wrong_form_is_dropped_and_reported() {
    let doc = read(
        &document(&accident(
            "<sit:totalNumberOfVehiclesInvolved>79,06</sit:totalNumberOfVehiclesInvolved>",
        ))
        .replace("52.0907", "north"),
    );
    assert_eq!(first_accident(&doc).total_number_of_vehicles_involved, None);
    // The latitude is required, so its loss is also reported as missing.
    assert_eq!(
        kinds(&doc),
        [W::InvalidValue, W::MissingElement, W::InvalidValue]
    );
    assert_eq!(
        doc.warnings[2].detail,
        "expected a non-negative integer, found \"79,06\""
    );
}

#[test]
fn q21_unknown_type_reads_as_the_declared_type_when_that_is_concrete() {
    // Spain extends a concrete type in a national namespace.
    let xml = document(&accident("")).replace(
        r#"<sit:locationReference xsi:type="loc:PointLocation">"#,
        r#"<sit:locationReference xsi:type="loc:PointLocation"><loc:supplementaryPositionalDescription><loc:roadInformation xsi:type="es:ExtendedRoadInformation" xmlns:es="urn:example:es"><loc:roadName>A-1</loc:roadName><es:extra>1</es:extra></loc:roadInformation></loc:supplementaryPositionalDescription>"#,
    );
    let doc = read(&xml);
    assert_eq!(kinds(&doc), [W::UnknownType, W::UnknownElement]);
    assert_eq!(
        doc.warnings[0].detail,
        "\"es:ExtendedRoadInformation\" is not a known kind of RoadInformation, reading as RoadInformation"
    );
}

#[test]
fn q21_unknown_type_is_skipped_when_the_declared_type_is_abstract() {
    let doc = read(&document(
        &accident("").replace("sit:Accident", "sit:NotARecordType"),
    ));
    assert_eq!(records(&doc).len(), 0);
    let w = only_warning(&doc);
    assert_eq!(
        (w.kind, w.detail.as_str()),
        (
            W::UnknownType,
            "\"sit:NotARecordType\" is not a known kind of SituationRecord"
        )
    );
}

#[test]
fn q22_enumeration_values_standard_extended_and_unknown() {
    let doc = read(&document(&accident(
        r#"<sit:accidentType>collision</sit:accidentType>
           <sit:accidentType _extendedValue="droneCrash">_extended</sit:accidentType>
           <sit:accidentType>fromANewerRelease</sit:accidentType>
           <sit:accidentType _extendedValue="rearEnd">other</sit:accidentType>
           <sit:accidentType>_extended</sit:accidentType>"#,
    )));
    assert_eq!(
        first_accident(&doc).accident_type,
        [
            AccidentTypeEnum::Collision,
            AccidentTypeEnum::Extended("droneCrash".into()),
            AccidentTypeEnum::Extended("fromANewerRelease".into()),
            AccidentTypeEnum::Other,
            AccidentTypeEnum::Extended(String::new()),
        ]
    );
    assert_eq!(
        kinds(&doc),
        [
            W::UnknownEnumValue,
            W::UnknownAttribute,
            W::MissingAttribute
        ]
    );
    assert_eq!(
        AccidentTypeEnum::Extended("droneCrash".into()).as_str(),
        "droneCrash"
    );
    assert_eq!(AccidentTypeEnum::Collision.as_str(), "collision");
}

// ---- the security requirements that are about names (read-v3/SR-005, read-v3/SR-006) ----------------------------

#[test]
fn sr005_prefix_rebound_to_a_foreign_namespace_is_not_standard_content() {
    let inner = r#"<x xmlns:sit="urn:evil"><sit:accidentType>collision</sit:accidentType></x>"#;
    let doc = read(&document(&accident(&format!(
        "<sit:_situationRecordExtension>{inner}</sit:_situationRecordExtension>"
    ))));
    assert_eq!(first_accident(&doc).accident_type, []);
    // The same trick outside an extension slot: the element is foreign, so it is skipped and reported.
    let xml = document(&accident(
        r#"<sit:accidentType xmlns:sit="urn:evil">collision</sit:accidentType>"#,
    ));
    let doc = read(&xml);
    assert_eq!(first_accident(&doc).accident_type, []);
    let w = only_warning(&doc);
    assert_eq!(
        (w.kind, w.detail.as_str()),
        (
            W::UnknownElement,
            "\"accidentType\" in namespace \"urn:evil\""
        )
    );
}

#[test]
fn sr005_type_attribute_outside_the_schema_instance_namespace_selects_nothing() {
    let xml = document(&accident("")).replace(
        r#"xsi:type="sit:Accident""#,
        r#"type="sit:Accident" e:type="sit:Accident" xmlns:e="urn:evil""#,
    );
    let doc = read(&xml);
    assert_eq!(
        records(&doc).len(),
        0,
        "without a real xsi:type the abstract record cannot be typed"
    );
    assert_eq!(kinds(&doc), [W::UnknownType]);
}

#[test]
fn sr005_type_that_is_not_derived_from_the_declared_type_is_not_used() {
    // A known type, but not a kind of situation record.
    let doc = read(&document(
        &accident("").replace("sit:Accident", "sit:SituationPublication"),
    ));
    assert_eq!(records(&doc).len(), 0);
    assert_eq!(kinds(&doc), [W::UnknownType]);
    // On an element whose type has no alternatives, a different type is reported and ignored.
    let doc = read(
        &document(&accident(""))
            .replace("<sit:validity>", "<sit:validity xsi:type=\"sit:Accident\">"),
    );
    assert_eq!(kinds(&doc), [W::UnexpectedType]);
    assert!(records(&doc)[0].validity().is_some());
}

#[test]
fn sr005_near_miss_level_c_and_foreign_namespaces_are_foreign() {
    for ns in [
        "http://levelc/schema/3/situation",
        "http://levelC/schema/3/situation/",
        "http://evil.example/levelC/schema/3/situation",
        "http://datex2.eu/schema/3/Situation",
    ] {
        let xml = document(&accident(&format!(
            r#"<f:accidentType xmlns:f="{ns}">collision</f:accidentType>"#
        )));
        let doc = read(&xml);
        assert_eq!(first_accident(&doc).accident_type, [], "{ns}");
        assert_eq!(kinds(&doc), [W::UnknownElement], "{ns}");
    }
}

#[test]
fn sr006_everything_skipped_is_counted() {
    let unknown = "<sit:a><sit:b/></sit:a><sit:c x=\"1\"/><u:d xmlns:u=\"urn:u\"/>";
    let doc = read(&document(&accident(unknown)).replace("id=\"S1\"", "id=\"S1\" extra=\"1\""));
    // Three skipped elements (inner content of a skipped element is part of it) and one attribute.
    assert_eq!(doc.warnings.len() + doc.suppressed_warnings, 4);
    assert!(
        doc.warnings
            .iter()
            .all(|w| !w.path.is_empty() && w.offset > 0)
    );
}

// ---- cases that are not deviations of a particular feed ---------------

#[test]
fn repeated_single_element_keeps_the_first_and_reports_the_rest() {
    let doc = read(&document(&accident(
        "<sit:totalNumberOfVehiclesInvolved>2</sit:totalNumberOfVehiclesInvolved><sit:totalNumberOfVehiclesInvolved>9</sit:totalNumberOfVehiclesInvolved>",
    )));
    assert_eq!(
        first_accident(&doc).total_number_of_vehicles_involved,
        Some(2)
    );
    let w = only_warning(&doc);
    assert_eq!(
        (w.kind, w.detail.as_str()),
        (W::Repeated, "\"totalNumberOfVehiclesInvolved\"")
    );
}

#[test]
fn comments_processing_instructions_and_stray_text_are_ignored() {
    let xml = document(&accident(
        "<!-- note --><?app do?>stray text<sit:accidentType>col<!-- inside -->lision</sit:accidentType><![CDATA[x]]>",
    ));
    let doc = read(&format!("<!-- before -->{xml}<!-- after -->"));
    assert_eq!(doc.warnings, []);
    assert_eq!(
        first_accident(&doc).accident_type,
        [AccidentTypeEnum::Collision]
    );
}

#[test]
fn text_in_cdata_is_text() {
    let doc = read(&document(&accident("")).replace("EXAMPLE", "<![CDATA[A <b> & C]]>"));
    let Publication::Situation(p) = &doc.publications[0] else {
        panic!()
    };
    assert_eq!(
        p.publication_creator
            .as_ref()
            .unwrap()
            .national_identifier
            .as_deref(),
        Some("A <b> & C")
    );
}
