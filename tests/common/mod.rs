//! Helpers shared by the integration tests: small documents built around one accident record.
#![allow(dead_code)]

use datex2::v3::{self, Document, Publication, situation::SituationRecord};
use datex2::{Warning, WarningKind};

/// The conforming fixture; it validates against the official 3.7 schema.
pub const FIXTURE: &str = include_str!("../fixtures/situation.xml");

pub const NAMESPACES: &str = r#"xmlns:d2="http://datex2.eu/schema/3/d2Payload" xmlns:com="http://datex2.eu/schema/3/common" xmlns:sit="http://datex2.eu/schema/3/situation" xmlns:loc="http://datex2.eu/schema/3/locationReferencing" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance""#;

/// The children every situation record needs, with `extra` appended.
pub fn record_body(extra: &str) -> String {
    format!(
        "<sit:situationRecordCreationTime>2026-11-19T11:40:00Z</sit:situationRecordCreationTime>\
         <sit:situationRecordVersionTime>2026-11-19T11:58:00Z</sit:situationRecordVersionTime>\
         <sit:probabilityOfOccurrence>certain</sit:probabilityOfOccurrence>\
         <sit:validity><com:validityStatus>active</com:validityStatus>\
         <com:validityTimeSpecification><com:overallStartTime>2026-11-19T11:40:00Z</com:overallStartTime></com:validityTimeSpecification></sit:validity>\
         <sit:locationReference xsi:type=\"loc:PointLocation\"><loc:pointByCoordinates><loc:pointCoordinates>\
         <loc:latitude>52.0907</loc:latitude><loc:longitude>5.1214</loc:longitude></loc:pointCoordinates></loc:pointByCoordinates></sit:locationReference>\
         {extra}"
    )
}

/// An accident record with `extra` children.
pub fn accident(extra: &str) -> String {
    format!(
        r#"<sit:situationRecord xsi:type="sit:Accident" id="R1" version="1">{}</sit:situationRecord>"#,
        record_body(extra)
    )
}

/// The children of a situation publication around the given situation records.
pub fn publication_body(records: &str) -> String {
    format!(
        "<com:publicationTime>2026-11-19T12:00:00Z</com:publicationTime>\
         <com:publicationCreator><com:country>NL</com:country><com:nationalIdentifier>EXAMPLE</com:nationalIdentifier></com:publicationCreator>\
         <sit:situation id=\"S1\"><sit:situationVersionTime>2026-11-19T11:58:00Z</sit:situationVersionTime>\
         <sit:headerInformation><com:confidentiality>noRestriction</com:confidentiality><com:informationStatus>real</com:informationStatus></sit:headerInformation>\
         {records}</sit:situation>"
    )
}

/// A whole document: a `d2:payload` situation publication around the given records.
pub fn document(records: &str) -> String {
    format!(
        r#"<d2:payload {NAMESPACES} xsi:type="sit:SituationPublication" lang="en" modelBaseVersion="3">{}</d2:payload>"#,
        publication_body(records)
    )
}

pub fn read(xml: &str) -> Document {
    v3::read(xml.as_bytes()).unwrap_or_else(|e| panic!("{e}"))
}

pub fn kinds(doc: &Document) -> Vec<WarningKind> {
    doc.warnings.iter().map(|w| w.kind).collect()
}

pub fn only_warning(doc: &Document) -> &Warning {
    assert_eq!(doc.warnings.len(), 1, "{:#?}", doc.warnings);
    &doc.warnings[0]
}

/// The situation records of the first publication.
pub fn records(doc: &Document) -> Vec<&SituationRecord> {
    match doc.publications.first() {
        Some(Publication::Situation(p)) => p
            .situation
            .iter()
            .flat_map(|s| &s.situation_record)
            .collect(),
        other => panic!("expected a situation publication, got {other:?}"),
    }
}

pub fn first_accident(doc: &Document) -> &v3::situation::Accident {
    match records(doc).first() {
        Some(SituationRecord::Accident(a)) => a,
        other => panic!("expected an accident, got {other:?}"),
    }
}

/// Where `cargo xtask fetch` puts the official schemas.
pub fn schema_dir() -> std::path::PathBuf {
    std::env::var_os("DATEX2_SCHEMA_DIR").map_or_else(
        || std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/datex2-schemas/3.7"),
        std::path::PathBuf::from,
    )
}

/// A local folder of sample feeds, one sub-folder per source, named by `DATEX2_CORPUS_DIR`.
/// Real feeds are other people's data and are not part of the repository; tests that need them
/// say that they were skipped when the variable is not set.
pub fn corpus_dir() -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(std::env::var_os("DATEX2_CORPUS_DIR")?);
    dir.is_dir().then_some(dir)
}

/// Validates a document with `xmllint` against the official 3.7 schema: a validator this project
/// did not write. `None` when `xmllint` or the fetched schemas are not available.
pub fn xmllint(xml: &str) -> Option<Result<(), String>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let schema = schema_dir().join("DATEXII_3_D2Payload.xsd");
    if !schema.is_file() {
        return None;
    }
    let mut child = Command::new("xmllint")
        .args(["--noout", "--huge", "--schema"])
        .arg(schema)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let xml = xml.to_owned();
    let feeder = std::thread::spawn(move || stdin.write_all(xml.as_bytes()));
    let out = child.wait_with_output().ok()?;
    let _ = feeder.join();
    Some(if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr)
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" | "))
    })
}

/// Asserts that the official schema accepts the document; says so when it could not be checked.
pub fn assert_valid(xml: &str) {
    match xmllint(xml) {
        Some(Ok(())) => {}
        Some(Err(e)) => panic!("the official schema rejects the written document: {e}\n{xml}"),
        None => skipped("xmllint or the schemas are not available (run `cargo xtask fetch`)"),
    }
}

/// A JSON schema set the writer's output is checked against.
#[derive(Clone, Copy, Debug)]
pub enum JsonSchema {
    /// The official DATEX II 3.7 JSON schemas, entry `{"payload": …}`.
    Official,
    /// The Mobilithek AFIR profile, table publication: `{"payload": …}`.
    MobilithekTable,
    /// The Mobilithek AFIR profile, status publication: `{"messageContainer": …}`.
    MobilithekStatus,
}

impl JsonSchema {
    fn location(self) -> (std::path::PathBuf, &'static str) {
        let root = schema_dir()
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let profile = root.join("afir-mobilithek-01-00-00");
        match self {
            Self::Official => (root.join("3.7-json"), "DATEXII_3_D2Payload.json"),
            Self::MobilithekTable => (
                profile.join("EnergyInfrastructureTablePublication"),
                "DATEXII_3_D2Payload.json",
            ),
            Self::MobilithekStatus => (
                profile.join("EnergyInfrastructureStatusPublication"),
                "DATEXII_3_MessageContainer.json",
            ),
        }
    }
}

/// The command that runs Python with `jsonschema`: a plain `python3` that has it, or `uv`.
fn python() -> Option<&'static [&'static str]> {
    use std::process::{Command, Stdio};
    static FOUND: std::sync::OnceLock<Option<&'static [&'static str]>> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        const CANDIDATES: [&[&str]; 2] = [
            &["python3", "-I"],
            &[
                "uv",
                "run",
                "--quiet",
                "--with",
                "jsonschema",
                "python3",
                "-I",
            ],
        ];
        CANDIDATES.into_iter().find(|command| {
            Command::new(command[0])
                .args(&command[1..])
                .args(["-c", "import jsonschema, referencing"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        })
    })
}

/// Validates JSON with Python `jsonschema` against a schema set this project did not write.
/// `None` when Python with `jsonschema` or the fetched schemas are not available.
pub fn jsonschema(json: &str, schema: JsonSchema) -> Option<Result<(), String>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let (folder, entry) = schema.location();
    if !folder.join(entry).is_file() {
        return None;
    }
    let command = python()?;
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/validate_json.py");
    let mut child = Command::new(command[0])
        .args(&command[1..])
        .arg(script)
        .arg(folder)
        .arg(entry)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let json = json.to_owned();
    let feeder = std::thread::spawn(move || stdin.write_all(json.as_bytes()));
    let out = child.wait_with_output().ok()?;
    let _ = feeder.join();
    Some(if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    })
}

/// Asserts that the schema set accepts the JSON; says so when it could not be checked.
pub fn assert_json_valid(json: &str, schema: JsonSchema) {
    match jsonschema(json, schema) {
        Some(Ok(())) => {}
        Some(Err(e)) => {
            panic!("the {schema:?} JSON schema rejects the written document:\n{e}\n{json}")
        }
        None => skipped(
            "Python with jsonschema or the schemas are not available (run `cargo xtask fetch`)",
        ),
    }
}

/// A validation that could not run: a note locally, a failure where `DATEX2_REQUIRE_VALIDATORS`
/// is set, as in CI, so that a missing validator cannot pass for a successful one.
pub fn skipped(why: &str) {
    assert!(
        std::env::var_os("DATEX2_REQUIRE_VALIDATORS").is_none(),
        "validation required, but {why}"
    );
    eprintln!("skipped: {why}");
}

/// Asserts that `written` is the JSON document at `original` as the writer writes it back: equal
/// apart from `targetClass`, empty arrays and the spelling of equal numbers (see
/// `tests/support/same_json.py`).
pub fn assert_same_json(original: &std::path::Path, written: &str) {
    use std::process::Command;
    let file = std::env::temp_dir().join(format!(
        "datex2-same-json-{}-{:?}.json",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&file, written).unwrap();
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/same_json.py");
    let out = Command::new("python3")
        .arg("-I")
        .arg(script)
        .arg(original)
        .arg(&file)
        .output();
    let _ = std::fs::remove_file(&file);
    match out {
        Ok(out) if out.status.success() => {}
        Ok(out) => panic!(
            "{} is not written back as it was:\n{}{}",
            original.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(_) => skipped("python3 is not available"),
    }
}
