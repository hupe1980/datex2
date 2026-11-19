//! Writes what was read from the local corpus, when it is there, and lets an independent validator
//! judge the result.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;

use common::xmllint;
use datex2::v3;

const SOURCES: [&str; 5] = [
    "nl-ndw",
    "fi-digitraffic",
    "es-dgt-nap",
    "fr-dialog",
    "be-verkeerscentrum",
];

/// Validating a very large document with `xmllint` takes minutes and gigabytes.
const VALIDATE_UP_TO: usize = 20 << 20;

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

#[test]
fn what_was_read_is_written_valid_or_refused() {
    let Some(corpus) = common::corpus_dir() else {
        eprintln!("skipped: DATEX2_CORPUS_DIR is not set");
        return;
    };
    let (mut written, mut validated, mut refused) = (0, 0, BTreeMap::<String, usize>::new());
    let mut invalid = Vec::new();
    for source in SOURCES {
        let mut files: Vec<PathBuf> = std::fs::read_dir(corpus.join(source))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        files.sort();
        for path in files {
            let name = path.strip_prefix(&corpus).unwrap().display().to_string();
            if !(name.ends_with(".xml") || name.ends_with(".xml.gz")) || name.contains("TRUNCATED")
            {
                continue;
            }
            let bytes = load(&path);
            if !String::from_utf8_lossy(&bytes[..bytes.len().min(4000)]).contains("/schema/3/") {
                continue;
            }
            let doc = v3::read(&bytes[..]).unwrap_or_else(|e| panic!("{name}: {e}"));
            for publication in &doc.publications {
                match v3::to_string(publication) {
                    Err(e) => {
                        if *refused.entry(format!("{:?}", e.kind)).or_insert(0) == 0 {
                            eprintln!("first {:?}: {name}: {e}", e.kind);
                        }
                        *refused.entry(format!("{:?}", e.kind)).or_default() += 1;
                    }
                    Ok(xml) => {
                        written += 1;
                        // What was written reads back as the same value, without anything to tolerate.
                        let again = v3::read(xml.as_bytes()).unwrap_or_else(|e| {
                            panic!("{name}: written output does not read: {e}")
                        });
                        assert_eq!(again.publications.len(), 1, "{name}");
                        assert!(
                            again.publications[0] == *publication,
                            "{name}: read-write-read changed the publication"
                        );
                        assert_eq!(again.warnings, [], "{name}");
                        if xml.len() <= VALIDATE_UP_TO {
                            match xmllint(&xml) {
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
    eprintln!(
        "wrote {written} publications ({validated} validated by xmllint); refused: {refused:?}"
    );
    assert!(
        invalid.is_empty(),
        "written documents the official schema rejects: {invalid:#?}"
    );
    assert!(written + refused.values().sum::<usize>() >= 25);
}
