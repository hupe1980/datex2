//! Writing a message container in every encoding: XML, 3.7 JSON and the Mobilithek profile, to a
//! sink and to a string, with the same functions that write a publication.

#![allow(clippy::unwrap_used)]

mod common;

use common::assert_container_valid;
use datex2::v3::exchange_information as ex;
use datex2::v3::message_container::MessageContainer;
use datex2::v3::{self, Publication, json};
use datex2::{ErrorKind, Extension, MultilingualString};

fn situation() -> Publication {
    let xml = include_str!("fixtures/situation.xml");
    v3::read(xml.as_bytes()).unwrap().publications.remove(0)
}

fn exchange() -> ex::ExchangeInformation {
    ex::ExchangeInformation {
        exchange_context: Some(Box::new(ex::ExchangeContext {
            coded_exchange_protocol: Some(ex::ProtocolTypeEnum::SnapshotPush),
            exchange_specification_version: Some("3.0".into()),
            supplier_or_cis_requester: Some(Box::new(ex::Agent {
                name: Some("Verkehrszentrale Nord".into()),
                ..Default::default()
            })),
            ..Default::default()
        })),
        dynamic_information: Some(Box::new(ex::DynamicInformation {
            exchange_status: Some(ex::ExchangeStatusEnum::Online),
            message_generation_timestamp: "2026-11-20T08:00:00+01:00".parse().ok(),
            ..Default::default()
        })),
        ..Default::default()
    }
}

fn container(payload: Vec<Publication>) -> MessageContainer {
    MessageContainer {
        payload,
        exchange_information: Some(Box::new(exchange())),
        ..Default::default()
    }
}

/// write-roots/SR-002 (US1-1): an XML container is accepted by the official message container schema and
/// reads back to the same publications and exchange information.
#[test]
fn xml_container_is_valid_and_reads_back() {
    let written = container(vec![situation(), situation()]);
    let xml = v3::to_string(&written).unwrap();
    assert!(
        xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?><con:messageContainer "),
        "{}",
        &xml[..120]
    );
    assert_container_valid(&xml);
    let doc = v3::read(xml.as_bytes()).unwrap();
    assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);
    assert_eq!(doc.envelope, v3::Envelope::MessageContainer);
    assert_eq!(doc.publications, written.payload);
    assert_eq!(doc.exchange_information, written.exchange_information);
}

/// write-roots US1-1: the same container in JSON reads back as well.
#[test]
fn json_container_reads_back() {
    let written = container(vec![situation()]);
    let text = json::to_string(&written).unwrap();
    assert!(text.starts_with(r#"{"messageContainer":{"#), "{text}");
    let doc = json::read(text.as_bytes()).unwrap();
    assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);
    assert_eq!(doc.publications, written.payload);
    assert_eq!(doc.exchange_information, written.exchange_information);
}

/// write-roots FR-005: the string form writes what the sink form writes, and fails as it fails.
#[test]
fn string_and_sink_agree_for_every_encoding_and_root() {
    let publication = situation();
    let good = container(vec![situation()]);
    let mut bad = container(vec![situation()]);
    bad.exchange_information = None;
    let sink = |w: &dyn Fn(&mut Vec<u8>) -> Result<(), datex2::Error>| {
        let mut bytes = Vec::new();
        w(&mut bytes).map(|()| String::from_utf8(bytes).unwrap())
    };
    for (name, root) in [
        ("publication", v3::Root::from(&publication)),
        ("container", v3::Root::from(&good)),
        ("invalid container", v3::Root::from(&bad)),
    ] {
        assert_eq!(
            v3::to_string(root),
            sink(&|b| v3::write(b, root)),
            "XML, {name}"
        );
        assert_eq!(
            json::to_string(root),
            sink(&|b| json::write(b, root)),
            "JSON, {name}"
        );
        assert_eq!(
            json::mobilithek::to_string(root),
            sink(&|b| json::mobilithek::write(b, root)),
            "Mobilithek, {name}"
        );
    }
}

/// write-roots/SR-002: a container without the required generation time is refused in every encoding,
/// naming the member and the object that lacks it.
#[test]
fn sr002_missing_exchange_member_is_named_in_every_encoding() {
    let without_time = |payload| {
        let mut written = container(payload);
        let exchange = written.exchange_information.as_mut().unwrap();
        exchange
            .dynamic_information
            .as_mut()
            .unwrap()
            .message_generation_timestamp = None;
        written
    };
    let written = without_time(vec![situation()]);
    let errors = [
        v3::to_string(&written).unwrap_err(),
        json::to_string(&written).unwrap_err(),
        // The profile refuses a situation before it gets to the exchange information.
        json::mobilithek::to_string(&without_time(Vec::new())).unwrap_err(),
    ];
    for e in errors {
        assert_eq!(e.kind, ErrorKind::MissingRequired, "{e}");
        assert!(e.expected.contains("messageGenerationTimestamp"), "{e}");
        assert!(e.path.ends_with("dynamicInformation"), "{e}");
    }
}

/// write-roots/SR-002: the Mobilithek container holds only what the profile covers.
#[test]
fn sr002_mobilithek_refuses_a_publication_outside_the_profile() {
    let e = json::mobilithek::to_string(&container(vec![situation()])).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidValue, "{e}");
    assert!(e.to_string().contains("AFIR Energy Infrastructure"), "{e}");
}

/// write-roots edge case: a container may carry no publication.
#[test]
fn empty_container_is_written_and_valid() {
    let xml = v3::to_string(&container(Vec::new())).unwrap();
    assert_container_valid(&xml);
    let text = json::to_string(&container(Vec::new())).unwrap();
    assert!(json::read(text.as_bytes()).unwrap().publications.is_empty());
}

/// write-roots/SR-001: free text in the exchange information cannot add elements to the XML.
#[test]
fn sr001_markup_in_exchange_text_reads_back_unchanged() {
    let hostile = r#"<b>&amp; "quoted" 'single' </con:messageContainer><x/>"#;
    let mut written = container(vec![situation()]);
    let exchange = written.exchange_information.as_mut().unwrap();
    let supplier = exchange
        .exchange_context
        .as_mut()
        .unwrap()
        .supplier_or_cis_requester
        .as_mut()
        .unwrap();
    supplier.name = Some(hostile.into());
    supplier.address = Some(hostile.into());
    supplier.service_url = Some(hostile.into());
    exchange
        .dynamic_information
        .as_mut()
        .unwrap()
        .exchange_status_description = Some(MultilingualString::new("en", hostile));
    let xml = v3::to_string(&written).unwrap();
    assert!(!xml.contains("<x/>"), "{xml}");
    assert_container_valid(&xml);
    let doc = v3::read(xml.as_bytes()).unwrap();
    assert_eq!(doc.exchange_information, written.exchange_information);
    let again = json::read(json::to_string(&written).unwrap().as_bytes()).unwrap();
    assert_eq!(again.exchange_information, written.exchange_information);
}

/// write-roots edge case: extension content keeps its encoding in a container as in a publication.
#[test]
fn xml_extension_in_a_container_is_not_written_as_json() {
    let mut written = container(vec![situation()]);
    written.message_container_extension = Some(Box::new(Extension {
        xml: r#"<ext:note xmlns:ext="urn:example">x</ext:note>"#.into(),
        ..Default::default()
    }));
    let e = json::to_string(&written).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidExtension, "{e}");
}
