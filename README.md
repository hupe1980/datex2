# datex2

Read, stream and write [DATEX II](https://datex2.eu/), the European standard for road traffic and
travel information, as typed Rust data, in XML and JSON.

**Documentation: <https://hupe1980.github.io/datex2/>** · [API reference](https://docs.rs/datex2)

**Status: early.** DATEX II **version 3**, XML and JSON, including the German AFIR Recharging
profile for the Mobilithek. Not yet: DATEX II version 1 and 2.

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

No version, namespace, schema or envelope has to be stated. A byte slice works as well as a file.

## Why this crate

- **Every publication of DATEX II 3.7** as plain structs and enums named as in the standard:
  situations, traffic data, VMS, parking, energy infrastructure (including the AFIR namespaces),
  facilities, traffic regulations, controlled zones and more.
- **A reader that survives real feeds, and tells you.** National feeds deviate from the standard:
  Level C namespaces, a payload without `xsi:type`, required elements left out, a decimal comma
  in a number. The reader accepts these and reports each as a warning with its kind, the element
  path and the byte offset. Nothing is dropped silently and no value is made up.
- **A strict writer.** When `write` succeeds, the official 3.7 schema accepts the document.
  When it cannot, the error names the element and the rule.
- **JSON, including the Mobilithek.** The official 3.7 JSON mapping and the AFIR Recharging
  profile the Mobilithek requires, read and written from the same model. Money keeps its exact
  digits; no JSON library is involved.
- **Bounded memory for large feeds.** `stream` delivers one record at a time: 8 MB for a 189 MB
  file.
- **Safe on untrusted input.** Depth, attributes and token length are bounded, document type
  declarations are refused, messages are safe to log, the reader is fuzzed, and there is no
  `unsafe` code. One dependency; no I/O of its own. The [threat model](THREAT_MODEL.md) lists every
  threat with its control and its tests.

## Large files: one record at a time

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

You name the record type: `Situation`, `SituationRecord`, `MeasurementSite`,
`SiteMeasurements`, `VmsControllerStatus`, `EnergyInfrastructureSite` and 90 more. Asking for a
type that is not a record does not compile.
[More on streaming](https://hupe1980.github.io/datex2/docs/streaming/).

## Writing

```rust,no_run
use datex2::v3::{self, Publication, common, situation::SituationPublication};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let publication = Publication::Situation(Box::new(SituationPublication {
        lang: Some("en".into()),
        publication_time: Some("2026-11-19T12:00:00Z".parse()?),
        publication_creator: Some(Box::new(common::InternationalIdentifier {
            country: Some("NL".into()),
            national_identifier: Some("EXAMPLE".into()),
            ..Default::default()
        })),
        ..Default::default()
    }));
    v3::write(std::io::stdout().lock(), &publication)?;
    Ok(())
}
```

Namespaces, prefixes, `xsi:type`, element order and fixed attributes are the writer's business.
What the schema would reject is an error:

```text
required content missing: expected required element "sit:probabilityOfOccurrence", found none at payload/situation/situationRecord (byte 1432)
```

[More on writing](https://hupe1980.github.io/datex2/docs/writing/), including what the guarantee
does not cover.

## JSON and the Mobilithek

```rust,no_run
use datex2::v3::json;
use std::{fs::File, io::BufReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Either spelling: the official 3.7 mapping or the Mobilithek's AFIR profile.
    let doc = json::read(BufReader::new(File::open("charging-points.json")?))?;
    // For the Mobilithek; what the profile's schemas would reject is an error.
    json::mobilithek::write(std::io::stdout().lock(), &doc.publications[0])?;
    Ok(())
}
```

Static data is a table publication; dynamic data a status publication in a message container
marked as snapshot or delta. [`examples/mobilithek.rs`](examples/mobilithek.rs) publishes a site
with an AC and a DC charging point. When a write succeeds, the profile's schemas accept the
output; where the profile spells something differently from 3.7, the writer handles it.
[More on JSON](https://hupe1980.github.io/datex2/docs/json/).

## Tested on

All 33 version 3 samples fetched on 2026-11-19 from open feeds of the Netherlands (NDW), Finland
(Digitraffic), Spain (DGT), France (DiaLog) and Belgium (Vlaams Verkeerscentrum) read without
error. Twelve AFIR recharging feeds from the Mobilithek read without error; nine are written back
for the profile, and three are refused with the rule they break (a time zone name where the
profile wants an offset, a price with ten decimal places). Everything written is validated with
`xmllint` and Python `jsonschema`, against schemas this project did not write.
[Tested feeds and known limits](https://hupe1980.github.io/datex2/docs/feeds/).

## Measured

Apple M4 Pro, Rust 1.100, release build, one core:

| What | Result |
|---|---|
| Reading a 189 MB situation publication | 1.2 s, about 158 MB/s, 320 MB peak memory |
| Streaming it, situation by situation | 0.9 to 1.2 s, 8 MB peak memory |
| Writing it back | 1.0 s, about 189 MB/s |
| Reading the same situations as JSON (124 MB) | 1.4 s, about 90 MB/s |
| Writing them as JSON | 0.4 to 0.5 s, about 270 MB/s |
| Clean debug build of the crate | 18 to 21 s |

Minimum supported Rust version: 1.86.

## Look at a file

```text
cargo run --release --example inspect -- incidents.xml
gzip -dc incidents.xml.gz | cargo run --release --example inspect
```

prints the envelope, each publication with its record count, and the warnings grouped by kind.

## Developing

```text
cargo xtask fetch            # download the official schemas and verify their checksums
cargo xtask codegen --check  # the committed model is what the schemas produce
cargo test                   # validation uses xmllint and Python jsonschema when installed
```

The typed model is generated from the schemas published at <https://docs.datex2.eu/>; the schemas
themselves are not part of this repository.
[How the code is generated](https://hupe1980.github.io/datex2/docs/codegen/).

## Unofficial

This crate is not affiliated with or endorsed by the DATEX II organisation.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
