# Threat model

This document says what `datex2` is meant to defend against, what it trusts and where it stops,
and lists every threat it knows with its control and the tests that are the evidence. The same
text is published in the repository and in the crate documentation.

## Release status

This describes `datex2` 0.1: reading, streaming and writing DATEX II version 3 XML.

The crate is a library. It reads bytes its caller gives it and writes bytes to a sink its caller
gives it. It opens no files and no sockets, starts no processes, holds no credentials and keeps no
state between calls. It contains no `unsafe` code and has one dependency, `quick-xml`.

## Adversary

| Who | Can do |
|---|---|
| **A feed publisher**, or anyone who can alter a feed in transit | Choose every byte of a document the caller reads |
| **A data source** of the caller: an operator's free-text field, another feed that was read | Choose values the caller places in a publication it writes |
| **The schema host**, or anyone between it and the maintainer | Serve an altered schema when the typed model is generated |

None of them can run code in the caller's process or choose the caller's limits.

## Assets

| Asset | What is at stake |
|---|---|
| **The caller's availability** | Memory and processing time while a document is read |
| **The data handed to the caller** | That it is what the document says: what road users are told, what a driver is charged |
| **The document the caller publishes** | That it holds what the caller set and nothing else, and that it is valid |
| **The generated model and the published crate** | That they are what the official schemas produce |

## Trusted

- The caller: its code, the limits it sets, the sink it writes to, and what it does with warnings.
- The Rust toolchain, the standard library and `quick-xml`.
- The pinned SHA-256 values of the schema files, which are committed to the repository.
- The maintainer's machine and the CI that build and release the crate.

## In scope

- Any input to `v3::read`, `v3::read_with`, `v3::stream` and `v3::stream_with`, whatever its
  size or content.
- Any value written with `v3::write` and `v3::to_string`, including text, identifiers, extended
  enumeration values and extension content that came from an untrusted source.
- The text of every `Error` and `Warning`, which callers log.
- The schema files the generator reads.

## Out of scope

- **Whether a feed is authentic.** The crate does not know where bytes came from. Use TLS and
  whatever authentication the publisher offers.
- **Whether the data is true.** A well-formed document that says a road is open is read as saying
  so.
- **Uniqueness of identifiers** across a written document; the schema's identity constraints are
  not checked.
- **Transport.** Downloading, decompressing and time-outs are the caller's. A decompression bomb
  must be bounded where the decompression happens; the crate bounds only what it parses.
- **Total document size.** A whole read holds about 1.7 times the XML size in memory. A caller
  that cannot afford that for the feeds it reads should stream, or bound the input.
- **Side channels** such as timing.

## Threats

The identifiers are stable: an entry is never renumbered or reused. A threat that no longer
applies is marked `retired`, not removed. `Evidence` names test functions; a gate in the test
suite fails when one of them does not exist.

| Id | Threat | Status | Control | Evidence |
|---|---|---|---|---|
| T-01 | A document with extreme nesting, attribute counts or text lengths makes the reader use memory or time far beyond its size | mitigated | Limits on depth, attributes per element, namespace declarations in scope and token length, applied while reading and also to content that is skipped or kept raw; the parser's buffer is bounded per token | `tests/limits.rs::sr001_depth_limit_names_itself_and_can_be_raised` `tests/limits.rs::sr001_attribute_limit_names_itself_and_can_be_raised` `tests/limits.rs::sr001_text_limit_names_itself_and_can_be_raised` `tests/limits.rs::sr001_oversized_token_is_refused_before_it_is_buffered` `tests/limits.rs::sr001_namespace_declarations_in_scope_are_bounded` |
| T-02 | A document type declaration with internal or external entities makes the reader expand them or fetch a resource | mitigated | Any document type declaration is refused before content is read; only predefined entities and character references are decoded | `tests/limits.rs::sr003_document_type_declaration_is_refused_before_content` `tests/limits.rs::sr003_undeclared_entity_is_refused_by_name` |
| T-03 | A document with millions of deviations makes the warning list grow without bound | mitigated | The list is capped; further warnings are counted | `tests/limits.rs::sr004_warnings_are_capped_and_the_rest_is_counted` |
| T-04 | Prefix rebinding, look-alike namespaces or a forged type attribute make the reader attribute content to the wrong type, or drop it without telling | mitigated | Names are matched by namespace URI and local name; the two tolerated exceptions are exact and always reported; a type attribute counts only in the XML Schema instance namespace and only for a subtype of the declared type; everything skipped is reported | `tests/deviations.rs::sr005_prefix_rebound_to_a_foreign_namespace_is_not_standard_content` `tests/deviations.rs::sr005_type_attribute_outside_the_schema_instance_namespace_selects_nothing` `tests/deviations.rs::sr005_type_that_is_not_derived_from_the_declared_type_is_not_used` `tests/deviations.rs::sr005_near_miss_level_c_and_foreign_namespaces_are_foreign` `tests/deviations.rs::sr006_everything_skipped_is_counted` |
| T-05 | Control characters, terminal escapes or very long text in a document reach an error or warning and forge or flood the caller's log | mitigated | Quoted input, element names in paths and the parser's own messages are shortened and have control characters escaped | `tests/errors.rs::sr007_quoted_input_is_shortened_and_has_no_control_characters` `tests/errors.rs::sr007_element_names_in_the_path_are_made_safe_too` `tests/errors.rs::sr007_parser_messages_are_made_safe_too` |
| T-06 | An altered schema makes the generator emit a wrong or malicious model that is then published | mitigated | Every schema file is checked against a committed SHA-256 before it is read; identifiers are derived by a fixed rule and no schema text is copied into code; CI fails when the committed model differs from the generator's output | `xtask/src/main.rs::sr008_schema_that_fails_its_pin_is_refused` `xtask/src/main.rs::sr008_pin_file_rejects_paths_and_other_files` `xtask/src/main.rs::sr008_identifiers_are_always_valid` `xtask/src/main.rs::sr008_check_notices_any_difference` |
| T-07 | A large document makes the memory of a stream grow with the document although the caller chose streaming to prevent that | mitigated | A stream holds one record, the bounded parser buffer and the capped warning list, which the caller can empty | `tests/stream_memory.rs::stream_sr001_peak_memory_does_not_depend_on_the_number_of_records` `tests/stream.rs::stream_sr001_limits_apply_between_and_inside_records` |
| T-08 | A broken document makes a stream report the same failure for ever, so the caller's loop never ends | mitigated | After the first error and after the end, a stream yields nothing; every step consumes input or ends | `tests/stream.rs::stream_sr002_truncated_document_gives_the_records_before_the_cut_then_one_error_then_nothing` `tests/stream.rs::stream_sr002_finished_stream_stays_finished` |
| T-09 | Elements named like records, placed elsewhere or in foreign namespaces, make a stream deliver what a whole read would not treat as a record, or miss real records silently | mitigated | Records are found only along the generated route for the declared publication type; a publication that cannot hold the record type is an error or a warning, never an empty success | `tests/stream.rs::stream_sr003_elements_named_like_records_elsewhere_are_not_records` `tests/stream.rs::stream_sr003_record_in_another_datex_namespace_is_delivered_and_reported` `tests/stream.rs::root_publication_that_cannot_hold_the_record_type_fails_at_once` |
| T-10 | Markup in a text, an identifier or an extended enumeration value adds elements or attributes to a written document, or breaks it | mitigated | Every text and attribute value passes through one escaping routine; characters XML 1.0 cannot carry are an error; element, attribute and type names are constants | `tests/write.rs::write_sr001_markup_in_values_is_escaped_and_reads_back_identical` `tests/write.rs::write_sr001_characters_xml_cannot_carry_are_refused` |
| T-11 | Extension content, which is written as raw XML, breaks out of its element or brings a document type declaration or entity to the consumers of the feed | mitigated | Extension content is parsed before it is copied and refused unless it is well-formed, balanced, elements only at its top level, and free of document type declarations and custom entities; namespace bindings are written as checked attribute text | `tests/write.rs::write_sr002_extension_content_that_is_not_safe_is_refused` `tests/write.rs::write_sr002_well_formed_extension_content_survives` |
| T-12 | Data that lacks required content or breaks schema rules is written as if it were valid | mitigated | The writer fails on absent required content, list bounds and value rules; an independent validator checks what the tests write | `tests/write.rs::write_sr003_each_rule_is_named_in_the_error` `tests/write.rs::list_with_more_entries_than_allowed_is_refused` `tests/write.rs::write_sr003_publications_built_in_code_are_accepted_by_the_official_schema` |

Beyond the tests, three fuzz targets (`read`, `stream`, `roundtrip`) assert that no input makes
the crate crash, hang or return unsafe text, and that whatever the writer accepts reads back as
the same value. Fuzzing found the two gaps that the last two tests of T-05 now cover.

## Limits of the controls

- **T-01 bounds a token, not a document.** A document of many small elements is read in full by
  `v3::read`. Its memory is proportional to its size.
- **T-04 tolerates on purpose.** A known element in another DATEX II namespace is accepted and
  reported. A caller that needs strictness should treat `WrongNamespace` and `LevelCNamespace`
  warnings as failures.
- **T-12 does not cover identity constraints.** Two objects with the same identifier are written.
- **Warnings are advice.** The controls of T-04 report; acting on the report is the caller's.

## Non-goals

- Authenticating or signing documents.
- Sandboxing the caller, or protecting against a caller that sets limits too high.
- Validating a document against the schema on reading. The reader is tolerant by design; the
  writer is the strict side.

## Reporting

Please report a suspected vulnerability privately; see `SECURITY.md` in the repository.
