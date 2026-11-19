+++
title = "Code generation"
description = "How the datex2 Rust types, readers and writers are generated from the official DATEX II 3.7 XSD schemas, how the schemas are fetched and pinned, and how to regenerate."
weight = 120

[extra]
seo_title = "Generating Rust types from the DATEX II schemas"
+++

The typed model, its readers, its writers and the stream routes are generated from the official
DATEX II 3.7 XML schemas. The generated code is committed; nobody edits it by hand.

## The schemas are not in the repository

The schemas are published at [docs.datex2.eu](https://docs.datex2.eu/). They are downloaded
when needed, and every file is checked against a SHA-256 pinned in
`xtask/schemas-3.7.sha256` before it is read. A file that does not match is refused.

```sh
cargo xtask fetch            # download the 19 schema files and verify them
cargo xtask codegen          # regenerate src/v3/generated/
cargo xtask codegen --check  # fail if the committed code differs
```

CI runs `fetch` and `codegen --check` on every change, so the committed model is always exactly
what the pinned schemas produce.

## The mapping

The DATEX II schemas use a small part of XML Schema: sequences, type extension, attributes and
enumerations; no choices, groups, unions or substitution groups. The mapping is therefore short:

| Schema | Rust |
|---|---|
| Concrete complex type | A struct with its inherited and own fields |
| Element or attribute occurring at most once | `Option<T>` |
| Repeating element | `Vec<T>` |
| A type with subtypes, used as a field type | An enum chosen by `xsi:type`, with accessors for shared fields |
| A subtype that only fixes an attribute | Read as its base type |
| Enumeration | An enum with `Extended(String)` |
| Other simple type | An alias of a primitive |
| Extension element | `Extension`, raw content |
| Facets and fixed values | Checks and constants in the writer |

The generator stops with an error on anything outside this mapping, such as a schema construct
or a pattern facet it has no rule for. A schema update cannot silently produce a wrong model.

## Only names and structure

The generated code contains the names and the structure of the model. It does not contain the
definition texts of the standard; the API reference links each item to its XML name, and the
official data dictionary explains what it means.

## Why not an existing XSD tool

The schemas are large (about 1,150 complex types), and polymorphism by `xsi:type` across
19 namespaces is exactly where general-purpose XML data binding in Rust is weakest. A generator
written for this one schema dialect is about 1,500 lines, and its output builds in seconds
because it is plain code without generics or derive-heavy types.
