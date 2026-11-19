+++
title = "Untrusted input"
description = "How the datex2 reader stays safe on DATEX II feeds you do not control: limits on depth, attributes and token length, no DTDs or entities, log-safe messages, fuzzing, no unsafe code."
weight = 90

[extra]
seo_title = "Parsing untrusted DATEX II input safely"
+++

A traffic feed is input from the network, published by someone else, and often large. The
reader treats it as untrusted.

## Limits

Applied while reading, with defaults far above anything seen in real feeds:

| `Limits` field | Default | Bounds |
|---|---|---|
| `max_depth` | 100 | Element nesting |
| `max_attributes` | 256 | Attributes on one element, namespace declarations included. Namespace declarations in scope may number four times this |
| `max_text_len` | 8 MiB | A single text value, attribute value or extension content. A single tag or run of text in the input may be 64 KiB longer |
| `max_warnings` | 1,000 | Warnings kept; further ones are counted |

Exceeding a limit is an error that names the limit and its value. To raise one, use
`read_with` or `stream_with`:

```rust
fn lenient(xml: &[u8]) -> Result<datex2::v3::Document, datex2::Error> {
    let mut limits = datex2::Limits::default();
    limits.max_text_len = 64 << 20;
    datex2::v3::read_with(xml, &limits)
}
```

The length limit holds for what the parser buffers, not only for what the reader keeps: a text
that never ends is refused after 8 MiB, not after it has filled memory.

## What is refused outright

- **Document type declarations.** A document with `<!DOCTYPE …>` is refused before any content
  is read. That rules out entity expansion attacks and external entities.
- **Custom entities.** Only the five predefined entities and character references are decoded.
- **Encodings other than UTF-8.**

## Names are matched exactly

Elements, attributes and type names are matched by namespace URI and local name, never by
prefix. A prefix rebound to another namespace does not make foreign content look standard; an
attribute named `type` outside the XML Schema instance namespace selects nothing; an `xsi:type`
is only honoured when it names a subtype of the declared type. The two exceptions (Level C
namespaces, and a known element in another DATEX II namespace) are narrow and always reported;
see [Tolerance and warnings](@/docs/tolerance.md).

## Messages are safe to log

Errors and warnings quote the input. Quoted text is cut to 80 characters, element names in
paths are cut too, and control characters are written as escapes, so a hostile document cannot
forge or flood a log line.

## Memory

- A whole read holds about 1.7 times the XML size.
- A [stream](@/docs/streaming.md) holds one record, the parser's buffer and the capped warning
  list, whatever the size of the document.

## Writing

Values you write may come from untrusted sources too. The writer escapes every text and
attribute value, refuses characters XML 1.0 cannot carry, and parses extension content before
copying it; see [Writing](@/docs/writing.md).

## How this is checked

- Every limit and refusal has a test at its boundary.
- Three fuzz targets (read, stream, read-then-write) assert that nothing crashes, hangs, grows
  without bound, or returns unsafe text. The fuzzer found two places where input reached a
  message unescaped; both are fixed and have regression tests.
- The crate forbids `unsafe` code. It opens no files and no sockets.

## The threat model

The full list of threats, each with its control and the tests that are its evidence, and what
is out of scope, is the
[threat model](https://github.com/hupe1980/datex2/blob/main/THREAT_MODEL.md). It is also part
of the [API reference](https://docs.rs/datex2/latest/datex2/threat_model/index.html).

## Reporting a problem

Please report security problems privately through the repository's
[security advisories](https://github.com/hupe1980/datex2/security/advisories).
