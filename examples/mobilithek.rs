//! Publishes a charging site for the Mobilithek: its static data as `table.json`, its dynamic
//! data as a snapshot in `status.json`. Both follow the German AFIR Recharging profile.
//!
//! The site has one station with an AC point of 22 kW and a DC point of 150 kW, ad-hoc prices per
//! kWh. Run it with `cargo run --example mobilithek`.

use datex2::v3::afir_energy_infrastructure as aegi;
use datex2::v3::afir_facilities as afac;
use datex2::v3::{Publication, common as com, json, location_referencing as loc};
use datex2::{DateTime, MultilingualString};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let now: DateTime = "2026-11-20T08:00:00+01:00".parse()?;
    let table = Publication::AfirEnergyInfrastructureTable(Box::new(
        aegi::EnergyInfrastructureTablePublication {
            lang: Some("de".into()),
            publication_time: Some(now),
            publication_creator: Some(creator()),
            header_information: Some(header()),
            energy_infrastructure_table: vec![aegi::EnergyInfrastructureTable {
                id: Some("TABLE-1".into()),
                version: Some("1".into()),
                energy_infrastructure_site: vec![site(now)?],
                ..Default::default()
            }],
            ..Default::default()
        },
    ));
    json::mobilithek::write(std::fs::File::create("table.json")?, &table)?;

    let status = Publication::AfirEnergyInfrastructureStatus(Box::new(
        aegi::EnergyInfrastructureStatusPublication {
            lang: Some("de".into()),
            publication_time: Some(now),
            publication_creator: Some(creator()),
            table_reference: vec![reference("TABLE-1")],
            header_information: Some(header()),
            energy_infrastructure_site_status: vec![aegi::EnergyInfrastructureSiteStatus {
                reference: Some(Box::new(reference("SITE-1"))),
                last_updated: Some(now),
                energy_infrastructure_station_status: vec![
                    aegi::EnergyInfrastructureStationStatus {
                        reference: Some(Box::new(reference("STATION-1"))),
                        last_updated: Some(now),
                        refill_point_status: vec![
                            point_status("POINT-AC", aegi::RefillPointStatusEnum::Available, now),
                            point_status("POINT-DC", aegi::RefillPointStatusEnum::Charging, now),
                        ],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        },
    ));
    // A complete snapshot; `json::mobilithek::delta` sends only the points that changed.
    let snapshot = json::mobilithek::snapshot(status, now);
    json::mobilithek::write(std::fs::File::create("status.json")?, &snapshot)?;
    println!("wrote table.json and status.json");
    Ok(())
}

fn site(now: DateTime) -> Result<aegi::EnergyInfrastructureSite> {
    let station = aegi::EnergyInfrastructureStation {
        id: Some("STATION-1".into()),
        version: Some("1".into()),
        last_updated: Some(now),
        operating_hours: Some(afac::OperatingHours::OpenAllHours(Box::default())),
        location_reference: Some(location(52.520008, 13.404954)),
        total_maximum_power: Some(172_000.0),
        number_of_refill_points: Some(2),
        authentication_and_identification_methods: vec![
            aegi::AuthenticationAndIdentificationEnum::CreditCard,
        ],
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
                "0.59",
                now,
            )?,
            charging_point(
                "POINT-DC",
                aegi::CurrentTypeEnum::Dc,
                150_000.0,
                aegi::ConnectorTypeEnum::Iec62196T2COMBO,
                "0.69",
                now,
            )?,
        ],
        ..Default::default()
    };
    Ok(aegi::EnergyInfrastructureSite {
        id: Some("SITE-1".into()),
        version: Some("1".into()),
        last_updated: Some(now),
        operating_hours: Some(afac::OperatingHours::OpenAllHours(Box::default())),
        location_reference: Some(location(52.520008, 13.404954)),
        operator: Some(afac::Organisation::AnOrganisation(Box::new(
            afac::AnOrganisation {
                name: Some(MultilingualString::new("de", "Beispiel Laden GmbH")),
                ..Default::default()
            },
        ))),
        energy_infrastructure_station: vec![station],
        ..Default::default()
    })
}

fn charging_point(
    id: &str,
    current: aegi::CurrentTypeEnum,
    watt: f32,
    connector: aegi::ConnectorTypeEnum,
    euro_per_kwh: &str,
    now: DateTime,
) -> Result<aegi::RefillPoint> {
    let rate = aegi::EnergyRate {
        id: Some(format!("{id}-ADHOC")),
        rate_policy: Some(aegi::RatePolicyEnum::AdHoc),
        last_updated: Some(now),
        applicable_currency: vec!["EUR".into()],
        energy_price: vec![aegi::EnergyPrice {
            price_type: Some(aegi::PriceTypeEnum::PricePerKWh),
            value: Some(euro_per_kwh.parse()?), // a Decimal: written with exactly these digits
            tax_included: Some(true),
            ..Default::default()
        }],
        ..Default::default()
    };
    Ok(aegi::RefillPoint::ElectricChargingPoint(Box::new(
        aegi::ElectricChargingPoint {
            id: Some(id.into()),
            version: Some("1".into()),
            last_updated: Some(now),
            delivery_unit: Some(aegi::DeliveryUnitEnum::KWh),
            energy_product: vec![aegi::AnyEnergyProduct::ElectricEnergy(Box::new(
                aegi::ElectricEnergy {
                    energy_rate: vec![rate],
                    ..Default::default()
                },
            ))],
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
        },
    )))
}

fn point_status(
    id: &str,
    status: aegi::RefillPointStatusEnum,
    now: DateTime,
) -> aegi::AnyRefillPointStatus {
    aegi::AnyRefillPointStatus::ElectricChargingPointStatus(Box::new(
        aegi::ElectricChargingPointStatus {
            reference: Some(Box::new(reference(id))),
            last_updated: Some(now),
            status: Some(status),
            ..Default::default()
        },
    ))
}

fn location(latitude: f32, longitude: f32) -> loc::LocationReference {
    let coordinates = loc::PointCoordinates {
        latitude: Some(latitude),
        longitude: Some(longitude),
        ..Default::default()
    };
    loc::LocationReference::PointLocation(Box::new(loc::PointLocation {
        coordinates_for_display: Some(Box::new(coordinates.clone())),
        point_by_coordinates: Some(Box::new(loc::PointByCoordinates {
            point_coordinates: Some(Box::new(coordinates)),
            ..Default::default()
        })),
        ..Default::default()
    }))
}

fn reference(id: &str) -> com::VersionedReference {
    com::VersionedReference {
        id: Some(id.into()),
        version: Some("1".into()),
    }
}

fn creator() -> Box<com::InternationalIdentifier> {
    Box::new(com::InternationalIdentifier {
        country: Some("DE".into()),
        national_identifier: Some("DE-EXAMPLE".into()),
        ..Default::default()
    })
}

fn header() -> Box<com::HeaderInformation> {
    Box::new(com::HeaderInformation {
        confidentiality: Some(com::ConfidentialityValueEnum::NoRestriction),
        information_status: Some(com::InformationStatusEnum::Real),
        ..Default::default()
    })
}
