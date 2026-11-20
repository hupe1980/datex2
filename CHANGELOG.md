# Changelog

All notable changes to this crate are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate follows
[Semantic Versioning](https://semver.org/). While the version is 0.x, a minor release may break
the API; each break is named here with its migration.

## [Unreleased]

## [0.2.0] - 2026-11-20

### Added

- DATEX II version 3 **JSON**, read and written with the same types as XML (`v3::json`): the
  official 3.7 mapping, with `json::read`, `json::write`, `json::to_string` and
  `json::write_container`. When a write succeeds, the official 3.7 JSON schemas accept the output.
- The German **AFIR Recharging profile** that the Mobilithek requires (`v3::json::mobilithek`),
  revision 2 of profile 01-00-00: table publications for static data, status publications in a
  message container marked as snapshot or delta for dynamic data. The reader takes the profile's
  spelling without a setting; the writer produces it and refuses what the profile's schemas
  would reject, naming the member and the rule.
- Exact numbers in JSON: decimals keep their digits, `f32` values are written as the shortest
  text that reads back to the same value. No JSON dependency.
- Exchange 2020 types (`v3::exchange_information`, `v3::message_container` and two more), and
  `Document::exchange_information`, read from XML and JSON containers.
- Examples `mobilithek` (a charging site with an AC and a DC point, written for the
  Mobilithek) and `rewrite --json`.
- A `json` fuzz target, and threats T-13 to T-18 in the threat model.

### Changed

- **Breaking:** `Extension` has a new public field `json` for extension content read from JSON,
  and `Time` a new public field `fraction_digits`, so that a time is written back with the
  digits it was read with (`12:00:00.010` stays `12:00:00.010`). Code that builds an
  `Extension` with a struct literal adds `..Default::default()`; code that builds a `Time` adds
  `fraction_digits: 0`, which writes as many digits as the nanoseconds need. Parsing with
  `"08:00:00".parse::<Time>()` is unchanged.
- The exchange information of an XML `messageContainer` is now read instead of skipped, so a
  container with incomplete exchange information gives warnings about it.
- A refusal of a missing required member names the object that lacks it, not the member before.

## [0.1.0] - 2026-11-19

### Added

- Reading DATEX II version 3 XML into typed data with `v3::read`: all 19 payload namespaces of
  release 3.7, any document shape (`payload`, `messageContainer`, a root named after the
  publication, SOAP around them), tolerant of the deviations found in national feeds and
  reporting each as a `Warning`.
- Streaming records one at a time in bounded memory with `v3::stream::<T>` for 96 record types.
- Writing schema-valid DATEX II version 3 XML with `v3::write` and `v3::to_string`, with an error
  naming the element and the rule for anything the schema would reject.
- `Limits` on nesting depth, attributes, token length and warnings for untrusted input.
- Examples `read`, `stream`, `write`, `inspect` and `rewrite`.
- A threat model, published in the repository and in the crate documentation.

## [0.0.1]

Placeholder release that reserved the crate name. No functionality.
