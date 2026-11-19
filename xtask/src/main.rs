//! Maintainer tasks: `cargo xtask fetch` and `cargo xtask codegen [--check]`.
//!
//! The official schemas are not stored in the repository. `fetch` downloads them into
//! `target/datex2-schemas/3.7` (or `$DATEX2_SCHEMA_DIR`), and every file is checked against a
//! pinned SHA-256 before it is read.

mod emit;
mod schema;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use sha2::{Digest, Sha256};

const VERSION: &str = "3.7";
const BASE_NS: &str = "http://datex2.eu/schema/3/";
const SOURCE: &str = "https://docs.datex2.eu/_static/data/v3.7";
const PINS: &str = include_str!("../schemas-3.7.sha256");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repository")
        .to_owned()
}

fn schema_dir() -> PathBuf {
    std::env::var_os("DATEX2_SCHEMA_DIR")
        .map_or_else(|| root().join("target/datex2-schemas/3.7"), PathBuf::from)
}

/// File name and pinned digest, in the order of the pin file.
fn pins(text: &str) -> Result<Vec<(String, String)>, String> {
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (digest, name) = l
                .split_once("  ")
                .ok_or_else(|| format!("bad pin line: {l}"))?;
            if digest.len() != 64 || name.contains(['/', '\\']) || !name.ends_with(".xsd") {
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

/// Reads one schema file, refusing it unless its digest matches the pin.
fn read_pinned(dir: &Path, name: &str, digest: &str) -> Result<String, String> {
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
    String::from_utf8(bytes).map_err(|_| format!("{}: not UTF-8", path.display()))
}

fn fetch(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, digest) in pins(PINS)? {
        let path = dir.join(&name);
        if !path.exists() {
            let status = Command::new("curl")
                .args([
                    "--fail",
                    "--silent",
                    "--show-error",
                    "--location",
                    "--proto",
                    "=https",
                    "--output",
                ])
                .arg(&path)
                .arg(format!("{SOURCE}/{name}"))
                .status()
                .map_err(|e| format!("curl: {e}"))?;
            if !status.success() {
                return Err(format!("download of {name} failed"));
            }
        }
        if let Err(e) = read_pinned(dir, &name, &digest) {
            // A file that fails its pin must not stay where the generator would look for it.
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
        println!("ok  {name}");
    }
    Ok(())
}

fn generate(dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut schema = schema::Schema::default();
    for (name, digest) in pins(PINS)? {
        let text = read_pinned(dir, &name, &digest)?;
        schema::parse(&mut schema, &dir.join(&name), &text, BASE_NS)?;
    }
    emit::Emitter::new(&schema).emit(VERSION)
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
    let files = generate(&schema_dir())?;
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
        ["fetch"] => fetch(&schema_dir()),
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
        for line in [
            format!("{digest}  ../evil.xsd"),
            format!("{digest}  sub/x.xsd"),
            format!("{digest}  run.sh"),
            "short  a.xsd".to_owned(),
        ] {
            assert!(pins(&line).is_err(), "{line}");
        }
        assert_eq!(pins(PINS).unwrap().len(), 19);
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

    /// The committed model is what the pinned schemas produce. Skipped where the schemas are absent.
    #[test]
    fn sr008_committed_model_matches_the_schemas() {
        if !schema_dir().is_dir() {
            eprintln!(
                "skipped: no schemas at {} (run `cargo xtask fetch`)",
                schema_dir().display()
            );
            return;
        }
        let files = generate(&schema_dir()).unwrap();
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
