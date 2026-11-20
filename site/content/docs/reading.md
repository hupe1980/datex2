+++
title = "Reading"
description = "Read a DATEX II v3 XML document into typed Rust data with datex2::v3::read: the Document, publications, envelopes, shared fields of polymorphic types and warnings."
weight = 30

[extra]
seo_title = "Reading DATEX II v3 XML in Rust"
+++

## One function

```rust,no_run
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = std::io::BufReader::new(std::fs::File::open("incidents.xml")?);
    let doc = datex2::v3::read(file)?;
    println!("{} publications, {} warnings", doc.publications.len(), doc.warnings.len());
    Ok(())
}
```

`read` takes anything that implements `BufRead`: a file in a `BufReader`, a byte slice, a
decompressing reader. It returns a `Document`:

| Field | Content |
|---|---|
| `publications` | The publications found, in document order. One for a `payload`; none, one or several for a `messageContainer`. |
| `envelope` | The shape of the document: `Payload`, `MessageContainer` or `Publication`. |
| `soap` | Whether the document was wrapped in a SOAP envelope. |
| `warnings` | Every deviation the reader tolerated. |
| `suppressed_warnings` | How many further warnings were counted but not kept. |

`read` holds the whole result in memory, about 1.7 times the size of the XML. For large feeds
use [`stream`](@/docs/streaming.md).

## What it accepts

| Document | Result |
|---|---|
| Root `payload` with `xsi:type` | One publication |
| Root `payload` without `xsi:type` | One publication, its type inferred from its content, and a warning |
| Root `messageContainer` | Every payload in it, and its exchange information in `exchange_information` |
| A root named after the publication type (`SituationPublication`) | One publication |
| SOAP 1.1 or 1.2 around a payload or message container | The same as without SOAP |
| Anything else | An error naming the root element and its namespace |

Namespace prefixes do not matter; names are matched by namespace URI. A byte order mark, either
line-end style and a missing XML declaration are all fine. The input must be UTF-8.

## Publications

`Publication` is an enum with one variant per publication type. Match on the ones you handle:

```rust
use datex2::v3::Publication;

fn describe(publication: &Publication) -> String {
    match publication {
        Publication::Situation(p) => format!("{} situations", p.situation.len()),
        Publication::MeasuredData(p) => format!("{} site measurements", p.site_measurements.len()),
        Publication::Vms(p) => format!("{} sign controllers", p.vms_controller_status.len()),
        _ => "another kind of publication".to_owned(),
    }
}
```

What every publication has is available on the enum itself:

```rust
fn header(publication: &datex2::v3::Publication) {
    println!("{:?} {:?}", publication.lang(), publication.publication_time());
    if let Some(creator) = publication.publication_creator() {
        println!("{:?}/{:?}", creator.country, creator.national_identifier);
    }
}
```

## Polymorphic types

Where the standard has an abstract class, the crate has an enum of its concrete kinds, chosen by
the document's `xsi:type`. `SituationRecord` has 28 kinds, `LocationReference` 10. Each such enum
has an accessor for every field its kinds share, so common questions need no `match`:

```rust
use datex2::v3::situation::SituationRecord;

fn is_active(record: &SituationRecord) -> bool {
    use datex2::v3::common::ValidityStatusEnum::Active;
    record.validity().and_then(|v| v.validity_status.as_ref()) == Some(&Active)
}
```

An accessor returns `Option<&T>` for a single value, `Option<&str>` for text and `&[T]` for a
list.

## Values

| In the document | In Rust |
|---|---|
| Text | `String` |
| Multilingual text | `MultilingualString`, with `get("en")` and `first()` |
| Enumeration | An enum; a value outside the standard list is `Extended(String)` |
| Timestamp | `DateTime`, kept exactly as written: nine fractional digits, the offset, or no offset at all |
| Decimal amount (prices) | `Decimal`, the digits as written, never a float |
| Numbers | `f32`, `f64`, `i32`, `i64`, `u64`; measure types are aliases such as `KilometresPerHour = f32` |
| Reference to another object | `VersionedReference { id, version }` or `Reference { id }` |
| Extension content | `Extension`, the content as written, with the namespace bindings it uses |

A `DateTime` without a time zone has `offset_minutes == None` and no `unix_timestamp()`; the
reader does not guess a zone.

## Limits

`read` applies default [limits](@/docs/security.md) to the input. To change one:

```rust
fn read_deep(xml: &[u8]) -> Result<datex2::v3::Document, datex2::Error> {
    let mut limits = datex2::Limits::default();
    limits.max_depth = 200;
    datex2::v3::read_with(xml, &limits)
}
```
