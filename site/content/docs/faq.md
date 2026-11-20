+++
title = "FAQ"
description = "Answers about the datex2 Rust crate: DATEX II versions supported, JSON, version 2, AFIR charging data and the Mobilithek, why fields are optional, validation, streaming, async and licensing."
weight = 130

[extra]
seo_title = "DATEX II Rust crate FAQ"

[[extra.faq]]
q = "Which DATEX II versions does datex2 support?"
a = "DATEX II version 3, in XML and in JSON. The types are generated from release 3.7 and read documents of every 3.x release. For JSON, the 3.7 mapping and the German AFIR Recharging profile are supported. Versions 1 and 2 are not supported."

[[extra.faq]]
q = "Can I read DATEX II version 2 feeds?"
a = "Not with this release. Version 2 is a different model with a different root element (d2LogicalModel). The reader recognises a version 2 document and says so in the error."

[[extra.faq]]
q = "Does it support the JSON encoding?"
a = "Yes, the DATEX II 3.7 JSON mapping, read and written with the same types as XML. When the writer succeeds, the official 3.7 JSON schemas accept the output. The JSON of earlier 3.x releases differs in places and is read as far as it agrees with 3.7. Money keeps its exact digits, and the crate has no JSON dependency."

[[extra.faq]]
q = "Can I use it to publish charging-point data under AFIR?"
a = "Yes. Build EnergyInfrastructureTablePublication (static data) and EnergyInfrastructureStatusPublication (dynamic data) and write them as XML, as 3.7 JSON, or with json::mobilithek for the AFIR Recharging profile that Germany's Mobilithek requires: a table publication, and status publications in a message container marked as snapshot or delta. When the write succeeds, the profile's schemas accept the output. examples/mobilithek.rs is a complete site."

[[extra.faq]]
q = "Why is every field an Option, even required ones?"
a = "Because real feeds leave required content out: 25 of 57 sampled version 3 documents did. With Option, the reader can give you everything that is there and report what is missing, instead of failing or inventing a value. The writer still enforces what the schema requires."

[[extra.faq]]
q = "Does the reader validate a document against the schema?"
a = "No. It is tolerant by design and reports deviations as warnings. To check validity, write the publication back: the writer refuses whatever the schema would reject and names the rule."

[[extra.faq]]
q = "Is the output of the writer guaranteed to be valid?"
a = "When the write succeeds, the official 3.7 schemas accept the document (XML or JSON; for json::mobilithek, the profile's schemas), except for the uniqueness constraints on identifiers, which are not checked. The test suite validates written documents with xmllint and Python jsonschema."

[[extra.faq]]
q = "How do I read a gzip-compressed feed?"
a = "Wrap the file in a gzip decoder and a BufReader, for example flate2::read::GzDecoder. The crate reads any BufRead and does no decompression itself."

[[extra.faq]]
q = "Is there an async API?"
a = "No. Parsing is CPU work on bytes you already have. Download with the HTTP client of your choice and pass the bytes; in an async program, run the read on a blocking thread for large documents."

[[extra.faq]]
q = "Does it resolve Alert-C or OpenLR locations to coordinates?"
a = "No. Location references are given as typed values. Resolving them needs location tables or a map and is left to other crates."

[[extra.faq]]
q = "Is datex2 an official DATEX II implementation?"
a = "No. It is an independent open-source implementation, not affiliated with or endorsed by the DATEX II organisation."
+++

Short answers to what people ask first. Each links into the documentation where there is more to say: [versions and limits](@/docs/feeds.md#known-limits), [tolerance](@/docs/tolerance.md), [writing](@/docs/writing.md).
