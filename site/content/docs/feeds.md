+++
title = "Tested feeds and limits"
description = "The national DATEX II v3 feeds datex2 was tested on (Netherlands, Finland, Spain, France, Belgium, and AFIR charging data from Germany's Mobilithek), what was not verified, and the known limits of this release."
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

### JSON from the Mobilithek

Twelve AFIR recharging feeds from the [Mobilithek](https://mobilithek.info/), Germany's National
Access Point, static and dynamic, in the AFIR Recharging profile's JSON. All read without error.
Files cut off at 2 MB report the cut as a syntax error.

What the reader reports on them: a member only the profile has (`refillPointByReference`, 123
times in one feed), two location kinds in one wrapper where the model holds one (333 times in
one feed), and one feed's own schema faults.

## Written back

Of the 32 publications in those samples, the strict [writer](@/docs/writing.md) accepts 19.
Each of them reads back as an equal value, and `xmllint` accepts the 14 that are small enough to
validate in a test run. The other 13 are refused because they are not valid against the 3.7
schema: required content is missing (7), a value has a form the schema forbids (4), or a
required list is empty (2).

## Written as JSON

Of the same 32 publications, the 3.7 JSON writer accepts 15. The official 3.7 JSON schemas accept
the 11 small enough to validate in a test run, and each reads back as an equal value. Besides the
faults the XML writer refuses, JSON refuses language codes that are not two lowercase letters
(5) and extension content read from XML (6), which [stays in its
encoding](@/docs/json.md#extension-content-stays-in-its-encoding).

Of the twelve Mobilithek feeds, nine are written back for the profile; the profile's schemas
accept each, and each reads back as an equal value. Three are refused: two give a time zone as
a name (`Europe/Berlin`) where the profile describes an offset, one has a price with ten decimal
places.

## Not verified

- Feeds that need a login or a contract: Norway, Sweden, the United Kingdom, Switzerland,
  Austria (live), most German offers.
- Countries for which no open version 3 feed was found.
- Sample files that national access points publish for Austria and Slovenia read, but were not
  part of the criteria above.

If a feed you use fails or loses data, please
[open an issue](https://github.com/hupe1980/datex2/issues) with a small sample.

## Known limits {#known-limits}

- **Version 3 only.** DATEX II version 2, which France (Bison Futé), Luxembourg, Switzerland and
  most German sources still publish, is not read. Version 1 neither. Of JSON, the 3.7 mapping
  and the Mobilithek's AFIR profile are supported; the JSON of earlier 3.x releases differs and
  is read only as far as it agrees with 3.7.
- **Older shapes of draft namespaces.** The types come from the 3.7 schemas. The parking,
  controlled-zone and traffic-regulation parts of the model were drafts before 3.6 and changed.
  Documents written against the older drafts use element names 3.7 no longer has; the reader
  reports each as an unknown element and reads the rest. The Dutch truck-parking status feed is
  one such case: it reads without error and yields no parking records. The Dutch emission-zone
  feed loses its zones for the same reason.
- **A whole read holds the document in memory**, about 1.7 times the XML size. Use
  [streaming](@/docs/streaming.md) for large files.
- **A stream delivers records, not the publication header.**
- **Message containers are written only as JSON.** The exchange information of a container is
  read from XML and JSON; the XML writer writes single publications.
- **No field descriptions in the API reference.** Names are the standard's, so the official
  [data dictionary](https://docs.datex2.eu/) applies; each item names the XML element it stands
  for.
- **No geometry helpers.** Coordinates, Alert-C codes and OpenLR data are given as typed values;
  resolving them to map positions is left to other crates.
- **The writer does not check uniqueness constraints** of the schema.
