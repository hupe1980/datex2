+++
title = "Measurements"
description = "Measured speed, memory use and build time of the datex2 crate on real DATEX II feeds: 158 MB/s reading, 8 MB to stream a 189 MB file, 9 s clean build."
weight = 110

[extra]
seo_title = "DATEX II parsing speed and memory in Rust"
+++

All figures: Apple M4 Pro, Rust 1.100, 2026-11-19, release build, one core. The input is the
Dutch NDW roadworks and events feed, 189 MB of XML with 13,140 situations, unless stated.

| What | Result |
|---|---|
| Reading the whole file (`v3::read`) | 1.2 s, about 158 MB/s; 320 MB peak memory |
| Reading a 70 MB measured-data file | 0.5 s, about 134 MB/s; 94 MB peak memory |
| Streaming the file situation by situation (`v3::stream`) | 0.9 to 1.2 s; **8 MB** peak memory |
| Writing the publication back (`v3::to_string`, 181 MB) | 1.0 s, about 189 MB/s |
| Clean debug build of the crate | 9 s |
| Clean release build | about 20 s |

## Reproducing them

```sh
cargo build --release --examples
/usr/bin/time -l target/release/examples/inspect big.xml     # whole read
/usr/bin/time -l target/release/examples/stream              # reads ./roadworks.xml
```

`/usr/bin/time -l` is the macOS form; on Linux use `/usr/bin/time -v`.

## Why the build is fast

The model has about 670 structs and 420 enums, with a reader and a writer each. The code is
generated as plain, non-generic functions: no derive macros beyond `Debug`, `Clone`, `Default`
and `PartialEq`, and no serde. Element names are matched as integers, so a reader is a `match`
on constants.

## Why streaming memory is flat

A stream reads one record with the ordinary reader and hands it over. Between records it keeps
the parser's buffer, which is bounded by `Limits::max_text_len`, and the capped warning list.
A test with a counting allocator checks that peak heap use is the same for 200 and for 20,000
records.
