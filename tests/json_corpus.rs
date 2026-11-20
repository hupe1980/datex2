//! JSON against the local corpus, when it is there: real Mobilithek feeds, and every v3 XML
//! sample converted to 3.7 JSON. An independent validator judges what is written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;

use common::{JsonSchema, jsonschema};
use datex2::v3::message_container::MessageContainer;
use datex2::v3::{self, Publication, json};
use datex2::{ErrorKind, WarningKind};

/// Validating very large documents with Python takes long; larger ones are only read back.
const VALIDATE_UP_TO: usize = 4 << 20;

fn load(path: &Path) -> Vec<u8> {
    if path.extension().is_some_and(|e| e == "gz") {
        Command::new("gzip")
            .arg("-dc")
            .arg(path)
            .output()
            .expect("gzip is available")
            .stdout
    } else {
        std::fs::read(path).expect("readable sample")
    }
}

fn files(dir: &Path, suffixes: &[&str]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| suffixes.iter().any(|s| p.to_string_lossy().ends_with(s)))
        .collect();
    files.sort();
    files
}

/// Writes for the Mobilithek what a profile document held, as the same kind of document.
fn write_back(doc: &v3::Document) -> Result<(String, JsonSchema), datex2::Error> {
    if doc.envelope == v3::Envelope::MessageContainer {
        let container = MessageContainer {
            payload: doc.publications.clone(),
            exchange_information: doc.exchange_information.clone(),
            ..Default::default()
        };
        let mut bytes = Vec::new();
        json::mobilithek::write(&mut bytes, &container)?;
        Ok((
            String::from_utf8(bytes).unwrap(),
            JsonSchema::MobilithekStatus,
        ))
    } else {
        Ok((
            json::mobilithek::to_string(&doc.publications[0])?,
            JsonSchema::MobilithekTable,
        ))
    }
}

/// The publications with the profile's name and version where they named none, as the writer does.
fn named(publications: &[Publication]) -> Vec<Publication> {
    let mut publications = publications.to_vec();
    for publication in &mut publications {
        let (name, version) = match publication {
            Publication::AfirEnergyInfrastructureTable(p) => {
                (&mut p.profile_name, &mut p.profile_version)
            }
            Publication::AfirEnergyInfrastructureStatus(p) => {
                (&mut p.profile_name, &mut p.profile_version)
            }
            _ => continue,
        };
        if name.is_none() && version.is_none() {
            *name = Some(json::mobilithek::PROFILE_NAME.into());
            *version = Some(json::mobilithek::PROFILE_VERSION.into());
        }
    }
    publications
}

#[test]
fn mobilithek_feeds_read_and_write_back_for_the_profile() {
    let Some(corpus) = common::corpus_dir() else {
        eprintln!("skipped: DATEX2_CORPUS_DIR is not set");
        return;
    };
    let mut warnings = BTreeMap::<WarningKind, usize>::new();
    let mut refused = BTreeMap::<String, usize>::new();
    let (mut read, mut validated, mut invalid) = (0, 0, Vec::new());
    for path in files(&corpus.join("de-mobilithek"), &[".json", ".json.gz"]) {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = load(&path);
        if name.contains("TRUNCATED") {
            // Cut at 2 MB: everything up to the cut is read, then the end is reported, not a panic.
            let error = json::read(&bytes[..]).expect_err(&name);
            assert_eq!(error.kind, ErrorKind::Syntax, "{name}: {error}");
            continue;
        }
        let doc = json::read(&bytes[..]).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            !doc.publications.is_empty() || doc.exchange_information.is_some(),
            "{name}"
        );
        read += 1;
        for w in &doc.warnings {
            if !warnings.contains_key(&w.kind) {
                eprintln!("first {:?}: {name}: {w}", w.kind);
            }
            *warnings.entry(w.kind).or_default() += 1;
        }
        if doc.publications.is_empty() {
            continue;
        }
        match write_back(&doc) {
            Err(e) => {
                let reason = format!("{:?}: {}", e.kind, e.expected);
                if !refused.contains_key(&reason) {
                    eprintln!("refused: {name}: {e}");
                }
                *refused.entry(reason).or_default() += 1;
            }
            Ok((text, schema)) => {
                let again = json::read(text.as_bytes())
                    .unwrap_or_else(|e| panic!("{name}: written output does not read: {e}"));
                assert!(again.warnings.is_empty(), "{name}: {:#?}", again.warnings);
                assert!(
                    again.publications == named(&doc.publications),
                    "{name}: the written document reads back different"
                );
                if text.len() <= VALIDATE_UP_TO {
                    match jsonschema(&text, schema) {
                        Some(Ok(())) => validated += 1,
                        Some(Err(e)) => invalid.push(format!("{name}: {e}")),
                        None => {}
                    }
                }
            }
        }
    }
    eprintln!(
        "Mobilithek: {read} read, warnings {warnings:?}, {validated} written and validated, refused {refused:?}"
    );
    assert!(
        invalid.is_empty(),
        "the profile schema rejects written output:\n{}",
        invalid.join("\n")
    );
}

const XML_SOURCES: [&str; 5] = [
    "nl-ndw",
    "fi-digitraffic",
    "es-dgt-nap",
    "fr-dialog",
    "be-verkeerscentrum",
];

#[test]
fn xml_corpus_converts_to_valid_3_7_json_and_back() {
    let Some(corpus) = common::corpus_dir() else {
        eprintln!("skipped: DATEX2_CORPUS_DIR is not set");
        return;
    };
    let (mut written, mut validated, mut invalid) = (0, 0, Vec::new());
    let mut refused = BTreeMap::<String, usize>::new();
    for source in XML_SOURCES {
        for path in files(&corpus.join(source), &[".xml", ".xml.gz"]) {
            let name = path.strip_prefix(&corpus).unwrap().display().to_string();
            if name.contains("TRUNCATED") {
                continue;
            }
            let bytes = load(&path);
            if !String::from_utf8_lossy(&bytes[..bytes.len().min(4000)]).contains("/schema/3/") {
                continue;
            }
            let doc = v3::read(&bytes[..]).unwrap_or_else(|e| panic!("{name}: {e}"));
            for publication in &doc.publications {
                match json::to_string(publication) {
                    Err(e) => {
                        let reason = format!("{:?}: {}", e.kind, e.expected);
                        if !refused.contains_key(&reason) {
                            eprintln!("refused: {name}: {e}");
                        }
                        *refused.entry(reason).or_default() += 1;
                    }
                    Ok(text) => {
                        written += 1;
                        let again = json::read(text.as_bytes())
                            .unwrap_or_else(|e| panic!("{name}: written JSON does not read: {e}"));
                        assert!(
                            again.warnings.is_empty(),
                            "{name}: {:#?}",
                            &again.warnings[..again.warnings.len().min(3)]
                        );
                        assert!(
                            again.publications.len() == 1 && again.publications[0] == *publication,
                            "{name}: JSON reads back different"
                        );
                        if text.len() <= VALIDATE_UP_TO {
                            match jsonschema(&text, JsonSchema::Official) {
                                Some(Ok(())) => validated += 1,
                                Some(Err(e)) => invalid.push(format!("{name}: {e}")),
                                None => {}
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("XML → 3.7 JSON: {written} written, {validated} validated, refused {refused:?}");
    assert!(
        invalid.is_empty(),
        "the official JSON schema rejects written output:\n{}",
        invalid.join("\n")
    );
}
