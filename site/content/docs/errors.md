+++
title = "Errors"
description = "When a datex2 read or write fails and what the error says: element path, byte offset, what was expected and what was found, on one line that is safe to log."
weight = 80

[extra]
seo_title = "Errors when reading and writing DATEX II"
+++

## Shape

Reading and writing share one error type. An `Error` has five public fields and prints on one
line:

```text
malformed XML: expected the end tag of "validity", found the end of the input at payload/situation/situationRecord/validity (byte 1101)
```

| Field | Content |
|---|---|
| `kind` | The class of failure, an `ErrorKind` |
| `path` | The element path from the root, local names joined by `/` |
| `offset` | Reading: the byte offset in the input. Writing: the number of bytes written before the failure |
| `expected` | What should have been there |
| `found` | What was there. Text from the input is cut to 80 characters and control characters are escaped |

`Error` implements `std::error::Error`, so `?` works with `Box<dyn Error>` and with `anyhow`.

## When reading fails

The reader is [tolerant](@/docs/tolerance.md). It fails only when the document cannot be read
at all:

| `ErrorKind` | Meaning |
|---|---|
| `Syntax` | Not well-formed XML, or the input ended early |
| `NotDatex2` | XML, but not DATEX II v3. The error names the root element; for a version 2 document it says so |
| `UnknownPublication` | The payload names a publication type that does not exist, or names none and its content does not identify one |
| `UnexpectedPublication` | A stream was asked for records that this publication cannot hold |
| `LimitExceeded` | One of the [limits](@/docs/security.md) was exceeded; the error names the limit and its value |
| `DoctypeNotAllowed` | The document has a document type declaration or refers to a custom entity |
| `UnsupportedEncoding` | The document is not UTF-8 |
| `Io` | The underlying reader failed |

## When writing fails

The writer is [strict](@/docs/writing.md):

| `ErrorKind` | Meaning |
|---|---|
| `MissingRequired` | A required element or attribute is `None` |
| `InvalidCount` | A list has fewer or more entries than the schema allows |
| `InvalidValue` | A value breaks a length, range, digit or pattern rule, or holds a character XML cannot carry |
| `InvalidExtension` | Extension content is not well-formed or not safe to copy |
| `Io` | The sink failed |

## Handling by kind

```rust
use datex2::ErrorKind;

fn explain(error: &datex2::Error) -> &'static str {
    match error.kind {
        ErrorKind::NotDatex2 => "this is not a DATEX II v3 file",
        ErrorKind::Syntax => "the file is damaged or incomplete",
        ErrorKind::LimitExceeded => "the file is larger or deeper than the configured limits",
        _ => "the file could not be read",
    }
}
```

`ErrorKind` and `WarningKind` may gain variants in later releases; include a `_` arm.
