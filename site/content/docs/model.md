+++
title = "The typed model"
description = "How DATEX II v3 maps to Rust in the datex2 crate: one module per namespace, structs with optional fields, enums chosen by xsi:type, extended enumeration values and extension content."
weight = 60

[extra]
seo_title = "The DATEX II typed model in Rust"
+++

The Rust types are generated from the official DATEX II 3.7 schemas. Nothing is renamed or
reinterpreted, so the standard's [data dictionary](https://docs.datex2.eu/) describes them.

## Modules

One module per namespace of the standard, under `datex2::v3`:

| Module | Namespace prefix | Content |
|---|---|---|
| `common` | `com` | Types shared by all: validity, periods, vehicle characteristics, identifiers |
| `location_referencing` | `loc` | Point, linear and area locations; Alert-C, TPEG-Loc, OpenLR, coordinates |
| `situation` | `sit` | Situations and situation records |
| `road_traffic_data` | `roa` | Measured and elaborated data, measurement sites |
| `vms` | `vms` | Variable message signs |
| `parking` | `prk` | Parking tables and status |
| `energy_infrastructure`, `afir_energy_infrastructure` | `egi`, `aegi` | Charging and refuelling: the 2022 model and the AFIR model of 3.7 |
| `facilities`, `afir_facilities` | `fac`, `afac` | Facilities, operating hours, organisations, rates |
| `traffic_regulation`, `controlled_zone` | `tro`, `cz` | Traffic regulations; controlled zones |
| `traffic_management_plan`, `rerouting_management_enhanced` | `tmp`, `rer` | Traffic management plans, rerouting |
| `fault_and_status` | `fst` | Devices, faults, status |
| `common_extension`, `location_extension`, `urban_extensions` | `comx`, `locx`, `ubx` | Standard extensions |

## Names

| In the standard | In Rust |
|---|---|
| Class `SituationRecord` | `situation::SituationRecord` |
| Element `situationRecordCreationTime` | field `situation_record_creation_time` |
| Enumeration `AccidentTypeEnum`, literal `collisionWithAnimal` | `AccidentTypeEnum::CollisionWithAnimal` |
| Element `type` | field `r#type` |

Every item's documentation states the XML name it stands for, so searching the API reference
for an element name finds the field.

## Classes become structs

A class becomes a struct with public fields: its own and the ones it inherits, in the
standard's order.

| In the schema | Field type |
|---|---|
| An element or attribute that occurs at most once | `Option<T>` (`Option<Box<T>>` for a class) |
| An element that may repeat | `Vec<T>` |

**A field is `Option` also where the schema requires it.** Real feeds leave required content
out, so a field that is always there in theory is not in practice. When a required element is
absent, the reader leaves the field `None` and records a
[warning](@/docs/tolerance.md); the [writer](@/docs/writing.md) refuses to write it.

Every struct implements `Default`, `Clone`, `Debug` and `PartialEq`.

## Abstract classes become enums

Where a field's type has subtypes, the document says which one it holds with `xsi:type`, and
the field's Rust type is an enum:

- an **abstract** class becomes an enum of that name:
  `situation::SituationRecord::{Accident, MaintenanceWorks, AbnormalTraffic, …}`;
- a **concrete** class with subtypes keeps its struct, and fields use an enum named `Any…`:
  `location_referencing::PointLocation` and `AnyPointLocation::{PointLocation, PointLocationForParking}`.

Each enum has one accessor per shared field:

```rust
use datex2::v3::situation::SituationRecord;

fn created(record: &SituationRecord) -> Option<&datex2::DateTime> {
    record.situation_record_creation_time()
}
```

## Enumerations

An enumeration becomes an enum with one variant per literal, plus `Extended(String)`:

- the standard's own mechanism for extra values (`_extended` with an `_extendedValue`
  attribute) is read into `Extended`;
- a literal the model does not know, for example from a newer release, is read into `Extended`
  too, with a warning.

`as_str()` gives the value as it is written in a document.

## References

A reference to another object is a `VersionedReference { id, version }` or a
`Reference { id }`. The standard defines a separate reference class per target class
(63 of them) that differ only in a fixed `targetClass` attribute; the crate reads them all as
these two types, and the writer puts the right `targetClass` back.

## Extension content

Every class of the standard ends with an extension element. Its content is defined outside the
standard, so the crate keeps it as it is:

```rust
fn show(extension: &datex2::Extension) {
    println!("{}", extension.xml); // the XML inside the extension element
    for (prefix, uri) in &extension.namespaces {
        println!("uses {prefix} = {uri}"); // bindings from the surrounding document
    }
}
```

To interpret an extension, parse `xml` with an XML parser, declaring the listed bindings.

## Simple types

Measure types are aliases, which keeps the unit visible without wrapping the number:
`KilometresPerHour = f32`, `MetresAsFloat = f32`, `Percentage = f32`. The reader does not
enforce the schema's ranges or lengths on what it reads; the writer does.
