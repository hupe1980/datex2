+++
title = "Documentation"
description = "How to read, stream and write DATEX II v3 road traffic data in Rust with the datex2 crate, in XML and JSON: the typed model, tolerance and warnings, the strict writers, the Mobilithek profile, limits and tested feeds."
sort_by = "weight"
template = "section.html"
page_template = "page.html"
weight = 0

[extra]
nav_title = "Overview"
+++

`datex2` is a Rust crate for [DATEX II](@/docs/datex2.md), the European standard for
exchanging road traffic and travel information. It turns DATEX II version 3, XML or JSON, into
typed Rust data and back.

## Where to start

| If you want to | Read |
|---|---|
| Get data out of a file in five minutes | [Getting started](@/docs/getting-started.md) |
| Understand what DATEX II is and how its documents are built | [DATEX II in ten minutes](@/docs/datex2.md) |
| Read a whole document | [Reading](@/docs/reading.md) |
| Process a feed of hundreds of megabytes | [Streaming](@/docs/streaming.md) |
| Publish a valid document | [Writing](@/docs/writing.md) |
| Read or write JSON, or publish for the Mobilithek | [JSON and the Mobilithek](@/docs/json.md) |
| Find a type or a field | [The typed model](@/docs/model.md) |

## What to expect

| Topic | Read |
|---|---|
| What the reader accepts from real feeds and how it tells you | [Tolerance and warnings](@/docs/tolerance.md) |
| What an error looks like and when a read fails | [Errors](@/docs/errors.md) |
| Why it is safe to point at feeds you do not control | [Untrusted input](@/docs/security.md) |
| Which national feeds it was tested on, and the known limits | [Tested feeds and limits](@/docs/feeds.md) |
| Speed, memory and build time | [Measurements](@/docs/performance.md) |

## Scope

The crate handles **DATEX II version 3**: all 19 payload namespaces of release 3.7, read from
documents of any 3.x release, in XML and in JSON (the 3.7 mapping and the Mobilithek's AFIR
Recharging profile). It does not handle DATEX II version 1 or 2, or any network transport. See [the FAQ](@/docs/faq.md) for the reasons and the plans.

The crate is unofficial: it is not affiliated with or endorsed by the DATEX II organisation.
