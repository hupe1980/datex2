+++
title = "DATEX II in ten minutes"
description = "What DATEX II is, who publishes it, how a DATEX II v3 document is built (payload, publication, xsi:type, namespaces, extensions, profiles) and which EU rules require it."
weight = 20

[extra]
seo_title = "What is DATEX II? The standard in ten minutes"
+++

This page is the background you need to make sense of a DATEX II document and of the names in
this crate. It is a summary, not the standard.

## What it is

DATEX II is the European standard for exchanging road traffic and travel information between
organisations: road operators, traffic information services, national access points, and, more
recently, operators of charging and refuelling stations. It is published by CEN as EN 16157 and
CEN/TS 16157 and maintained by the [DATEX II organisation](https://datex2.eu/).

The standard defines a data model (in UML), and encodings of it. This crate handles the XML
encoding of version 3.

## Who has to use it

Several EU rules name DATEX II as the format for data that must be made available through
National Access Points:

| Rule | Data |
|---|---|
| Delegated Regulation (EU) 886/2013 | Safety-related traffic information |
| Delegated Regulation (EU) 885/2013 | Truck parking |
| Delegated Regulation (EU) 2022/670 | Real-time traffic information (DATEX II or TN-ITS) |
| Implementing Regulation (EU) 2025/655, under the Alternative Fuels Infrastructure Regulation (AFIR) | Charging and refuelling points; DATEX II from 14 April 2026 |

## Versions

| Version | State |
|---|---|
| 1.0 | From 2006. Still published by a few sources. |
| 2.0 to 2.3 | From 2011 to 2014. Still the only format of several national feeds. One XML namespace; root element `d2LogicalModel`. |
| 3.0 to 3.7 | From 2018. 3.7 is current. One XML namespace per part of the model; root element `payload`. |

Versions 2 and 3 are different models, not two dialects of one. **This crate handles version 3.**

A version 3 document does not say which 3.x release it follows; it only says
`modelBaseVersion="3"`. Parts of the model that are finished standards stay compatible across
3.x releases. Parts that were still drafts (parking, controlled zones, traffic regulations,
traffic management plans) changed between releases; see
[Tested feeds and limits](@/docs/feeds.md#known-limits).

## How a document is built

A minimal situation publication:

```xml
<d2:payload xmlns:d2="http://datex2.eu/schema/3/d2Payload"
            xmlns:com="http://datex2.eu/schema/3/common"
            xmlns:sit="http://datex2.eu/schema/3/situation"
            xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
            xsi:type="sit:SituationPublication" lang="en" modelBaseVersion="3">
  <com:publicationTime>2026-11-19T12:00:00Z</com:publicationTime>
  <com:publicationCreator>
    <com:country>NL</com:country>
    <com:nationalIdentifier>EXAMPLE</com:nationalIdentifier>
  </com:publicationCreator>
  <sit:situation id="S1">
    <sit:situationRecord xsi:type="sit:Accident" id="S1-R1" version="2">
      …
    </sit:situationRecord>
  </sit:situation>
</d2:payload>
```

The ideas in it recur everywhere:

- **Payload and publication.** The root is always `payload`. What kind of publication it holds
  is said by `xsi:type`. There are 30 publication types.
- **Polymorphism by `xsi:type`.** Wherever the model has an abstract class, the document names
  the concrete one in `xsi:type`: a `situationRecord` is an `Accident`, a
  `MaintenanceWorks`, an `AbnormalTraffic` and so on. In this crate these are Rust enums.
- **One namespace per part of the model**, each with a conventional prefix:
  `com` (common), `loc` (location referencing), `sit` (situation), `roa` (road traffic data),
  `vms`, `prk` (parking), `egi` (energy infrastructure), `fac` (facilities), and others. In this
  crate each is a module.
- **Identity.** Objects that others refer to carry `id` and often `version`. A reference is an
  element with the same `id` and `version` and a `targetClass` attribute.

### Publications you will meet

| Publication | Content |
|---|---|
| `SituationPublication` | Events and conditions on the road: accidents, roadworks, closures, weather, speed limits |
| `MeasuredDataPublication` with `MeasurementSiteTablePublication` | Counts, speeds and travel times, and the description of the sites that measure them |
| `ElaboratedDataPublication` | Derived traffic data such as travel times on routes |
| `VmsPublication`, `VmsTablePublication` | What variable message signs show, and where they are |
| `ParkingTablePublication`, `ParkingStatusPublication` | Parking sites and their occupancy |
| `EnergyInfrastructureTablePublication`, `EnergyInfrastructureStatusPublication` | Charging and refuelling sites, and their availability |
| `TrafficRegulationPublication`, `ControlledZoneTablePublication` | Machine-readable traffic regulations; low-emission and access zones |

### Envelopes

Exchange between systems is specified separately ("Exchange 2020"). A payload may arrive
wrapped in a `messageContainer`, which can hold several payloads plus exchange metadata, and
that may arrive in a SOAP envelope. Most open feeds simply serve a file by HTTP. The reader
unwraps all of these; it does not interpret the exchange metadata.

### Locations

Where something is can be said in several ways, often several at once: coordinates, a line
string, an Alert-C (TMC) location code, a TPEG-Loc description, an OpenLR reference. The crate
gives you these as typed values. It does not resolve codes to coordinates or match them to a map.

## Extensions and profiles

DATEX II is built to be adapted:

- **Extended enumeration values.** Every enumeration has a literal `_extended`; the actual value
  is then in the attribute `_extendedValue`. In this crate: the `Extended(String)` variant.
- **Extension slots.** Every class ends with an element named `_…Extension` that may hold
  elements from other namespaces. National feeds use them widely. The crate keeps their content
  as raw XML ([`Extension`](https://docs.rs/datex2/latest/datex2/struct.Extension.html)).
- **Levels.** "Level A" is the standard model. "Level B" adds content through extension slots
  and stays readable by a Level A reader. "Level C" changes the model and uses its own
  namespaces (`http://levelC/schema/3/…`); some national feeds are published this way.
- **Profiles.** A profile is a subset of the model chosen for a purpose, for example the
  recommended profiles for safety-related traffic information. A reader of the full model reads
  every profile.

## Why reading it is hard

The model is large (in 3.7 about 670 classes and 420 enumerations with 3,800 values), almost everything
is optional, polymorphism is everywhere, and real feeds deviate from the schema in many small
ways. That is the work this crate takes off your hands; [Tolerance and warnings](@/docs/tolerance.md)
lists the deviations it knows.

## Official sources

- The standard's site: [datex2.eu](https://datex2.eu/)
- Documentation, the data dictionary and the schemas: [docs.datex2.eu](https://docs.datex2.eu/)
