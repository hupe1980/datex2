# Changelog

All notable changes to this crate are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate follows
[Semantic Versioning](https://semver.org/). While the version is 0.x, a minor release may break
the API; each break is named here with its migration.

## [Unreleased]

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
