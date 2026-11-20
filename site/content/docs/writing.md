+++
title = "Writing"
description = "Write schema-valid DATEX II v3 XML from Rust with datex2::v3::write: namespaces, xsi:type and element order handled for you, and exact errors for every schema rule."
weight = 50

[extra]
seo_title = "Writing valid DATEX II v3 XML in Rust"
+++

## One function

Fill the typed model and write it:

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

`write` takes any `io::Write`; `to_string` returns the document as a `String`. Every struct of
the model implements `Default`, so you name only the fields you set.

## A publication or a message container

The same functions take a `&MessageContainer` and write an Exchange 2020 `con:messageContainer`:
publications with exchange information (sender, snapshot or delta, generation time):

```rust,no_run
use datex2::v3::exchange_information::{
    DynamicInformation, ExchangeContext, ExchangeInformation, ExchangeStatusEnum, ProtocolTypeEnum,
};
use datex2::v3::{self, Publication, message_container::MessageContainer};

fn deliver(situations: Vec<Publication>) -> Result<String, Box<dyn std::error::Error>> {
    let container = MessageContainer {
        payload: situations,
        exchange_information: Some(Box::new(ExchangeInformation {
            exchange_context: Some(Box::new(ExchangeContext {
                coded_exchange_protocol: Some(ProtocolTypeEnum::SnapshotPush),
                exchange_specification_version: Some("3.0".into()),
                supplier_or_cis_requester: Some(Box::default()),
                ..Default::default()
            })),
            dynamic_information: Some(Box::new(DynamicInformation {
                exchange_status: Some(ExchangeStatusEnum::Online),
                message_generation_timestamp: Some("2026-11-20T08:00:00+01:00".parse()?),
                ..Default::default()
            })),
            ..Default::default()
        })),
        ..Default::default()
    };
    Ok(v3::to_string(&container)?)
}
```

Every member set above is required. For the Mobilithek,
[`snapshot` and `delta`](@/docs/json.md#writing-for-the-mobilithek) build it in one call.

## What the writer takes care of

- the XML declaration and the `d2:payload` or `con:messageContainer` root;
- namespace declarations, with the standard's usual prefixes;
- `xsi:type` on the payload and on every polymorphic object;
- the order of elements, which the schema fixes;
- attributes with a fixed value: `modelBaseVersion="3"`, and `targetClass` on references;
- escaping of all text and attribute values.

## The guarantee

**When `write` returns `Ok`, the official DATEX II 3.7 schema accepts the document**, and for a
message container the official Exchange 2020 schema as well. The test suite does not take the
writer's word for it: `xmllint` validates every document the tests write against the official
schemas.

One exception: the schema's uniqueness constraints (for example, that no two situations in a
publication share an `id`) are not checked.

## What it refuses

The model lets you leave required fields empty, because real feeds do and the
[reader is tolerant](@/docs/tolerance.md). The schema does not, and the writer is strict. It
returns an error naming the element and the rule:

```text
required content missing: expected required element "sit:probabilityOfOccurrence", found none at payload/situation/situationRecord (byte 1432)
wrong number of elements: expected at least 1 of element "sit:situationRecord", found 0 at payload/situation (byte 512)
value not allowed by the schema: expected 0 to 359 in "bearing" of type AngleInDegrees, found 360 at payload/…/pointByCoordinates (byte 1711)
value not allowed by the schema: expected at most 8 digits, 2 of them after the point, in "value" of type AmountOfMoney, found "0.589" at payload/…/energyRate/energyPrice (byte 4065)
```

| Rule | Example |
|---|---|
| Required element or attribute absent | A situation record without `probabilityOfOccurrence` |
| Too few or too many entries in a list | A situation without a record; more than seven days in a weekly pattern |
| Text too long | More than 1,024 characters in an ordinary text field |
| Number out of range | A bearing of 360 |
| Too many digits | A price with three decimal places |
| Wrong form | A currency that is not three capital letters; a language code such as `english!`; an IP address; a time-zone offset |
| A character XML cannot carry | A control character in a text |
| Unsafe extension content | See below |

A rule from a named schema type names it (`AngleInDegrees`, `AmountOfMoney`): look it up in the
standard, or as the field's type in the model.

This makes the writer a practical way to find out what a publication needs: write it, read the
error, fill the field. The writer never rounds, truncates or pads a value to make it fit.

A document that was read with warnings about missing required content will not write back
unchanged. That is intended: the crate does not turn an invalid feed into a document that claims
to be valid.

## Round trips

A document that reads without warnings is written back to one that reads as an equal value.
That includes extended enumeration values, timestamps with any fraction and offset, exact
decimal amounts, and extension content from national namespaces.

## Extension content

[`Extension`](https://docs.rs/datex2/latest/datex2/struct.Extension.html) holds raw XML. Before
copying it into the output, the writer parses it and refuses content that is not well-formed,
that would close the extension element, that holds a document type declaration or a custom
entity, or that has text directly inside the extension element (the schema allows only
elements there). The namespace bindings kept with the content are declared on the extension
element.

## Form of the output

A `d2:payload` or a `con:messageContainer` root. Compact: no indentation or line breaks; pipe it
through `xmllint --format` to read it. SOAP envelopes are not written. For JSON, see
[JSON and the Mobilithek](@/docs/json.md).
