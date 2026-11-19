+++
title = "Streaming"
description = "Stream DATEX II v3 records one at a time with datex2::v3::stream: situations, measurement sites, charging sites and 90 more record types in bounded memory."
weight = 40

[extra]
seo_title = "Streaming large DATEX II feeds in Rust"
+++

National feeds reach hundreds of megabytes. `stream` delivers the records of a document one at a
time and keeps only one in memory.

```rust,no_run
use datex2::v3::{self, situation::Situation};
use std::{fs::File, io::BufReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = BufReader::new(File::open("roadworks.xml")?);
    for situation in v3::stream::<Situation>(file)? {
        let situation = situation?;
        println!(
            "{:?}: {} records",
            situation.id,
            situation.situation_record.len()
        );
    }
    Ok(())
}
```

Streaming the 189 MB Dutch roadworks feed this way takes 8 MB of memory and the same time as a
whole read, which takes 320 MB.

## You name the record type

Feeds nest their bulk at different depths: a situation publication holds situations directly; a
measurement site table holds one table with twenty thousand sites; a charging publication holds
one table with thousands of sites. So the stream does not guess what a record is. You ask for a
type, and get every object of that type that the publication holds:

| Ask for | From | You get |
|---|---|---|
| `situation::Situation` | Situation publication | Each situation |
| `situation::SituationRecord` | Situation publication | Each record of each situation |
| `road_traffic_data::SiteMeasurements` | Measured data | The measurements of each site |
| `road_traffic_data::MeasurementSite` | Measurement site table | Each site of each table |
| `road_traffic_data::PhysicalQuantity` | Elaborated data | Each quantity |
| `vms::VmsControllerStatus` | VMS publication | Each controller's status |
| `vms::VmsController` | VMS table publication | Each controller of each table |
| `energy_infrastructure::EnergyInfrastructureSite` | Energy infrastructure table | Each site of each table |
| `parking::HierarchyElementGeneral` | Parking table | Each element of each table |
| `v3::Publication` | Any document | Each payload of a message container |

96 types can be streamed: every type the standard holds as a repeated object under a
publication, directly or inside up to two containers. They are listed under
[`Record`](https://docs.rs/datex2/latest/datex2/v3/trait.Record.html) in the API reference.

- Asking for a type that is not a record **does not compile**.
- Asking a document for records its publication cannot hold (situations from a VMS file) is an
  **error when the stream is created**, not an empty result. Inside a message container, a
  payload of another kind is skipped with a warning.

## What is delivered and what is not

Records are equal to the ones a whole read returns; the test suite compares them on real feeds.

Content of the publication outside the records is passed over: the publication time, header
information, the identifier of a table. If you need those, read the document whole, or stream
`Publication` from a message container.

## Warnings

The stream tolerates and reports the same deviations as `read`. Keep the stream in a variable
to get at them:

```rust
use datex2::v3::{self, situation::SituationRecord};

fn count(xml: &[u8]) -> Result<usize, datex2::Error> {
    let mut records = v3::stream::<SituationRecord>(xml)?;
    let mut count = 0;
    while let Some(record) = records.next() {
        record?;
        count += 1;
        for warning in records.take_warnings() {
            eprintln!("{warning}");
        }
    }
    Ok(count)
}
```

`take_warnings` empties the list. On a long stream, take them regularly: the list is capped at
`Limits::max_warnings` (1,000), after which warnings are only counted in
`suppressed_warnings()`.

## Errors in the middle

With a stream, a failure can arrive after work has been done.

- Input that is not DATEX II v3 fails when the stream is created.
- A failure further in (a truncated download, a limit exceeded) is delivered as one `Err` item,
  after the records before it.
- After an error, and after the last record, the stream yields nothing more.

So `for record in stream { let record = record?; … }` processes everything before the break and
then returns the error.

## Stopping early

The stream reads no further than the record it has just delivered. Drop it and the rest of the
input is never read.
