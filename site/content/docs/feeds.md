+++
title = "Tested feeds and limits"
description = "The national DATEX II v3 feeds datex2 was tested on (Netherlands, Finland, Spain, France, Belgium), what was not verified, and the known limits of this release."
weight = 100

[extra]
seo_title = "Tested national DATEX II feeds and known limits"
+++

A DATEX II library is only as good as its agreement with what is actually published. This page
says what the crate was tested on, on which date, and where it stops.

## Tested on

Samples fetched on 2026-11-19 from open feeds. All 33 version 3 XML samples from these five
sources read without error, and streaming them gives the same records as reading them whole.

| Source | What the samples hold | Remarks |
|---|---|---|
| Netherlands, [NDW open data](https://opendata.ndw.nu/) | Situations, measured data, measurement site tables, VMS, emission zones, truck parking status | `messageContainer` envelopes; files up to 189 MB |
| Finland, [Fintraffic Digitraffic](https://www.digitraffic.fi/en/road-traffic/) | Roadworks and announcements as 3.5 and 3.7, measured data, site tables | Empty enumeration elements; some required elements absent |
| Spain, DGT national access point | Situations, VMS, cameras and signs as device publications, energy infrastructure, controlled zones | Level C namespaces; national subtypes |
| France, DiaLog | Traffic regulations | Unprefixed type names |
| Belgium, Vlaams Verkeerscentrum | Situations | No `xsi:type` on the payload; `situation` in the payload's namespace |

## Written back

Of the 32 publications in those samples, the strict [writer](@/docs/writing.md) accepts 19.
Each of them reads back as an equal value, and `xmllint` accepts the 14 that are small enough to
validate in a test run. The other 13 are refused because they are not valid against the 3.7
schema: required content is missing (7), a value has a form the schema forbids (4), or a
required list is empty (2).

## Not verified

- Feeds that need a login or a contract: Norway, Sweden, the United Kingdom, Switzerland,
  Austria (live), most German offers.
- Countries for which no open version 3 feed was found.
- Sample files that national access points publish for Austria and Slovenia read, but were not
  part of the criteria above.

If a feed you use fails or loses data, please
[open an issue](https://github.com/hupe1980/datex2/issues) with a small sample.

## Known limits {#known-limits}

- **Version 3 XML only.** DATEX II version 2, which France (Bison Futé), Luxembourg,
  Switzerland and most German sources still publish, is not read. Version 1 neither. The JSON
  encoding of 3.7, used for much German charging-point data, is not read or written.
- **Older shapes of draft namespaces.** The types come from the 3.7 schemas. The parking,
  controlled-zone and traffic-regulation parts of the model were drafts before 3.6 and changed.
  Documents written against the older drafts use element names 3.7 no longer has; the reader
  reports each as an unknown element and reads the rest. The Dutch truck-parking status feed is
  one such case: it reads without error and yields no parking records. The Dutch emission-zone
  feed loses its zones for the same reason.
- **A whole read holds the document in memory**, about 1.7 times the XML size. Use
  [streaming](@/docs/streaming.md) for large files.
- **A stream delivers records, not the publication header.**
- **Exchange metadata is skipped.** A `messageContainer` is unwrapped; the Exchange 2020
  information in it is not interpreted and not written.
- **No field descriptions in the API reference.** Names are the standard's, so the official
  [data dictionary](https://docs.datex2.eu/) applies; each item names the XML element it stands
  for.
- **No geometry helpers.** Coordinates, Alert-C codes and OpenLR data are given as typed values;
  resolving them to map positions is left to other crates.
- **The writer does not check uniqueness constraints** of the schema.
