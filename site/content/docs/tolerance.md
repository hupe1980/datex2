+++
title = "Tolerance and warnings"
description = "Which deviations from the DATEX II standard the datex2 reader accepts in real national feeds, how each is reported as a warning, and what is never guessed."
weight = 70

[extra]
seo_title = "Tolerance and warnings for real DATEX II feeds"
+++

## The rule

Real feeds deviate from the standard. A reader that fails on the first deviation is useless
across Europe; a reader that silently papers over them hides problems. So:

1. **The reader accepts every deviation it knows**, and reads the rest of the document.
2. **Every deviation is reported** as a warning with its kind, the element path and the byte
   offset.
3. **Nothing is dropped silently.** What the reader skips, it reports.
4. **No value is made up.** What the document does not say stays `None`.

A read fails only when a document cannot be read at all; see [Errors](@/docs/errors.md).

## Reading the warnings

```rust
fn report(doc: &datex2::v3::Document) {
    for warning in &doc.warnings {
        eprintln!("{warning}");
        // or use the parts: warning.kind, warning.path, warning.offset, warning.detail
    }
    if doc.suppressed_warnings > 0 {
        eprintln!("{} more warnings were not kept", doc.suppressed_warnings);
    }
}
```

A warning prints on one line:

```text
missing element: required element "probabilityOfOccurrence" at payload/situation/situationRecord (byte 1476)
unknown element skipped: "parkingRecordStatus" in namespace parking at payload/parkingRecordStatus (byte 687)
```

At most `Limits::max_warnings` (1,000) warnings are kept; the rest are counted.

## What is tolerated

Each row was observed in a national feed and has a test of its own.

| Deviation | Seen in | What happens | Warning |
|---|---|---|---|
| Namespaces `http://levelC/schema/3/…` in place of the standard ones | ES, AT | Read as the standard namespace | `LevelCNamespace`, once per namespace |
| A known element in another DATEX II namespace than the schema says | BE | Accepted when its name identifies exactly one expected element | `WrongNamespace` |
| Root `payload` in no namespace | ES | Accepted when it carries a DATEX II `xsi:type` | `WrongNamespace` |
| No `xsi:type` on the payload | BE | The publication type is inferred from the first element only one type has | `MissingXsiType` |
| Required element or attribute absent | FI, BE, AT, NL, FR, ES | The field is `None` | `MissingElement`, `MissingAttribute` |
| A value of the wrong form, such as `79,06` for an integer | AT | The field is `None` | `InvalidValue` |
| An empty element where a value belongs | FI | The field is `None` | `EmptyValue` |
| A timestamp without time zone | DE | Kept as written, without offset | `NoTimeZone` |
| An enumeration value outside the standard list | — | Kept as `Extended` | `UnknownEnumValue` |
| An `xsi:type` the model does not know (a national subtype) | ES | Read as the declared type if that is concrete, otherwise skipped | `UnknownType` |
| An `xsi:type` where the type has no alternatives | — | Ignored | `UnexpectedType` |
| An unknown element or attribute | NL, ES, DE | Skipped | `UnknownElement`, `UnknownAttribute` |
| An element repeated where one is allowed | — | The first is kept | `Repeated` |

## What needs no tolerance

These are valid XML and are simply read correctly, without a warning: any namespace prefixes,
default namespaces and redeclarations; the XML Schema instance namespace under a prefix other
than `xsi`; a byte order mark; Windows line ends; a missing or single-quoted XML declaration;
timestamps with zero to nine fractional digits and any offset; identifiers with spaces,
semicolons and non-ASCII letters; a malformed `xsi:schemaLocation` (it is never used);
extension content from foreign namespaces (it is kept).

## What is never done

- **No default values.** A missing speed is not `0.0`.
- **No guessed time zones.**
- **No renaming.** An element the model does not know is reported, not mapped to a similar one.
- **No foreign content as standard content.** An element in a namespace that is not DATEX II is
  never read into a standard field, whatever its name.

## From warnings to a validity check

The warnings tell you how far a feed is from the standard. If you need a hard answer, write the
publication back: the [writer](@/docs/writing.md) refuses anything the schema would reject and
says why.
