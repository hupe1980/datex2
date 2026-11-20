+++
title = "JSON and the Mobilithek"
description = "Read and write DATEX II v3 JSON in Rust: the official 3.7 mapping and the German AFIR Recharging profile the Mobilithek requires, validated against both schema sets, with exact numbers and no JSON dependency."
weight = 55

[extra]
seo_title = "DATEX II JSON and Mobilithek AFIR publications in Rust"
+++

## Two spellings, one model

DATEX II 3.7 defines a JSON encoding of the same model as XML. The crate reads and writes it with
the same typed structs, in two spellings:

- **The official 3.7 mapping**, which the 3.7 JSON schemas describe.
- **The German AFIR Recharging profile** ("AFIR Energy Infrastructure" 01-00-00, revision 2),
  which the Mobilithek, Germany's National Access Point, requires for charging point data.

The reader takes both without a setting. The writer produces the one you call.

```rust,no_run
use datex2::v3::json;
use std::{fs::File, io::BufReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = json::read(BufReader::new(File::open("charging-points.json")?))?;
    for warning in &doc.warnings {
        eprintln!("{warning}");
    }
    json::write(std::io::stdout().lock(), &doc.publications[0])?;
    Ok(())
}
```

XML in, JSON out, and the reverse, work the same way: read with `v3::read` or `json::read`, write
with `v3::write`, `json::write` or `json::mobilithek::write`.

## Reading

`json::read` accepts:

- `{"payload": …}` with one publication or an array of them;
- `{"messageContainer": …}` with publications and exchange information, which lands in
  `Document::exchange_information`;
- `{"exchangeInformation": …}` beside or without a container, as the profile allows.

As with XML, the reader is tolerant and says so. A number written as a string, an enumeration
written as a bare string, an integer written as `22000.0`, `null` for a value, a member written
twice: each is accepted where the meaning is clear and listed as a warning. A number with an
exponent, such as `1.5E+2`, is valid JSON and is read as the exact value 150, without a warning.
Unknown members are skipped with a warning. [Limits](@/docs/security.md) apply as for XML:
nesting depth, text length and the warning cap.

## Writing the official 3.7 mapping

```rust,no_run
use datex2::v3::{self, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = v3::read(&std::fs::read("incidents.xml")?[..])?;
    json::write(std::io::stdout().lock(), &doc.publications[0])?;
    Ok(())
}
```

**When `json::write` returns `Ok`, the official 3.7 JSON schemas accept the output.** The rules
are those of the [XML writer](@/docs/writing.md), plus two that only the JSON schemas have: a
language code is two lowercase letters, and a date and time carries a time zone. The tests
check every document they write with Python `jsonschema`, a validator this project did not write.

`json::write_container` writes `{"messageContainer": …}`. DATEX II publishes no JSON schema for
the exchange part, so there the guarantee covers each payload.

## Writing for the Mobilithek

Static data (sites, stations, charging points, prices) is a table publication. Dynamic data
(status, current prices) is a status publication inside a message container, whose exchange
information says whether it is a complete snapshot or a delta:

```rust,no_run
use datex2::v3::exchange_information::{ExchangeContext, ExchangeInformation, ProtocolTypeEnum};
use datex2::v3::{Publication, json, message_container::MessageContainer};

fn publish(table: &Publication, status: Publication) -> Result<(), datex2::Error> {
    json::mobilithek::write(std::fs::File::create("table.json").unwrap(), table)?;
    let delta = MessageContainer {
        payload: vec![status],
        exchange_information: Some(Box::new(ExchangeInformation {
            exchange_context: Some(Box::new(ExchangeContext {
                coded_exchange_protocol: Some(ProtocolTypeEnum::DeltaPush),
                ..Default::default()
            })),
            ..Default::default()
        })),
        ..Default::default()
    };
    json::mobilithek::write_container(std::fs::File::create("status.json").unwrap(), &delta)
}
```

A complete, runnable site with an AC point of 22 kW and a DC point of 150 kW is
[`examples/mobilithek.rs`](https://github.com/hupe1980/datex2/blob/main/examples/mobilithek.rs);
its test validates both files against the profile's schemas.

**When `json::mobilithek::write` returns `Ok`, the profile's JSON schemas accept the output.**
The profile is 3.7 cut down to the AFIR content, but taken from an earlier stage of the model, so
some things are spelled differently. The writer handles them:

| DATEX II 3.7 (the model) | The profile writes |
|---|---|
| `energyProduct: [{"aegiElectricEnergy": {…}}]` | `electricEnergy: [{…}]` |
| `AfirFacilityLocation` in the location extension | `FacilityLocation` |
| `fault: {"comFault": {…}}` | `fault: {…}` |
| `roamingPlatform` on a site | `roamingPlatfom` |
| `regulatedCharacteristics: [{"euVehicleCategory": […]}]` | `euVehicleCategory: […]` |
| connector type `domesticFType` | `domesticF` |
| a time of day `08:00:00` | `08:00:00Z` |

Vehicle categories only the profile lists, such as `l` and `other`, are held as extended values
and written back as the profile's literal. The profile also allows less than 3.7, and requires
more in places. The error says where and what:

```text
value not allowed by the schema: expected content the AFIR Energy Infrastructure profile allows here, found "selfService" of "afirEnergyInfrastructure:ServiceTypeEnum", which the profile does not list (it lists physicalAttendance, unattended) at payload/aegiEnergyInfrastructureTablePublication/energyInfrastructureTable/energyInfrastructureSite/energyInfrastructureStation/serviceType/serviceType (byte 1913)
required content missing: expected member "numberOfRefillPoints" that the AFIR Energy Infrastructure profile requires, found none at payload/aegiEnergyInfrastructureTablePublication/energyInfrastructureTable/energyInfrastructureSite/energyInfrastructureStation (byte 2589)
```

Where the profile's schemas leave out a 3.7 value rule, the writer still applies it: a time zone
is an offset such as `+01:00`, not `Europe/Berlin`, and an amount of money has at most two
decimal places. Both are what the profile's own descriptions say.

When the publication does not set a profile name and version, the writer puts the profile's.
One member, `refillPointByReference` on a station, exists only in the profile; the 3.7 model
cannot hold it, so it is read as an unknown member with a warning.

## Numbers are exact

A [`Decimal`](https://docs.rs/datex2/latest/datex2/struct.Decimal.html), used for money, keeps
the digits it was given: `0.590` is written as `0.590`, never as `0.59000000000000004`. An `f32`
is written as the shortest text that reads back to the same value: `22000`, `52.520008`. No
global JSON setting is involved, and the crate does not depend on a JSON library.

## Extension content stays in its encoding

Extension content keeps the encoding it was read from. Content read from XML is XML text and is
not written as JSON, and the reverse; the writer refuses it rather than invent a translation.
