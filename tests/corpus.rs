//! Reads a local corpus of real national feeds, when `DATEX2_CORPUS_DIR` names one.
//!
//! The feeds are other people's data and are not part of the repository. Without the variable
//! (CI, a fresh clone) these tests print that they were skipped and pass.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use datex2::{ErrorKind, WarningKind, v3};

/// The feeds the first release is checked against.
const SOURCES: [&str; 5] = [
    "nl-ndw",
    "fi-digitraffic",
    "es-dgt-nap",
    "fr-dialog",
    "be-verkeerscentrum",
];

/// Elements that feeds written against an older release of a draft namespace still send, and that
/// the 3.7 schemas no longer define at that place. Anything else unknown in a standard namespace
/// would mean the generated model is missing something, and fails the test.
const FROM_OLDER_RELEASES: [&str; 5] = [
    "\"parkingRecordStatus\" in namespace parking",
    "\"urbanVehicleAccessRegulation\" in namespace controlledZone",
    "\"controlledZoneDescription\" in namespace controlledZone",
    "\"controlledZoneRecordVersionTime\" in namespace controlledZone",
    // Defined in 3.7, but no longer on the controlled zone itself.
    "\"urlForFurtherInformation\" in namespace controlledZone",
];

fn load(path: &Path) -> Vec<u8> {
    if path.extension().is_some_and(|e| e == "gz") {
        // Truncated samples end mid-stream, so gzip reports an error after writing what it has.
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

fn samples(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.to_string_lossy().ends_with(".xml") || p.to_string_lossy().ends_with(".xml.gz")
        })
        .collect();
    files.sort();
    files
}

#[test]
fn every_v3_sample_of_the_five_feeds_reads() {
    let Some(corpus) = common::corpus_dir() else {
        eprintln!("skipped: DATEX2_CORPUS_DIR is not set");
        return;
    };
    let mut read = 0;
    let mut unexpected = std::collections::BTreeSet::new();
    for source in SOURCES {
        for path in samples(&corpus.join(source)) {
            let bytes = load(&path);
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4000)]).into_owned();
            if !head.contains("/schema/3/") {
                continue; // version 1 or 2, or not DATEX II
            }
            let name = path.strip_prefix(&corpus).unwrap().display().to_string();
            let doc = match v3::read(&bytes[..]) {
                Ok(doc) => doc,
                Err(e) if name.contains("TRUNCATED") => {
                    // The sample was cut at 2 MB when it was saved; everything before the cut must read.
                    assert_eq!(e.kind, ErrorKind::Syntax, "{name}: {e}");
                    assert!(
                        e.offset + 65_536 > bytes.len() as u64,
                        "{name}: failed before the cut: {e}"
                    );
                    read += 1;
                    continue;
                }
                Err(e) => panic!("{name}: {e}"),
            };
            assert!(!doc.publications.is_empty(), "{name}: no publication");
            for w in doc
                .warnings
                .iter()
                .filter(|w| w.kind == WarningKind::UnknownElement)
            {
                let standard_namespace =
                    !w.detail.ends_with('"') && w.detail.contains(" in namespace ");
                if standard_namespace && !FROM_OLDER_RELEASES.contains(&w.detail.as_str()) {
                    unexpected.insert(format!("{name}: {}", w.detail));
                }
            }
            read += 1;
        }
    }
    assert!(
        unexpected.is_empty(),
        "unknown elements in standard namespaces: {unexpected:#?}"
    );
    assert!(
        read >= 25,
        "only {read} version 3 samples found; is the corpus complete?"
    );
    eprintln!("read {read} version 3 samples");
}

/// Streams `T` from the sample and returns the records; `None` when the publication holds no `T`.
fn streamed<T: v3::Record>(bytes: &[u8]) -> Option<Vec<T>> {
    match v3::stream::<T>(bytes) {
        Ok(stream) => Some(
            stream
                .map(|r| r.expect("the whole read succeeded, so must the stream"))
                .collect(),
        ),
        Err(e) if e.kind == ErrorKind::UnexpectedPublication => None,
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn streamed_records_equal_the_whole_read() {
    use v3::Publication as P;
    let Some(corpus) = common::corpus_dir() else {
        eprintln!("skipped: DATEX2_CORPUS_DIR is not set");
        return;
    };
    let (mut files, mut records) = (0, 0);
    for source in SOURCES {
        for path in samples(&corpus.join(source)) {
            let bytes = load(&path);
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4000)]).into_owned();
            if !head.contains("/schema/3/") || path.to_string_lossy().contains("TRUNCATED") {
                continue;
            }
            let name = path.strip_prefix(&corpus).unwrap().display().to_string();
            let whole = v3::read(&bytes[..]).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                streamed::<P>(&bytes).as_ref(),
                Some(&whole.publications),
                "{name}: publications"
            );
            // Records of the same type from every publication of the document, in order.
            let (mut situations, mut situation_records, mut measurements, mut sites, mut vms) =
                (vec![], vec![], vec![], vec![], vec![]);
            for publication in &whole.publications {
                match publication {
                    P::Situation(p) => {
                        situations.extend(p.situation.iter().cloned());
                        situation_records.extend(
                            p.situation
                                .iter()
                                .flat_map(|s| s.situation_record.iter().cloned()),
                        );
                    }
                    P::MeasuredData(p) => measurements.extend(p.site_measurements.iter().cloned()),
                    P::MeasurementSiteTable(p) => sites.extend(
                        p.measurement_site_table
                            .iter()
                            .flat_map(|t| t.measurement_site.iter().cloned()),
                    ),
                    P::Vms(p) => vms.extend(p.vms_controller_status.iter().cloned()),
                    _ => {}
                }
            }
            records += situations.len()
                + situation_records.len()
                + measurements.len()
                + sites.len()
                + vms.len();
            fn same<T: v3::Record + PartialEq + std::fmt::Debug>(
                bytes: &[u8],
                whole: Vec<T>,
                what: &str,
            ) {
                assert_eq!(streamed::<T>(bytes).unwrap_or_default(), whole, "{what}");
            }
            same(&bytes, situations, &name);
            same(&bytes, situation_records, &name);
            same(&bytes, measurements, &name);
            same(&bytes, sites, &name);
            same(&bytes, vms, &name);
            files += 1;
        }
    }
    assert!(
        files >= 25 && records > 100_000,
        "{files} files, {records} records"
    );
    eprintln!("compared {records} streamed records in {files} files");
}
