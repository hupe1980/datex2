//! One call reads every document shape, whatever the prefixes.

mod common;

use common::*;
use datex2::v3::{self, Envelope, Publication};

#[test]
fn payload_root() {
    let doc = read(FIXTURE);
    assert_eq!(doc.envelope, Envelope::Payload);
    assert!(!doc.soap);
    assert_eq!(doc.warnings, []);
    let accident = first_accident(&doc);
    assert_eq!(accident.id.as_deref(), Some("S1-R1"));
    assert_eq!(
        accident.accident_type,
        [v3::situation::AccidentTypeEnum::Collision]
    );
    assert_eq!(accident.total_number_of_vehicles_involved, Some(2));
    assert_eq!(
        accident.general_public_comment[0]
            .comment
            .as_ref()
            .unwrap()
            .get("nl"),
        Some("Twee auto's gebotst; de rechterrijstrook is versperd.")
    );
}

#[test]
fn shared_fields_are_reachable_without_matching() {
    let doc = read(FIXTURE);
    let publication = &doc.publications[0];
    assert_eq!(publication.lang(), Some("en"));
    assert_eq!(
        publication.publication_time().unwrap().to_string(),
        "2026-11-19T12:00:00Z"
    );
    let record = records(&doc)[0];
    assert_eq!(record.version(), Some("2"));
    assert_eq!(
        record.validity().unwrap().validity_status,
        Some(v3::common::ValidityStatusEnum::Active)
    );
}

fn container(payloads: &str) -> String {
    format!(
        r#"<mc:messageContainer xmlns:mc="http://datex2.eu/schema/3/messageContainer" xmlns:ex="http://datex2.eu/schema/3/exchangeInformation" {NAMESPACES} modelBaseVersion="3">{payloads}
           <mc:exchangeInformation><ex:exchangeContext><ex:codedExchangeProtocol>snapshotPull</ex:codedExchangeProtocol><ex:exchangeSpecificationVersion>3.0</ex:exchangeSpecificationVersion><ex:supplierOrCisRequester/></ex:exchangeContext>
           <ex:dynamicInformation><ex:exchangeStatus>online</ex:exchangeStatus><ex:messageGenerationTimestamp>2026-11-19T12:00:00Z</ex:messageGenerationTimestamp></ex:dynamicInformation></mc:exchangeInformation>
           </mc:messageContainer>"#
    )
}

fn container_payload() -> String {
    format!(
        r#"<mc:payload xsi:type="sit:SituationPublication" lang="en" modelBaseVersion="3">{}</mc:payload>"#,
        publication_body(&accident(""))
    )
}

#[test]
fn message_container_with_two_payloads() {
    let doc = read(&container(&format!(
        "{}{}",
        container_payload(),
        container_payload()
    )));
    assert_eq!(doc.envelope, Envelope::MessageContainer);
    assert_eq!(doc.publications.len(), 2);
    assert_eq!(doc.warnings, []);
    let exchange = doc
        .exchange_information
        .expect("exchange information is read");
    assert_eq!(
        exchange
            .exchange_context
            .and_then(|c| c.coded_exchange_protocol),
        Some(datex2::v3::exchange_information::ProtocolTypeEnum::SnapshotPull)
    );
}

#[test]
fn message_container_without_payload() {
    let doc = read(&container(""));
    assert_eq!(doc.publications.len(), 0);
    assert_eq!(doc.warnings, []);
}

#[test]
fn bare_publication_root() {
    let xml = format!(
        r#"<SituationPublication xmlns="http://datex2.eu/schema/3/situation" {NAMESPACES} lang="en" modelBaseVersion="3">{}</SituationPublication>"#,
        publication_body(&accident(""))
    );
    let doc = read(&xml);
    assert_eq!(doc.envelope, Envelope::Publication);
    assert!(matches!(doc.publications[0], Publication::Situation(_)));
    assert_eq!(doc.warnings, []);
}

#[test]
fn soap_envelopes() {
    for soap in [
        "http://schemas.xmlsoap.org/soap/envelope/",
        "http://www.w3.org/2003/05/soap-envelope",
    ] {
        let xml = format!(
            r#"<s:Envelope xmlns:s="{soap}"><s:Header><token xmlns="urn:example">x</token></s:Header><s:Body><pullSnapshotDataResponse xmlns="urn:example">{}</pullSnapshotDataResponse></s:Body></s:Envelope>"#,
            document(&accident(""))
        );
        let doc = read(&xml);
        assert!(doc.soap);
        assert_eq!(doc.envelope, Envelope::Payload);
        assert_eq!(
            doc.publications,
            read(&document(&accident(""))).publications,
            "the same data as without SOAP"
        );
        assert_eq!(doc.warnings, []);
    }
}

#[test]
fn reads_from_a_buffered_stream_in_small_pieces() {
    // A one-byte buffer forces every token across a buffer boundary.
    let stream = std::io::BufReader::with_capacity(1, FIXTURE.as_bytes());
    assert_eq!(v3::read(stream).unwrap(), read(FIXTURE));
}
