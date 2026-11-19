+++
title = "Getting started"
description = "Install the datex2 crate and read a DATEX II v3 traffic feed into typed Rust data in five lines: situations, warnings, streaming large files and writing a document."
weight = 10

[extra]
seo_title = "Getting started with DATEX II in Rust"
+++

## Install

```sh
cargo add datex2
```

The crate needs Rust 1.86 or newer and has one dependency (`quick-xml`). There are no feature
flags to choose.

## Read a file

Any DATEX II v3 XML document works: a national incident feed, a measurement file, a
charging-point table. This reads one and prints its situations.

```rust,no_run
use datex2::v3::{self, Publication};
use std::{fs::File, io::BufReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = v3::read(BufReader::new(File::open("incidents.xml")?))?;
    for publication in &doc.publications {
        if let Publication::Situation(p) = publication {
            println!("{} situations", p.situation.len());
        }
    }
    for warning in &doc.warnings {
        eprintln!("{warning}");
    }
    Ok(())
}
```

You did not state a version, a namespace or an envelope. The reader finds out. A byte slice
works as well as a file: `v3::read(&bytes[..])`.

If you have no file at hand, the Dutch road authority publishes open feeds at
[opendata.ndw.nu](https://opendata.ndw.nu/); `actueel_beeld.xml.gz` holds current situations.
Feeds are usually gzip-compressed; decompress them first, or wrap the file in a decoder such as
`flate2::read::GzDecoder` inside the `BufReader`.

## Look inside

Every publication type is a struct with public fields named as in the standard.
A situation holds situation records; a record is an accident, roadworks, a closure and so on.
Fields that all kinds share are available without matching on the kind:

```rust
use datex2::v3::situation::{SituationPublication, SituationRecord};

fn print(publication: &SituationPublication) {
    for situation in &publication.situation {
        for record in &situation.situation_record {
            // Shared by every kind of record:
            println!("{:?} valid: {:?}", record.id(), record.validity().map(|v| &v.validity_status));
            // Specific to one kind:
            if let SituationRecord::Accident(accident) = record {
                println!("  accident: {:?}", accident.accident_type);
            }
        }
    }
}
```

Every single-valued field is an `Option`, also where the standard calls it required. Real feeds
leave required content out; the reader tells you with a warning instead of failing or inventing
a value. [Tolerance and warnings](@/docs/tolerance.md) explains the rule.

## See what a file contains

The repository has an example that prints the envelope, the publications with their record
counts, and the warnings grouped by kind:

```sh
cargo run --release --example inspect -- incidents.xml
gzip -dc incidents.xml.gz | cargo run --release --example inspect
```

## Next

- A feed of hundreds of megabytes: [Streaming](@/docs/streaming.md).
- Publishing your own data: [Writing](@/docs/writing.md).
- New to the standard: [DATEX II in ten minutes](@/docs/datex2.md).
