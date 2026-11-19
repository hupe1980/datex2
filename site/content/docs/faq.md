+++
title = "FAQ"
description = "Answers about the datex2 Rust crate: DATEX II versions supported, JSON, version 2, AFIR charging data, why fields are optional, validation, streaming, async and licensing."
weight = 130

[extra]
seo_title = "DATEX II Rust crate FAQ"

[[extra.faq]]
q = "Which DATEX II versions does datex2 support?"
a = "DATEX II version 3 in XML. The types are generated from release 3.7 and read documents of every 3.x release. Versions 1 and 2 and the JSON encoding are not supported yet."

[[extra.faq]]
q = "Can I read DATEX II version 2 feeds?"
a = "Not with this release. Version 2 is a different model with a different root element (d2LogicalModel). Reading it is planned. The reader recognises a version 2 document and says so in the error."

[[extra.faq]]
q = "Does it support the JSON encoding?"
a = "Not yet. The JSON mapping of DATEX II changed in 3.5, 3.6 and 3.7 and is still being standardised; support for the 3.7 form is planned."

[[extra.faq]]
q = "Can I use it to publish charging-point data under AFIR?"
a = "You can build and write EnergyInfrastructureTablePublication and EnergyInfrastructureStatusPublication documents, in the 2022 namespaces and in the AFIR namespaces of 3.7, as XML that the official schema accepts. Access points that require JSON are not served yet."

[[extra.faq]]
q = "Why is every field an Option, even required ones?"
a = "Because real feeds leave required content out: 25 of 57 sampled version 3 documents did. With Option, the reader can give you everything that is there and report what is missing, instead of failing or inventing a value. The writer still enforces what the schema requires."

[[extra.faq]]
q = "Does the reader validate a document against the schema?"
a = "No. It is tolerant by design and reports deviations as warnings. To check validity, write the publication back: the writer refuses whatever the schema would reject and names the rule."

[[extra.faq]]
q = "Is the output of the writer guaranteed to be valid?"
a = "When the write succeeds, the official 3.7 schema accepts the document, except for the schema's uniqueness constraints on identifiers, which are not checked. The test suite validates written documents with xmllint."

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
