//! Prints what a DATEX II v3 file contains and every deviation the reader tolerated.
//!
//! ```text
//! cargo run --release --example inspect -- incidents.xml
//! gzip -dc incidents.xml.gz | cargo run --release --example inspect
//! ```

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};

use datex2::v3::{self, Publication};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Box<dyn BufRead> = match std::env::args().nth(1) {
        Some(path) => Box::new(BufReader::new(std::fs::File::open(path)?)),
        None => Box::new(BufReader::new(std::io::stdin())),
    };
    let doc = v3::read(input)?;

    println!(
        "envelope: {:?}{}",
        doc.envelope,
        if doc.soap { " in SOAP" } else { "" }
    );
    for publication in &doc.publications {
        let (kind, records) = match publication {
            Publication::Situation(p) => ("SituationPublication", p.situation.len()),
            Publication::MeasuredData(p) => ("MeasuredDataPublication", p.site_measurements.len()),
            Publication::ElaboratedData(p) => {
                ("ElaboratedDataPublication", p.physical_quantity.len())
            }
            Publication::MeasurementSiteTable(p) => (
                "MeasurementSiteTablePublication",
                p.measurement_site_table.len(),
            ),
            Publication::Vms(p) => ("VmsPublication", p.vms_controller_status.len()),
            Publication::VmsTable(p) => ("VmsTablePublication", p.vms_controller_table.len()),
            Publication::ParkingTable(p) => ("ParkingTablePublication", p.parking_table.len()),
            Publication::ParkingStatus(p) => (
                "ParkingStatusPublication",
                p.parking_status_information.len(),
            ),
            Publication::EnergyInfrastructureTable(p) => (
                "EnergyInfrastructureTablePublication",
                p.energy_infrastructure_table.len(),
            ),
            _ => ("another publication", 0),
        };
        match publication.publication_time() {
            Some(time) => println!("{kind}: {records} top-level records, published {time}"),
            None => println!("{kind}: {records} top-level records, no publication time"),
        }
    }

    let mut by_kind = BTreeMap::new();
    for warning in &doc.warnings {
        *by_kind.entry(warning.kind).or_insert(0usize) += 1;
    }
    for (kind, count) in &by_kind {
        let first = doc
            .warnings
            .iter()
            .find(|w| w.kind == *kind)
            .expect("counted above");
        println!("{count:>6} × {kind:?}, first: {first}");
    }
    if doc.suppressed_warnings > 0 {
        println!("{:>6} further warnings not kept", doc.suppressed_warnings);
    }
    Ok(())
}
