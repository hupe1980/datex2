//! Maintainer tasks: `cargo xtask fetch` and `cargo xtask codegen [--check]`.
//!
//! Nothing this generator reads is stored in the repository. `fetch` downloads four pinned sets
//! into `target/datex2-schemas` (or `$DATEX2_SCHEMA_DIR`): the DATEX II 3.7 XML and JSON schemas,
//! the Exchange 2020 schemas, and the German AFIR Recharging profile. Every file is checked
//! against its SHA-256 in `xtask/pins/` before it is read.

mod emit;
mod schema;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use sha2::{Digest, Sha256};

const VERSION: &str = "3.7";
const BASE_NS: &str = "http://datex2.eu/schema/3/";

/// A group of files fetched from one place and pinned in one file.
struct PinSet {
    /// Folder under the schema root.
    dir: &'static str,
    /// The pin file: one `<sha256>  <name>` line per file.
    pins: &'static str,
    /// Where a name is downloaded from; `None` for files taken out of an archive of the set.
    url: fn(&str) -> Option<String>,
}

/// The DATEX II 3.7 XML schemas: the source of the typed model.
const XSD: PinSet = PinSet {
    dir: "3.7",
    pins: include_str!("../pins/datex2-3.7-xsd.sha256"),
    url: |name| Some(format!("https://docs.datex2.eu/_static/data/v3.7/{name}")),
};

/// The DATEX II 3.7 JSON schemas: every JSON name the generator derives is checked against them.
const JSON: PinSet = PinSet {
    dir: "3.7-json",
    pins: include_str!("../pins/datex2-3.7-json.sha256"),
    url: |name| Some(format!("https://docs.datex2.eu/_static/data/v3.7/{name}")),
};

/// Exchange 2020: the message container and its exchange information, in one archive.
const EXCHANGE: PinSet = PinSet {
    dir: "exchange-2020.1",
    pins: include_str!("../pins/exchange-2020.1.sha256"),
    url: |name| {
        (name == EXCHANGE_ARCHIVE)
            .then(|| format!("https://docs.datex2.eu/_static/data/exchange/2020.1/{name}"))
    },
};
const EXCHANGE_ARCHIVE: &str = "cisxsd.zip";

/// The German AFIR Recharging profile 01-00-00 rev. 2, at the commit that published it.
const MOBILITHEK: PinSet = PinSet {
    dir: "afir-mobilithek-01-00-00",
    pins: include_str!("../pins/afir-mobilithek-01-00-00.sha256"),
    url: |name| {
        Some(format!(
            "https://raw.githubusercontent.com/MobilithekDE/AFIR-DATEX-II-Recharging-Profil/\
             7ab184bcb51f2ba1ed0b8e4e6e4997a490ab0275/Releases/Version%2001-00-00/\
             Schema%20und%20Beispiele/{name}"
        ))
    },
};

const SETS: [&PinSet; 4] = [&XSD, &JSON, &EXCHANGE, &MOBILITHEK];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repository")
        .to_owned()
}

/// Where the fetched sources live: `target/datex2-schemas`, or `$DATEX2_SCHEMA_DIR`.
fn schema_root() -> PathBuf {
    std::env::var_os("DATEX2_SCHEMA_DIR")
        .map_or_else(|| root().join("target/datex2-schemas"), PathBuf::from)
}

/// File name and pinned digest, in the order of the pin file. A name may have one folder level
/// and must end in `.xsd`, `.json` or `.zip`; nothing can point outside the set's folder.
fn pins(text: &str) -> Result<Vec<(String, String)>, String> {
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (digest, name) = l
                .split_once("  ")
                .ok_or_else(|| format!("bad pin line: {l}"))?;
            let safe = !name.starts_with('/')
                && !name.contains('\\')
                && name.split('/').count() <= 2
                && name
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != "..")
                && [".xsd", ".json", ".zip"]
                    .iter()
                    .any(|ext| name.ends_with(ext));
            if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) || !safe {
                return Err(format!("bad pin line: {l}"));
            }
            Ok((name.to_owned(), digest.to_owned()))
        })
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Reads one pinned file, refusing it unless its digest matches the pin.
fn read_bytes_pinned(dir: &Path, name: &str, digest: &str) -> Result<Vec<u8>, String> {
    let path = dir.join(name);
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("{}: {e} (run `cargo xtask fetch`)", path.display()))?;
    let actual = sha256(&bytes);
    if actual != digest {
        return Err(format!(
            "{}: checksum mismatch, expected {digest}, found {actual}",
            path.display()
        ));
    }
    Ok(bytes)
}

fn read_pinned(dir: &Path, name: &str, digest: &str) -> Result<String, String> {
    let bytes = read_bytes_pinned(dir, name, digest)?;
    String::from_utf8(bytes).map_err(|_| format!("{}: not UTF-8", dir.join(name).display()))
}

/// Every text file of a set, verified, by name.
fn read_set(set: &PinSet) -> Result<Vec<(String, String)>, String> {
    let dir = schema_root().join(set.dir);
    pins(set.pins)?
        .into_iter()
        .filter(|(name, _)| !name.ends_with(".zip"))
        .map(|(name, digest)| Ok((name.clone(), read_pinned(&dir, &name, &digest)?)))
        .collect()
}

fn download(url: &str, path: &Path) -> Result<(), String> {
    let status = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
        ])
        .args(["--user-agent", "datex2-codegen", "--output"])
        .arg(path)
        .arg(url)
        .status()
        .map_err(|e| format!("curl: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("download of {url} failed"))
    }
}

fn fetch() -> Result<(), String> {
    for set in SETS {
        let dir = schema_root().join(set.dir);
        let pinned = pins(set.pins)?;
        // Archives first: their members are taken out of them, not downloaded.
        let mut ordered: Vec<&(String, String)> =
            pinned.iter().filter(|(n, _)| n.ends_with(".zip")).collect();
        ordered.extend(pinned.iter().filter(|(n, _)| !n.ends_with(".zip")));
        for (name, digest) in ordered {
            let path = dir.join(name);
            if read_bytes_pinned(&dir, name, digest).is_err() {
                let _ = std::fs::remove_file(&path);
                std::fs::create_dir_all(path.parent().expect("a file has a folder"))
                    .map_err(|e| format!("{}: {e}", dir.display()))?;
                match (set.url)(name) {
                    Some(url) => download(&url, &path)?,
                    None => {
                        let archive = pinned
                            .iter()
                            .find(|(n, _)| n.ends_with(".zip"))
                            .ok_or_else(|| format!("{name}: no archive in its set"))?;
                        let status = Command::new("unzip")
                            .args(["-o", "-j", "-q"])
                            .arg(dir.join(&archive.0))
                            .arg(name)
                            .arg("-d")
                            .arg(&dir)
                            .status()
                            .map_err(|e| format!("unzip: {e}"))?;
                        if !status.success() {
                            return Err(format!("{name}: not found in {}", archive.0));
                        }
                    }
                }
            }
            if let Err(e) = read_bytes_pinned(&dir, name, digest) {
                // A file that fails its pin must not stay where the generator would look for it.
                let _ = std::fs::remove_file(&path);
                return Err(e);
            }
            println!("ok  {}/{name}", set.dir);
        }
    }
    Ok(())
}

fn generate() -> Result<BTreeMap<String, String>, String> {
    let (schema, official, profile) = inputs()?;
    emit::Emitter::new(&schema).emit(VERSION, &official, &profile)
}

/// The XML schemas, the official JSON definitions and the profile's, each verified by its pins.
fn inputs() -> Result<(schema::Schema, schema::JsonDefs, schema::JsonDefs), String> {
    let mut schema = schema::Schema::default();
    let xsd_dir = schema_root().join(XSD.dir);
    let exchange_dir = schema_root().join(EXCHANGE.dir);
    for (set, dir) in [(&XSD, &xsd_dir), (&EXCHANGE, &exchange_dir)] {
        for (name, text) in read_set(set)? {
            schema::parse(&mut schema, &dir.join(&name), &text, BASE_NS)?;
        }
    }
    let official = schema::json_definitions(&read_set(&JSON)?)?;
    let profile = schema::json_definitions(
        &read_set(&MOBILITHEK)?
            .into_iter()
            .filter(|(name, _)| name.contains("/DATEXII_3_"))
            .collect::<Vec<_>>(),
    )?;
    Ok((schema, official, profile))
}

/// Compares generated files with the ones on disk; returns the names that differ.
fn differences(out: &Path, files: &BTreeMap<String, String>) -> Vec<String> {
    let mut diff: Vec<String> = files
        .iter()
        .filter(|(name, content)| {
            std::fs::read_to_string(out.join(name)).ok().as_ref() != Some(content)
        })
        .map(|(name, _)| name.clone())
        .collect();
    if let Ok(entries) = std::fs::read_dir(out) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !files.contains_key(&name) {
                diff.push(format!("{name} (not generated)"));
            }
        }
    }
    diff
}

fn codegen(check: bool) -> Result<(), String> {
    let files = generate()?;
    let out = root().join("src/v3/generated");
    if check {
        let diff = differences(&out, &files);
        if diff.is_empty() {
            return Ok(());
        }
        return Err(format!(
            "src/v3/generated differs from the generator's output: {}",
            diff.join(", ")
        ));
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    for stale in differences(&out, &files)
        .iter()
        .filter_map(|d| d.strip_suffix(" (not generated)"))
    {
        std::fs::remove_file(out.join(stale)).map_err(|e| e.to_string())?;
    }
    for (name, content) in &files {
        std::fs::write(out.join(name), content).map_err(|e| format!("{name}: {e}"))?;
    }
    println!(
        "wrote {} files, {} bytes",
        files.len(),
        files.values().map(String::len).sum::<usize>()
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["fetch"] => fetch(),
        ["codegen"] => codegen(false),
        ["codegen", "--check"] => codegen(true),
        _ => Err("usage: cargo xtask fetch | codegen [--check]".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("datex2-xtask-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// read-v3/SR-008: a schema that differs from its pin by one byte is refused by name.
    #[test]
    fn sr008_schema_that_fails_its_pin_is_refused() {
        let dir = temp("pin");
        let content = b"<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'/>";
        std::fs::write(dir.join("A.xsd"), content).unwrap();
        let digest = sha256(content);
        assert!(read_pinned(&dir, "A.xsd", &digest).is_ok());
        let mut changed = content.to_vec();
        changed[5] ^= 1;
        std::fs::write(dir.join("A.xsd"), changed).unwrap();
        let e = read_pinned(&dir, "A.xsd", &digest).unwrap_err();
        assert!(
            e.contains("A.xsd") && e.contains("checksum mismatch"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sr008_pin_file_rejects_paths_and_other_files() {
        let digest = "0".repeat(64);
        assert!(pins(&format!("{digest}  ok.xsd\n")).is_ok());
        assert!(pins(&format!("{digest}  folder/ok.json\n")).is_ok());
        for line in [
            format!("{digest}  ../evil.xsd"),
            format!("{digest}  run.sh"),
            "short  a.xsd".to_owned(),
        ] {
            assert!(pins(&line).is_err(), "{line}");
        }
        for line in [
            format!("{digest}  a/b/c.json"),
            format!("{digest}  ./x.json"),
            format!("{digest}  /etc/x.xsd"),
        ] {
            assert!(pins(&line).is_err(), "{line}");
        }
        let counts: Vec<usize> = SETS
            .iter()
            .map(|set| pins(set.pins).unwrap().len())
            .collect();
        assert_eq!(counts, [19, 19, 5, 16]);
    }

    /// read-v3/SR-008: identifiers come from a fixed rule, whatever the schema text is.
    #[test]
    fn sr008_identifiers_are_always_valid() {
        let valid = |s: &str| {
            !s.is_empty()
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !s.starts_with(|c: char| c.is_ascii_digit())
        };
        for text in [
            "collision",
            "3G",
            "a b-c",
            "x\"; fn evil() {} //",
            "",
            "/* */",
            "_extended",
            "ünïcödé",
        ] {
            assert!(
                valid(&emit::pascal(text)),
                "{text:?} -> {:?}",
                emit::pascal(text)
            );
        }
        assert_eq!(emit::pascal("x\"; fn evil() {} //"), "XFnEvil");
        assert_eq!(
            emit::snake("alertCLocationCountryCode"),
            "alert_c_location_country_code"
        );
        assert_eq!(
            emit::snake("_situationRecordExtension"),
            "situation_record_extension"
        );
        assert_eq!(emit::snake("openlrLAAngle"), "openlr_la_angle");
    }

    /// read-v3/SR-008: the check notices a hand edit, a missing file and a file that should not be there.
    #[test]
    fn sr008_check_notices_any_difference() {
        let dir = temp("check");
        let files = BTreeMap::from([
            ("a.rs".to_owned(), "pub struct A;\n".to_owned()),
            ("b.rs".to_owned(), "pub struct B;\n".to_owned()),
        ]);
        for (name, content) in &files {
            std::fs::write(dir.join(name), content).unwrap();
        }
        assert_eq!(differences(&dir, &files), Vec::<String>::new());
        std::fs::write(dir.join("a.rs"), "pub struct A; // edited\n").unwrap();
        assert_eq!(differences(&dir, &files), ["a.rs"]);
        std::fs::remove_file(dir.join("a.rs")).unwrap();
        std::fs::write(dir.join("c.rs"), "").unwrap();
        assert_eq!(differences(&dir, &files), ["a.rs", "c.rs (not generated)"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// json-v3/SR-006: a profile that differs from 3.7 in a way the generator's lists do not
    /// explain stops the generator, naming the difference. Skipped where the schemas are absent.
    #[test]
    fn sr006_unexplained_profile_difference_stops_the_generator() {
        if !schema_root().join(MOBILITHEK.dir).is_dir() {
            eprintln!("skipped: no profile schemas (run `cargo xtask fetch`)");
            return;
        }
        let (schema, official, profile) = inputs().unwrap();
        let site = (
            "afirEnergyInfrastructure".to_owned(),
            "EnergyInfrastructureSite".to_owned(),
        );
        let connector = (
            "afirEnergyInfrastructure".to_owned(),
            "ConnectorTypeEnum".to_owned(),
        );
        let mut member = profile.clone();
        member
            .get_mut(&site)
            .unwrap()
            .members
            .push("chargingPlace".to_owned());
        let mut literal = profile.clone();
        literal
            .get_mut(&connector)
            .unwrap()
            .literals
            .as_mut()
            .unwrap()
            .push("schuko2".to_owned());
        for (changed, name) in [(member, "chargingPlace"), (literal, "schuko2")] {
            let error = emit::Emitter::new(&schema)
                .emit(VERSION, &official, &changed)
                .unwrap_err();
            assert!(error.contains(name), "{error}");
        }
        assert!(
            emit::Emitter::new(&schema)
                .emit(VERSION, &official, &profile)
                .is_ok()
        );
    }

    /// The committed model is what the pinned schemas produce. Skipped where the schemas are absent.
    #[test]
    fn sr008_committed_model_matches_the_schemas() {
        if !schema_root().join(XSD.dir).is_dir() {
            eprintln!(
                "skipped: no schemas at {} (run `cargo xtask fetch`)",
                schema_root().display()
            );
            return;
        }
        let files = generate().unwrap();
        assert_eq!(
            differences(&root().join("src/v3/generated"), &files),
            Vec::<String>::new()
        );
        // Nothing but names and structure: no documentation text from the schemas.
        for (name, content) in &files {
            assert!(
                !content.contains("xs:documentation") && !content.contains("<xs:"),
                "{name}"
            );
        }
    }
}
