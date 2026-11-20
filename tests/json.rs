//! DATEX II v3 in JSON: the official 3.7 mapping and the Mobilithek AFIR profile.
//!
//! Every document the writer produces here is checked with Python `jsonschema` against schemas
//! this project did not write: the official 3.7 JSON schemas or the profile's own schemas.

mod common;

use common::{JsonSchema, assert_json_valid};
use datex2::v3::afir_energy_infrastructure as aegi;
use datex2::v3::afir_facilities as afac;
use datex2::v3::common as com;
use datex2::v3::exchange_information as ex;
use datex2::v3::location_extension as locx;
use datex2::v3::location_referencing as loc;
use datex2::v3::message_container::MessageContainer;
use datex2::v3::{self, Publication, json};
use datex2::{DateTime, Decimal, ErrorKind, LangString, MultilingualString, WarningKind};

fn time(text: &str) -> Option<DateTime> {
    Some(text.parse().unwrap())
}

fn money(text: &str) -> Option<Decimal> {
    Some(text.parse().unwrap())
}

fn german(text: &str) -> MultilingualString {
    MultilingualString {
        values: vec![LangString {
            lang: "de".into(),
            text: text.into(),
        }],
    }
}

fn point(latitude: f32, longitude: f32) -> loc::LocationReference {
    let coordinates = loc::PointCoordinates {
        latitude: Some(latitude),
        longitude: Some(longitude),
        ..Default::default()
    };
    loc::LocationReference::PointLocation(Box::new(loc::PointLocation {
        coordinates_for_display: Some(Box::new(coordinates.clone())),
        location_extension: Some(Box::new(loc::LocationExtensionType {
            afir_facility_location: Some(Box::new(afac::AfirFacilityLocation {
                address: Some(Box::new(locx::Address {
                    postcode: Some("10117".into()),
                    city: Some(german("Berlin")),
                    country_code: Some("DE".into()),
                    address_line: vec![locx::AddressLine {
                        order: Some(0),
                        r#type: Some(locx::AddressLineTypeEnum::Street),
                        text: Some(german("Unter den Linden 1")),
                        ..Default::default()
                    }],
                    ..Default::default()
                })),
                ..Default::default()
            })),
            ..Default::default()
        })),
        point_by_coordinates: Some(Box::new(loc::PointByCoordinates {
            point_coordinates: Some(Box::new(coordinates)),
            ..Default::default()
        })),
        ..Default::default()
    }))
}

/// A price list of one energy rate, ad hoc, per kWh plus a session fee.
fn ad_hoc_rate(id: &str, per_kwh: &str) -> aegi::AnyEnergyProduct {
    aegi::AnyEnergyProduct::ElectricEnergy(Box::new(aegi::ElectricEnergy {
        energy_rate: vec![aegi::EnergyRate {
            id: Some(id.into()),
            rate_policy: Some(aegi::RatePolicyEnum::AdHoc),
            last_updated: time("2026-11-20T08:00:00+01:00"),
            applicable_currency: vec!["EUR".into()],
            energy_price: vec![
                aegi::EnergyPrice {
                    price_type: Some(aegi::PriceTypeEnum::PricePerKWh),
                    value: money(per_kwh),
                    tax_included: Some(true),
                    ..Default::default()
                },
                aegi::EnergyPrice {
                    price_type: Some(aegi::PriceTypeEnum::BasePrice),
                    value: money("0.00"),
                    tax_included: Some(true),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    }))
}

fn charging_point(
    id: &str,
    current: aegi::CurrentTypeEnum,
    watt: f32,
    connector: aegi::ConnectorTypeEnum,
    rate: &str,
    per_kwh: &str,
) -> aegi::RefillPoint {
    aegi::RefillPoint::ElectricChargingPoint(Box::new(aegi::ElectricChargingPoint {
        id: Some(id.into()),
        version: Some("1".into()),
        last_updated: time("2026-11-20T08:00:00+01:00"),
        delivery_unit: Some(aegi::DeliveryUnitEnum::KWh),
        energy_product: vec![ad_hoc_rate(rate, per_kwh)],
        current_type: Some(current),
        number_of_connectors: Some(1),
        available_charging_power: vec![watt],
        connector: vec![aegi::Connector {
            connector_type: Some(connector),
            connector_format: Some(aegi::ConnectorFormatTypeEnum::CableMode3),
            max_power_at_socket: Some(watt),
            ..Default::default()
        }],
        ..Default::default()
    }))
}

fn operator() -> afac::Organisation {
    afac::Organisation::AnOrganisation(Box::new(afac::AnOrganisation {
        name: Some(german("Beispiel Laden GmbH")),
        ..Default::default()
    }))
}

/// A site with one station: an AC point of 22 kW and a DC point of 150 kW, ad-hoc prices.
fn table() -> Publication {
    let station = aegi::EnergyInfrastructureStation {
        id: Some("STATION-1".into()),
        version: Some("1".into()),
        last_updated: time("2026-11-20T08:00:00+01:00"),
        operating_hours: Some(afac::OperatingHours::OpenAllHours(Box::default())),
        location_reference: Some(point(52.520008, 13.404954)),
        total_maximum_power: Some(172_000.0),
        authentication_and_identification_methods: vec![
            aegi::AuthenticationAndIdentificationEnum::CreditCard,
        ],
        number_of_refill_points: Some(2),
        service_type: vec![aegi::ServiceType {
            service_type: Some(aegi::ServiceTypeEnum::Unattended),
            ..Default::default()
        }],
        refill_point: vec![
            charging_point(
                "POINT-AC",
                aegi::CurrentTypeEnum::Ac,
                22_000.0,
                aegi::ConnectorTypeEnum::Iec62196T2,
                "RATE-AC",
                "0.59",
            ),
            charging_point(
                "POINT-DC",
                aegi::CurrentTypeEnum::Dc,
                150_000.0,
                aegi::ConnectorTypeEnum::Iec62196T2COMBO,
                "RATE-DC",
                "0.690",
            ),
        ],
        ..Default::default()
    };
    let site = aegi::EnergyInfrastructureSite {
        id: Some("SITE-1".into()),
        version: Some("1".into()),
        last_updated: time("2026-11-20T08:00:00+01:00"),
        operating_hours: Some(afac::OperatingHours::OpenAllHours(Box::default())),
        location_reference: Some(point(52.520008, 13.404954)),
        operator: Some(operator()),
        energy_infrastructure_station: vec![station],
        ..Default::default()
    };
    Publication::AfirEnergyInfrastructureTable(Box::new(
        aegi::EnergyInfrastructureTablePublication {
            lang: Some("de".into()),
            publication_time: time("2026-11-20T08:00:00+01:00"),
            publication_creator: Some(Box::new(com::InternationalIdentifier {
                country: Some("DE".into()),
                national_identifier: Some("DE-EXAMPLE".into()),
                ..Default::default()
            })),
            header_information: Some(Box::new(com::HeaderInformation {
                confidentiality: Some(com::ConfidentialityValueEnum::NoRestriction),
                information_status: Some(com::InformationStatusEnum::Real),
                ..Default::default()
            })),
            energy_infrastructure_table: vec![aegi::EnergyInfrastructureTable {
                id: Some("TABLE-1".into()),
                version: Some("1".into()),
                energy_infrastructure_site: vec![site],
                ..Default::default()
            }],
            ..Default::default()
        },
    ))
}

/// The table as 3.7 wants it: every energy price carries its price group index.
fn table_for_3_7() -> Publication {
    let mut publication = table();
    for_each_price(&mut publication, |price| price.price_group_index = Some(0));
    publication
}

fn for_each_price(publication: &mut Publication, mut f: impl FnMut(&mut aegi::EnergyPrice)) {
    let Publication::AfirEnergyInfrastructureTable(p) = publication else {
        unreachable!()
    };
    for table in &mut p.energy_infrastructure_table {
        for site in &mut table.energy_infrastructure_site {
            for station in &mut site.energy_infrastructure_station {
                for refill in &mut station.refill_point {
                    let aegi::RefillPoint::ElectricChargingPoint(point) = refill else {
                        continue;
                    };
                    for product in &mut point.energy_product {
                        let aegi::AnyEnergyProduct::ElectricEnergy(energy) = product else {
                            continue;
                        };
                        energy
                            .energy_rate
                            .iter_mut()
                            .flat_map(|r| &mut r.energy_price)
                            .for_each(&mut f);
                    }
                }
            }
        }
    }
}

fn reference(id: &str) -> Option<Box<com::VersionedReference>> {
    Some(Box::new(com::VersionedReference {
        id: Some(id.into()),
        version: Some("1".into()),
    }))
}

/// A status publication: the DC point is charging.
fn status() -> Publication {
    let point = aegi::ElectricChargingPointStatus {
        reference: reference("POINT-DC"),
        last_updated: time("2026-11-20T08:05:00+01:00"),
        status: Some(aegi::RefillPointStatusEnum::Charging),
        ..Default::default()
    };
    let station = aegi::EnergyInfrastructureStationStatus {
        reference: reference("STATION-1"),
        last_updated: time("2026-11-20T08:05:00+01:00"),
        refill_point_status: vec![aegi::AnyRefillPointStatus::ElectricChargingPointStatus(
            Box::new(point),
        )],
        ..Default::default()
    };
    let site = aegi::EnergyInfrastructureSiteStatus {
        reference: reference("SITE-1"),
        last_updated: time("2026-11-20T08:05:00+01:00"),
        energy_infrastructure_station_status: vec![station],
        ..Default::default()
    };
    Publication::AfirEnergyInfrastructureStatus(Box::new(
        aegi::EnergyInfrastructureStatusPublication {
            lang: Some("de".into()),
            publication_time: time("2026-11-20T08:05:00+01:00"),
            publication_creator: Some(Box::new(com::InternationalIdentifier {
                country: Some("DE".into()),
                national_identifier: Some("DE-EXAMPLE".into()),
                ..Default::default()
            })),
            table_reference: vec![*reference("TABLE-1").unwrap()],
            header_information: Some(Box::new(com::HeaderInformation {
                confidentiality: Some(com::ConfidentialityValueEnum::NoRestriction),
                information_status: Some(com::InformationStatusEnum::Real),
                ..Default::default()
            })),
            energy_infrastructure_site_status: vec![site],
            ..Default::default()
        },
    ))
}

fn container(protocol: ex::ProtocolTypeEnum) -> MessageContainer {
    MessageContainer {
        payload: vec![status()],
        exchange_information: Some(Box::new(ex::ExchangeInformation {
            exchange_context: Some(Box::new(ex::ExchangeContext {
                coded_exchange_protocol: Some(protocol),
                exchange_specification_version: Some("3.0".into()),
                supplier_or_cis_requester: Some(Box::default()),
                ..Default::default()
            })),
            dynamic_information: Some(Box::new(ex::DynamicInformation {
                exchange_status: Some(ex::ExchangeStatusEnum::Online),
                message_generation_timestamp: time("2026-11-20T08:05:00+01:00"),
                ..Default::default()
            })),
            ..Default::default()
        })),
        ..Default::default()
    }
}

fn mobilithek_container(protocol: ex::ProtocolTypeEnum) -> String {
    let mut bytes = Vec::new();
    json::mobilithek::write(&mut bytes, &container(protocol)).unwrap_or_else(|e| panic!("{e}"));
    String::from_utf8(bytes).unwrap()
}

/// What the profile writer adds to a publication that names no profile.
fn with_profile(mut publication: Publication) -> Publication {
    match &mut publication {
        Publication::AfirEnergyInfrastructureTable(p) => {
            p.profile_name = Some(json::mobilithek::PROFILE_NAME.into());
            p.profile_version = Some(json::mobilithek::PROFILE_VERSION.into());
        }
        Publication::AfirEnergyInfrastructureStatus(p) => {
            p.profile_name = Some(json::mobilithek::PROFILE_NAME.into());
            p.profile_version = Some(json::mobilithek::PROFILE_VERSION.into());
        }
        _ => {}
    }
    publication
}

fn read(text: &str) -> v3::Document {
    json::read(text.as_bytes()).unwrap_or_else(|e| panic!("{e}\n{text}"))
}

/// json-v3/SR-005: what the profile writer writes passes an independent validator.
#[test]
fn json_sr005_table_built_in_code_is_accepted_by_the_mobilithek_profile() {
    let text = json::mobilithek::to_string(&table()).unwrap_or_else(|e| panic!("{e}"));
    assert_json_valid(&text, JsonSchema::MobilithekTable);
    assert!(
        text.contains(
            r#""profileNameG":"AFIR Energy Infrastructure","profileVersionG":"01-00-00""#
        ),
        "{text}"
    );
    assert!(text.contains(r#""electricEnergy":[{"#), "{text}");
    assert!(text.contains(r#""FacilityLocation":{"#), "{text}");

    let doc = read(&text);
    assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);
    assert_eq!(doc.publications, vec![with_profile(table())]);
}

#[test]
fn table_built_in_code_is_accepted_by_the_official_schema() {
    let text = json::to_string(&table_for_3_7()).unwrap_or_else(|e| panic!("{e}"));
    assert_json_valid(&text, JsonSchema::Official);
    assert!(
        text.contains(r#""energyProduct":[{"aegiElectricEnergy":{"#),
        "{text}"
    );
    assert!(text.contains(r#""AfirFacilityLocation":{"#), "{text}");

    let doc = read(&text);
    assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);
    assert_eq!(doc.publications, vec![table_for_3_7()]);
}

#[test]
fn what_one_spelling_requires_and_the_other_forbids_is_named() {
    let error = json::to_string(&table()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::MissingRequired, "{error}");
    assert!(error.to_string().contains("priceGroupIndex"), "{error}");

    let error = json::mobilithek::to_string(&table_for_3_7()).unwrap_err();
    assert!(error.to_string().contains("priceGroupIndex"), "{error}");
    assert!(error.to_string().contains("profile"), "{error}");
}

#[test]
fn both_spellings_read_into_the_same_model() {
    let official = read(&json::to_string(&table_for_3_7()).unwrap());
    let profile = read(&json::mobilithek::to_string(&table()).unwrap());
    let mut expected = with_profile(table_for_3_7());
    for_each_price(&mut expected, |price| price.price_group_index = None);
    let mut official = official.publications.into_iter().next().unwrap();
    for_each_price(&mut official, |price| price.price_group_index = None);
    assert_eq!(with_profile(official), expected);
    assert_eq!(profile.publications, vec![with_profile(table())]);
}

#[test]
fn snapshot_and_delta_containers_are_accepted_by_the_mobilithek_profile() {
    for (protocol, literal) in [
        (ex::ProtocolTypeEnum::SnapshotPush, "snapshotPush"),
        (ex::ProtocolTypeEnum::DeltaPush, "deltaPush"),
    ] {
        let text = mobilithek_container(protocol.clone());
        assert_json_valid(&text, JsonSchema::MobilithekStatus);
        assert!(
            text.starts_with(r#"{"messageContainer":{"payload":[{"#),
            "{text}"
        );
        assert!(
            text.contains(&format!(
                r#""codedExchangeProtocol":{{"value":"{literal}"}}"#
            )),
            "{text}"
        );

        let doc = read(&text);
        assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);
        assert_eq!(doc.envelope, v3::Envelope::MessageContainer);
        assert_eq!(doc.publications, vec![with_profile(status())]);
        let exchange = doc.exchange_information.expect("exchange information");
        assert_eq!(
            exchange
                .exchange_context
                .and_then(|c| c.coded_exchange_protocol),
            Some(protocol)
        );
    }
}

#[test]
fn references_carry_their_target_class() {
    let text = mobilithek_container(ex::ProtocolTypeEnum::SnapshotPush);
    assert!(text.contains(r#""tableReference":[{"targetClass":"EnergyInfrastructureTable","idG":"TABLE-1","versionG":"1"}]"#), "{text}");
    assert!(
        text.contains(
            r#""reference":{"targetClass":"FacilityObject","idG":"POINT-DC","versionG":"1"}"#
        ),
        "{text}"
    );
}

#[test]
fn output_is_deterministic() {
    let first = json::mobilithek::to_string(&table()).unwrap();
    for _ in 0..3 {
        assert_eq!(json::mobilithek::to_string(&table()).unwrap(), first);
    }
    assert_eq!(
        mobilithek_container(ex::ProtocolTypeEnum::DeltaPush),
        mobilithek_container(ex::ProtocolTypeEnum::DeltaPush)
    );
}

/// json-v3/SR-004: decimals keep their digits, f32 is written shortest.
#[test]
fn json_sr004_money_and_measurements_keep_their_exact_value() {
    let mut publication = table();
    let prices = ["0.59", "0.590", "12.5", "999999.99"];
    let mut next = prices.iter();
    for_each_price(&mut publication, |price| {
        price.value = money(next.next().unwrap())
    });
    let text = json::mobilithek::to_string(&publication).unwrap();
    for price in prices {
        assert!(
            text.contains(&format!(r#""value":{price}}}"#))
                || text.contains(&format!(r#""value":{price},"#)),
            "{price} in {text}"
        );
    }
    // f32 values: the shortest text that reads back to the same f32, never 22000.0 or 52.520008087.
    assert!(
        text.contains(r#""availableChargingPower":[22000]"#),
        "{text}"
    );
    assert!(text.contains(r#""latitude":52.520008"#), "{text}");
    let doc = read(&text);
    assert_eq!(doc.publications, vec![with_profile(publication)]);
}

#[test]
fn price_of_0_1_is_not_0_1000000000000000055511151231257827() {
    let mut publication = table();
    for_each_price(&mut publication, |price| price.value = money("0.1"));
    let text = json::mobilithek::to_string(&publication).unwrap();
    assert!(text.contains(r#""value":0.1,"#), "{text}");
}

#[test]
fn profile_examples_read_without_warning_and_write_back_as_they_were() {
    let root = common::schema_dir()
        .parent()
        .unwrap()
        .join("afir-mobilithek-01-00-00");
    let cases = [
        (
            "EnergyInfrastructureTablePublication",
            JsonSchema::MobilithekTable,
        ),
        (
            "EnergyInfrastructureStatusPublication",
            JsonSchema::MobilithekStatus,
        ),
    ];
    for (name, schema) in cases {
        let path = root.join(name).join(format!("{name}_rev2.json"));
        let Ok(text) = std::fs::read_to_string(&path) else {
            common::skipped(
                "the Mobilithek profile samples are not fetched (run `cargo xtask fetch`)",
            );
            return;
        };
        let doc = read(&text);
        assert!(doc.warnings.is_empty(), "{name}: {:#?}", doc.warnings);
        let written = if let Some(exchange) = doc.exchange_information.clone() {
            let container = MessageContainer {
                payload: doc.publications.clone(),
                exchange_information: Some(exchange),
                ..Default::default()
            };
            let mut bytes = Vec::new();
            json::mobilithek::write(&mut bytes, &container).unwrap();
            String::from_utf8(bytes).unwrap()
        } else {
            json::mobilithek::to_string(&doc.publications[0]).unwrap()
        };
        assert_json_valid(&written, schema);
        // SC-003: what the profile publishes comes back as it was.
        common::assert_same_json(&path, &written);
        let again = read(&written);
        assert_eq!(again.publications, doc.publications, "{name}");
        assert_eq!(
            again.exchange_information, doc.exchange_information,
            "{name}"
        );
    }
}

#[test]
fn xml_fixture_converts_to_valid_3_7_json_and_back() {
    let doc = common::read(common::FIXTURE);
    let text = json::to_string(&doc.publications[0]).unwrap();
    assert_json_valid(&text, JsonSchema::Official);
    assert_eq!(read(&text).publications, doc.publications);
}

/// json-v3/SR-005: a value the profile does not list is refused with place and rule.
#[test]
fn json_sr005_profile_refusals_name_the_place_the_rule_and_the_choices() {
    // A service type 3.7 has and the profile does not list.
    let mut publication = table();
    if let Publication::AfirEnergyInfrastructureTable(p) = &mut publication {
        let station = &mut p.energy_infrastructure_table[0].energy_infrastructure_site[0]
            .energy_infrastructure_station[0];
        station.service_type[0].service_type = Some(aegi::ServiceTypeEnum::SelfService);
    }
    let error = json::mobilithek::to_string(&publication).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidValue);
    assert_eq!(
        error.path,
        "payload/aegiEnergyInfrastructureTablePublication/energyInfrastructureTable/energyInfrastructureSite/energyInfrastructureStation/serviceType/serviceType"
    );
    assert!(
        error.found.contains("\"selfService\"")
            && error
                .found
                .contains("it lists physicalAttendance, unattended"),
        "{error}"
    );
    // The official mapping has it.
    for_each_price(&mut publication, |price| price.price_group_index = Some(0));
    let text = json::to_string(&publication).unwrap();
    assert_json_valid(&text, JsonSchema::Official);
}

#[test]
fn language_code_must_be_two_lowercase_letters() {
    let mut publication = table();
    if let Publication::AfirEnergyInfrastructureTable(p) = &mut publication {
        p.energy_infrastructure_table[0].energy_infrastructure_site[0].name =
            Some(MultilingualString {
                values: vec![LangString {
                    lang: "DE".into(),
                    text: "Ladepark".into(),
                }],
            });
    }
    let error = json::mobilithek::to_string(&publication)
        .unwrap_err()
        .to_string();
    assert!(error.contains("lang") && error.contains("DE"), "{error}");
}

#[test]
fn timestamp_without_time_zone_is_refused() {
    let mut publication = table();
    if let Publication::AfirEnergyInfrastructureTable(p) = &mut publication {
        p.publication_time = time("2026-11-20T08:00:00");
    }
    let error = json::mobilithek::to_string(&publication)
        .unwrap_err()
        .to_string();
    assert!(error.contains("publicationTime"), "{error}");
}

#[test]
fn time_of_day_is_written_with_z_for_the_profile_and_read_back() {
    let table_text = std::fs::read_to_string(
        common::schema_dir()
            .parent()
            .unwrap()
            .join("afir-mobilithek-01-00-00/EnergyInfrastructureTablePublication/EnergyInfrastructureTablePublication_rev2.json"),
    );
    let Ok(text) = table_text else {
        common::skipped("the Mobilithek profile samples are not fetched (run `cargo xtask fetch`)");
        return;
    };
    let doc = read(&text);
    let profile = json::mobilithek::to_string(&doc.publications[0]).unwrap();
    assert!(
        profile.contains(r#""startTimeOfPeriod":"09:00:00Z""#),
        "{profile}"
    );
}

// --- Reading what senders produce -------------------------------------------------------------

fn payload(publication: &str) -> String {
    format!(r#"{{"payload":{publication}}}"#)
}

fn minimal_status(extra: &str) -> String {
    format!(
        r#"{{"modelBaseVersionG":"3","aegiEnergyInfrastructureStatusPublication":{{"lang":"de","publicationTime":"2026-11-20T08:05:00+01:00","publicationCreator":{{"country":"DE","nationalIdentifier":"X"}}{extra}}}}}"#
    )
}

#[test]
fn payload_may_be_one_publication_or_an_array() {
    let one = read(&payload(&minimal_status("")));
    let two = read(&payload(&format!("[{0},{0}]", minimal_status(""))));
    assert_eq!(one.publications.len(), 1);
    assert_eq!(two.publications.len(), 2);
    assert_eq!(two.publications[0], one.publications[0]);
}

#[test]
fn container_without_payload_is_read() {
    let doc = read(
        r#"{"messageContainer":{"exchangeInformation":{"dynamicInformation":{"exchangeStatus":{"value":"online"}}}}}"#,
    );
    assert!(doc.publications.is_empty());
    assert!(doc.exchange_information.is_some());
}

#[test]
fn integer_written_as_22000_0_is_accepted_with_a_warning() {
    let doc = read(&payload(&minimal_status(
        r#","energyInfrastructureSiteStatus":[{"reference":{"targetClass":"FacilityObject","idG":"S","versionG":"1"},"lastUpdated":"2026-11-20T08:05:00+01:00","availableCarParkingPlaces":22000.0}]"#,
    )));
    assert_eq!(
        doc.warnings.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [WarningKind::TypeMismatch],
        "{:#?}",
        doc.warnings
    );
    let Publication::AfirEnergyInfrastructureStatus(p) = &doc.publications[0] else {
        unreachable!()
    };
    assert_eq!(
        p.energy_infrastructure_site_status[0].available_car_parking_places,
        Some(22000)
    );
}

#[test]
fn integer_with_a_fraction_is_an_error_or_a_warning_never_a_silent_cut() {
    let result = json::read(payload(&minimal_status(r#","energyInfrastructureSiteStatus":[{"reference":{"targetClass":"FacilityObject","idG":"S","versionG":"1"},"lastUpdated":"2026-11-20T08:05:00+01:00","availableCarParkingPlaces":2.5}]"#)).as_bytes());
    match result {
        Err(_) => {}
        Ok(doc) => {
            assert!(!doc.warnings.is_empty());
            let Publication::AfirEnergyInfrastructureStatus(p) = &doc.publications[0] else {
                unreachable!()
            };
            assert_eq!(
                p.energy_infrastructure_site_status[0].available_car_parking_places,
                None
            );
        }
    }
}

/// json-v3/SR-002: null and a repeated member are reported, not merged silently.
#[test]
fn json_sr002_null_and_duplicate_members_are_reported() {
    let feed_type = |doc: &v3::Document| {
        let Publication::AfirEnergyInfrastructureStatus(p) = &doc.publications[0] else {
            unreachable!()
        };
        p.feed_type.clone()
    };
    let doc = read(&payload(&minimal_status(r#","feedType":null"#)));
    assert_eq!(feed_type(&doc), None);
    assert_eq!(
        doc.warnings.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [WarningKind::EmptyValue],
        "{:#?}",
        doc.warnings
    );

    let doc = read(&payload(&minimal_status(
        r#","feedType":"a","feedType":"b""#,
    )));
    assert_eq!(feed_type(&doc).as_deref(), Some("a"), "the first is kept");
    assert_eq!(doc.warnings.len(), 1, "{:#?}", doc.warnings);
    assert_eq!(doc.warnings[0].kind, WarningKind::Repeated);
    assert!(
        doc.warnings[0].to_string().contains("feedType"),
        "{}",
        doc.warnings[0]
    );
}

#[test]
fn unknown_members_are_skipped_with_a_warning() {
    let doc = read(&payload(&minimal_status(r#","roamingPlatfom":[{"x":1}]"#)));
    assert_eq!(doc.publications.len(), 1);
    assert!(
        doc.warnings
            .iter()
            .any(|w| w.kind == WarningKind::UnknownElement),
        "{:#?}",
        doc.warnings
    );
}

#[test]
fn malformed_json_is_an_error_with_a_place() {
    for text in [
        r#"{"payload":"#,
        r#"{"payload":{"modelBaseVersionG":"3","#,
        "[1,2]",
        "",
        r#"{"other":1}"#,
        "{\"payload\":\"\u{1}\"}",
    ] {
        let error = json::read(text.as_bytes()).unwrap_err().to_string();
        assert!(
            !error.contains('\n') && !error.chars().any(char::is_control),
            "{error:?}"
        );
    }
}

/// json-v3/SR-003: hostile text is escaped and reads back identical.
#[test]
fn json_sr003_strings_are_escaped_and_read_back_identical() {
    let mut publication = table();
    let nasty = "\"quoted\" \\ back\nslash \u{1} \u{2028} </script> é 😀";
    if let Publication::AfirEnergyInfrastructureTable(p) = &mut publication {
        let table = &mut p.energy_infrastructure_table[0];
        table.table_name = Some(nasty.into());
        table.id = Some(format!(r#"T","injected":"{nasty}"#));
        table.energy_infrastructure_site[0].name = Some(german(nasty));
    }
    // An extended enumeration value, which the profile allows as `extendedG`.
    first_point(&mut publication).connector[0].connector_type =
        Some(aegi::ConnectorTypeEnum::Extended(nasty.into()));
    let text = json::mobilithek::to_string(&publication).unwrap();
    assert!(
        !text.contains('\n') && !text.contains('\u{1}') && !text.contains(r#","injected":"#),
        "{text}"
    );
    assert_json_valid(&text, JsonSchema::MobilithekTable);
    let doc = read(&text);
    assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);
    assert_eq!(doc.publications, vec![with_profile(publication)]);
}

/// json-v3/SR-001: depth and text length are bounded and the error names the limit.
#[test]
fn json_sr001_limits_hold_while_reading() {
    let deep = format!(
        "{}{}",
        r#"{"payload":"#.to_owned() + &"[".repeat(10_000),
        "]".repeat(10_000) + "}"
    );
    let error = json::read(deep.as_bytes()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::LimitExceeded, "{error}");

    let long = payload(&minimal_status(&format!(
        r#","feedType":"{}""#,
        "a".repeat(2_000_000)
    )));
    let mut limits = datex2::Limits::default();
    limits.max_text_len = 1_000_000;
    let error = json::read_with(long.as_bytes(), &limits).unwrap_err();
    assert_eq!(error.kind, ErrorKind::LimitExceeded, "{error}");
}

/// json-v3/SR-003: extension content read from XML is not written as JSON.
#[test]
fn json_sr003_extension_content_does_not_cross_encodings() {
    // Extension content read from XML is XML text; JSON output refuses it rather than inventing JSON.
    let xml = common::document(&common::accident(
        r#"<sit:_situationRecordExtension><x:any xmlns:x="urn:example">1</x:any></sit:_situationRecordExtension><sit:accidentType>collision</sit:accidentType>"#,
    ));
    let doc = common::read(&xml);
    assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);
    let error = json::to_string(&doc.publications[0])
        .unwrap_err()
        .to_string();
    assert!(error.contains("extension"), "{error}");
}

/// json-v3/SR-004: no JSON library, so no process-wide number setting.
#[test]
fn json_sr004_json_support_adds_no_dependency() {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .split("\n[")
        .next()
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| l.split('=').next().unwrap().trim().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(dependencies, ["quick-xml"]);
}

// --- Where the profile and 3.7 differ in shape ------------------------------------------------

use datex2::v3::common_extension as comx;

/// The table with vehicle categories on the station: m1, and `l`, which only the profile lists.
fn table_with_categories(regulated: Vec<comx::RegulatedCharacteristics>) -> Publication {
    let mut publication = table();
    if let Publication::AfirEnergyInfrastructureTable(p) = &mut publication {
        let station = &mut p.energy_infrastructure_table[0].energy_infrastructure_site[0]
            .energy_infrastructure_station[0];
        station.applicable_for_vehicles = vec![com::VehicleCharacteristics {
            vehicle_characteristics_extension: Some(Box::new(
                com::VehicleCharacteristicsExtensionType {
                    vehicle_characteristics_extended: Some(Box::new(
                        comx::VehicleCharacteristicsExtended {
                            regulated_characteristics: regulated,
                            ..Default::default()
                        },
                    )),
                    ..Default::default()
                },
            )),
            ..Default::default()
        }];
    }
    publication
}

fn categories(list: &[comx::EuVehicleCategoryEnum]) -> Vec<comx::RegulatedCharacteristics> {
    vec![comx::RegulatedCharacteristics {
        eu_vehicle_category: list.to_vec(),
        ..Default::default()
    }]
}

#[test]
fn vehicle_categories_sit_where_each_spelling_puts_them() {
    let list = [
        comx::EuVehicleCategoryEnum::M1,
        comx::EuVehicleCategoryEnum::Extended("l".into()),
    ];
    let publication = table_with_categories(categories(&list));

    let profile = json::mobilithek::to_string(&publication).unwrap();
    assert!(profile.contains(r#""VehicleCharacteristicsExtended":{"euVehicleCategory":[{"value":"m1"},{"value":"l"}]}"#), "{profile}");
    assert_json_valid(&profile, JsonSchema::MobilithekTable);
    let doc = read(&profile);
    assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);
    assert_eq!(doc.publications, vec![with_profile(publication.clone())]);

    let mut official = publication;
    for_each_price(&mut official, |price| price.price_group_index = Some(0));
    let text = json::to_string(&official).unwrap();
    assert!(text.contains(r#""regulatedCharacteristics":[{"euVehicleCategory":[{"value":"m1"},{"value":"extendedG","extendedValueG":"l"}]}]"#), "{text}");
    assert_json_valid(&text, JsonSchema::Official);
    assert_eq!(read(&text).publications, vec![official]);
}

/// json-v3/SR-005: content the profile cannot express is refused, not dropped.
#[test]
fn json_sr005_regulated_characteristics_the_profile_cannot_express_are_refused() {
    let mut two = categories(&[comx::EuVehicleCategoryEnum::M1]);
    two.extend(categories(&[comx::EuVehicleCategoryEnum::N1]));
    let error = json::mobilithek::to_string(&table_with_categories(two))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("regulatedCharacteristics") && error.contains("euVehicleCategory"),
        "{error}"
    );

    let mut more = categories(&[comx::EuVehicleCategoryEnum::M1]);
    more[0].offroad_vehicle = Some(true);
    assert!(json::mobilithek::to_string(&table_with_categories(more)).is_err());
}

#[test]
fn roaming_platform_is_spelled_as_the_profile_spells_it() {
    let mut publication = table();
    if let Publication::AfirEnergyInfrastructureTable(p) = &mut publication {
        p.energy_infrastructure_table[0].energy_infrastructure_site[0].roaming_platform =
            vec![operator()];
    }
    let profile = json::mobilithek::to_string(&publication).unwrap();
    assert!(profile.contains(r#""roamingPlatfom":[{"#), "{profile}");
    assert_json_valid(&profile, JsonSchema::MobilithekTable);
    assert_eq!(
        read(&profile).publications,
        vec![with_profile(publication.clone())]
    );

    for_each_price(&mut publication, |price| price.price_group_index = Some(0));
    let official = json::to_string(&publication).unwrap();
    assert!(official.contains(r#""roamingPlatform":[{"#), "{official}");
    assert_eq!(read(&official).publications, vec![publication]);
}

#[test]
fn numbers_with_an_exponent_are_read_exactly() {
    let text = json::mobilithek::to_string(&table())
        .unwrap()
        .replace(r#""value":0.59"#, r#""value":5.9E-1"#)
        .replace("\"numberOfConnectors\":1", "\"numberOfConnectors\":1E0");
    let doc = read(&text);
    assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);
    let mut prices = Vec::new();
    let mut publication = doc.publications[0].clone();
    for_each_price(&mut publication, |price| prices.push(price.value.clone()));
    assert_eq!(prices[0], money("0.59"));
}

#[test]
fn exchange_information_copied_beside_the_container_is_not_a_warning() {
    let text = mobilithek_container(ex::ProtocolTypeEnum::DeltaPush);
    let inner = text.split(r#""exchangeInformation":"#).nth(1).unwrap();
    let inner = &inner[..inner.len() - 2];
    let doubled = format!(
        r#"{},"exchangeInformation":{inner}}}"#,
        &text[..text.len() - 1]
    );
    let doc = read(&doubled);
    assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);

    let other =
        doubled
            .replacen("deltaPush", "snapshotPush", 2)
            .replacen("snapshotPush", "deltaPush", 1);
    let doc = read(&other);
    assert_eq!(
        doc.warnings.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [WarningKind::Repeated]
    );
}

/// json-v3/SR-001: nesting inside a skipped member counts; warnings are capped and the rest counted.
#[test]
fn json_sr001_limits_apply_to_skipped_content_and_warnings() {
    // Nesting inside a member the reader skips counts as well.
    let deep = format!("{}{}", "[".repeat(5_000), "]".repeat(5_000));
    let text = payload(&minimal_status(&format!(r#","unknownMember":{deep}"#)));
    let error = json::read(text.as_bytes()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::LimitExceeded, "{error}");

    let unknown: String = (0..50).map(|i| format!(r#","unknown{i}":1"#)).collect();
    let mut limits = datex2::Limits::default();
    limits.max_warnings = 10;
    let doc = json::read_with(payload(&minimal_status(&unknown)).as_bytes(), &limits).unwrap();
    assert_eq!(doc.warnings.len(), 10);
    assert_eq!(doc.suppressed_warnings, 40);
}

/// json-v3/SR-002: a number as text and a member in both spellings are reported.
#[test]
fn json_sr002_number_as_text_and_both_spellings_are_reported() {
    let doc = read(&payload(&minimal_status(
        r#","energyInfrastructureSiteStatus":[{"reference":{"targetClass":"FacilityObject","idG":"S","versionG":"1"},"lastUpdated":"2026-11-20T08:05:00+01:00","availableCarParkingPlaces":"12"}]"#,
    )));
    assert_eq!(
        doc.warnings.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [WarningKind::TypeMismatch]
    );

    // The same member in both spellings: the first is kept, the second reported.
    let text = json::mobilithek::to_string(&table()).unwrap();
    let both = text.replacen(
        r#""electricEnergy":["#,
        r#""energyProduct":[{"aegiElectricEnergy":{}}],"electricEnergy":["#,
        1,
    );
    let doc = read(&both);
    assert_eq!(
        doc.warnings.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [WarningKind::Repeated],
        "{:#?}",
        doc.warnings
    );
}

/// json-v3/SR-003: extension content that is not exactly one object is refused.
#[test]
fn json_sr003_extension_content_that_is_not_one_object_is_refused() {
    let doc = common::read(&common::document(&common::accident(
        "<sit:accidentType>collision</sit:accidentType>",
    )));
    for (content, ok) in [
        (r#"{"nl":{"urgency":2}}"#, true),
        (r#"{"a":1},"injected":{"b":2}"#, false),
        (r#"{"a":1}}"#, false),
        (r#"["a"]"#, false),
        (r#"{"a":"#, false),
    ] {
        let mut publication = doc.publications[0].clone();
        let Publication::Situation(p) = &mut publication else {
            unreachable!()
        };
        let v3::situation::SituationRecord::Accident(a) = &mut p.situation[0].situation_record[0]
        else {
            unreachable!()
        };
        a.situation_record_extension = Some(Box::new(datex2::Extension {
            json: content.into(),
            ..Default::default()
        }));
        match json::to_string(&publication) {
            Ok(text) => {
                assert!(ok, "{content} was written: {text}");
                assert_json_valid(&text, JsonSchema::Official);
                assert_eq!(read(&text).publications, vec![publication]);
            }
            Err(error) => {
                assert!(!ok, "{content}: {error}");
                assert_eq!(error.kind, ErrorKind::InvalidExtension, "{error}");
            }
        }
    }
}

fn station(publication: &mut Publication) -> &mut aegi::EnergyInfrastructureStation {
    let Publication::AfirEnergyInfrastructureTable(p) = publication else {
        unreachable!()
    };
    &mut p.energy_infrastructure_table[0].energy_infrastructure_site[0]
        .energy_infrastructure_station[0]
}

fn first_point(publication: &mut Publication) -> &mut aegi::ElectricChargingPoint {
    let aegi::RefillPoint::ElectricChargingPoint(point) = &mut station(publication).refill_point[0]
    else {
        unreachable!()
    };
    point
}

/// json-v3/SR-005: members, required members, literals and faults the profile rejects are refused.
#[test]
fn json_sr005_each_kind_of_profile_refusal_is_named() {
    let refused = |change: &dyn Fn(&mut Publication)| {
        let mut publication = table();
        change(&mut publication);
        json::mobilithek::to_string(&publication)
            .unwrap_err()
            .to_string()
    };
    // A member the profile does not list.
    let error = refused(&|p| station(p).hydrogen = Some(Box::default()));
    assert!(
        error.contains(r#"member "hydrogen", which the profile does not allow here"#),
        "{error}"
    );
    // A required member left out.
    let error = refused(&|p| station(p).number_of_refill_points = None);
    assert!(
        error.contains(
            r#"member "numberOfRefillPoints" that the AFIR Energy Infrastructure profile requires"#
        ),
        "{error}"
    );
    // A member of the publication the profile does not list.
    let error = refused(&|p| {
        let Publication::AfirEnergyInfrastructureTable(p) = p else {
            unreachable!()
        };
        p.feed_description = Some(german("Ladepunkte"));
    });
    assert!(
        error.contains(r#"member "feedDescription", which the profile does not allow here"#),
        "{error}"
    );
    // An enumeration value the profile does not list.
    let error = refused(&|p| first_point(p).delivery_unit = Some(aegi::DeliveryUnitEnum::Litre));
    assert!(error.contains(r#""litre" of "afirEnergyInfrastructure:DeliveryUnitEnum", which the profile does not list"#), "{error}");

    // A fault of another kind than the plain one the profile writes without a wrapper.
    let mut status = status();
    let Publication::AfirEnergyInfrastructureStatus(p) = &mut status else {
        unreachable!()
    };
    p.energy_infrastructure_site_status[0].fault =
        Some(com::AnyFault::VmsControllerFault(Box::default()));
    let mut bytes = Vec::new();
    let container = MessageContainer {
        payload: vec![status],
        ..container(ex::ProtocolTypeEnum::SnapshotPush)
    };
    let error = json::mobilithek::write(&mut bytes, &container)
        .unwrap_err()
        .to_string();
    assert!(error.contains("fault"), "{error}");
}

#[test]
fn plain_fault_is_written_without_its_wrapper_for_the_profile() {
    let mut status = status();
    let Publication::AfirEnergyInfrastructureStatus(p) = &mut status else {
        unreachable!()
    };
    p.energy_infrastructure_site_status[0].fault =
        Some(com::AnyFault::Fault(Box::new(com::Fault {
            fault_identifier: Some("F1".into()),
            fault_creation_time: time("2026-11-20T08:00:00+01:00"),
            fault_last_update_time: time("2026-11-20T08:00:00+01:00"),
            ..Default::default()
        })));
    let container = MessageContainer {
        payload: vec![status.clone()],
        ..container(ex::ProtocolTypeEnum::SnapshotPush)
    };
    let mut bytes = Vec::new();
    json::mobilithek::write(&mut bytes, &container).unwrap_or_else(|e| panic!("{e}"));
    let text = String::from_utf8(bytes).unwrap();
    assert!(
        text.contains(r#""fault":{"faultIdentifier":"F1""#),
        "{text}"
    );
    assert_json_valid(&text, JsonSchema::MobilithekStatus);
    assert_eq!(read(&text).publications, vec![with_profile(status)]);
}

#[test]
fn values_json_cannot_carry_are_refused() {
    for value in [f32::NAN, f32::INFINITY] {
        let mut publication = table();
        first_point(&mut publication).available_charging_power = vec![value];
        let error = json::mobilithek::to_string(&publication).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidValue, "{error}");
        assert!(error.path.ends_with("availableChargingPower"), "{error}");
    }
}

#[test]
fn exchange_information_may_come_before_the_payload() {
    let text = mobilithek_container(ex::ProtocolTypeEnum::SnapshotPush);
    let (head, exchange) = text.split_once(r#","exchangeInformation":"#).unwrap();
    let payload = head.strip_prefix(r#"{"messageContainer":{"#).unwrap();
    let exchange = &exchange[..exchange.len() - 2];
    let swapped =
        format!(r#"{{"messageContainer":{{"exchangeInformation":{exchange},{payload}}}}}"#);
    let doc = read(&swapped);
    assert!(doc.warnings.is_empty(), "{:#?}", doc.warnings);
    assert_eq!(doc.publications, read(&text).publications);
    assert_eq!(doc.exchange_information, read(&text).exchange_information);
}

/// json-v3/SR-001: a million-digit number hits the length limit; hostile member names are made safe.
#[test]
fn json_sr001_huge_numbers_and_hostile_member_names_are_bounded() {
    let mut limits = datex2::Limits::default();
    limits.max_text_len = 1_000;
    let digits = "9".repeat(1_000_000);
    let text = payload(&minimal_status(&format!(
        r#","energyInfrastructureSiteStatus":[{{"availableCarParkingPlaces":{digits}}}]"#
    )));
    let error = json::read_with(text.as_bytes(), &limits).unwrap_err();
    assert_eq!(error.kind, ErrorKind::LimitExceeded, "{error}");

    let hostile = payload(&minimal_status(r#","\u001b[31mred\n\u0000":1"#));
    let doc = read(&hostile);
    let warning = doc.warnings[0].to_string();
    assert!(!warning.chars().any(char::is_control), "{warning:?}");
}

// ---- 009: the Mobilithek wrappers and the money rule ------------------------------------

fn profile_sample(name: &str) -> Option<String> {
    let path = common::schema_dir()
        .parent()?
        .join("afir-mobilithek-01-00-00")
        .join(name)
        .join(format!("{name}_rev2.json"));
    std::fs::read_to_string(path).ok()
}

/// write-roots/SR-003, US2: snapshot and delta are valid for the profile, carry the given time, and say
/// what the profile's own sample says apart from protocol and time.
#[test]
fn sr003_snapshot_and_delta_are_the_profile_sample_with_the_callers_time() {
    let generated: DateTime = "2026-04-14T14:00:00+02:00".parse().unwrap();
    for (container, protocol) in [
        (
            json::mobilithek::snapshot(status(), generated),
            ex::ProtocolTypeEnum::SnapshotPush,
        ),
        (
            json::mobilithek::delta(status(), generated),
            ex::ProtocolTypeEnum::DeltaPush,
        ),
    ] {
        assert_eq!(container.payload, vec![status()]);
        let exchange = container.exchange_information.as_deref().unwrap();
        let context = exchange.exchange_context.as_deref().unwrap();
        assert_eq!(context.coded_exchange_protocol, Some(protocol.clone()));
        let dynamic = exchange.dynamic_information.as_deref().unwrap();
        assert_eq!(dynamic.message_generation_timestamp, Some(generated));

        let text = json::mobilithek::to_string(&container).unwrap();
        assert_json_valid(&text, JsonSchema::MobilithekStatus);

        let Some(sample) = profile_sample("EnergyInfrastructureStatusPublication") else {
            common::skipped(
                "the Mobilithek profile samples are not fetched (run `cargo xtask fetch`)",
            );
            continue;
        };
        let mut expected = *read(&sample).exchange_information.unwrap();
        let ctx = expected.exchange_context.as_mut().unwrap();
        ctx.coded_exchange_protocol = Some(protocol);
        let dynamic = expected.dynamic_information.as_mut().unwrap();
        dynamic.message_generation_timestamp = Some(generated);
        assert_eq!(*exchange, expected);
    }
}

/// write-roots US2-3: the wrapped container is plain data; what the caller adds is written.
#[test]
fn a_wrapped_container_can_be_changed() {
    let mut container =
        json::mobilithek::delta(status(), "2026-04-14T14:00:00+02:00".parse().unwrap());
    let exchange = container.exchange_information.as_mut().unwrap();
    exchange
        .exchange_context
        .as_mut()
        .unwrap()
        .supplier_or_cis_requester
        .as_mut()
        .unwrap()
        .name = Some("Beispiel Laden GmbH".into());
    let text = json::mobilithek::to_string(&container).unwrap();
    assert_json_valid(&text, JsonSchema::MobilithekStatus);
    let again = read(&text);
    assert_eq!(again.exchange_information, container.exchange_information);

    // What the profile leaves out of the exchange information is refused like anything else.
    let exchange = container.exchange_information.as_mut().unwrap();
    exchange
        .dynamic_information
        .as_mut()
        .unwrap()
        .message_sequencing_number = Some(42);
    let e = json::mobilithek::to_string(&container).unwrap_err();
    assert!(
        e.path
            .ends_with("dynamicInformation/messageSequencingNumber"),
        "{e}"
    );
}

/// write-roots/SR-004: an amount with a fraction of a cent or nine digits is refused in every encoding,
/// and the error names AmountOfMoney, its limits, the member and the value. Nothing is rounded.
#[test]
fn sr004_money_breaking_amount_of_money_is_refused_naming_the_type() {
    for bad in ["0.589", "123456789"] {
        let mut official = table_for_3_7();
        for_each_price(&mut official, |price| price.value = money(bad));
        let mut profile = table();
        for_each_price(&mut profile, |price| price.value = money(bad));
        for (encoding, e) in [
            ("XML", v3::to_string(&official).unwrap_err()),
            ("JSON", json::to_string(&official).unwrap_err()),
            (
                "Mobilithek",
                json::mobilithek::to_string(&profile).unwrap_err(),
            ),
        ] {
            assert_eq!(e.kind, ErrorKind::InvalidValue, "{encoding}: {e}");
            assert_eq!(
                e.expected,
                "at most 8 digits, 2 of them after the point, in \"value\" of type AmountOfMoney",
                "{encoding}"
            );
            assert_eq!(e.found, format!("\"{bad}\""), "{encoding}");
        }
    }
    // Trailing zeros are not digits of the value, and the type has no minimum.
    for good in ["0.590", "-0.59", "12345678"] {
        let mut publication = table();
        for_each_price(&mut publication, |price| price.value = money(good));
        let text = json::mobilithek::to_string(&publication).unwrap();
        assert!(
            text.contains(&format!(r#""value":{good}"#)),
            "{good} in {text}"
        );
    }
}
