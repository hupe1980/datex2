//! The threat-model gate: `THREAT_MODEL.md` keeps its shape, its identifiers stay stable, and
//! every test it names as evidence exists.

use std::collections::BTreeSet;
use std::path::Path;

const MODEL: &str = include_str!("../THREAT_MODEL.md");

/// Every threat id ever published. Append-only: an id is never removed, renumbered or reused; a
/// threat that no longer applies becomes `retired`.
const PUBLISHED: [&str; 12] = [
    "T-01", "T-02", "T-03", "T-04", "T-05", "T-06", "T-07", "T-08", "T-09", "T-10", "T-11", "T-12",
];

/// The `## ` headings the document must have, in this order.
const HEADINGS: [&str; 10] = [
    "Release status",
    "Adversary",
    "Assets",
    "Trusted",
    "In scope",
    "Out of scope",
    "Threats",
    "Limits of the controls",
    "Non-goals",
    "Reporting",
];

const STATUSES: [&str; 3] = ["mitigated", "partial", "retired"];

/// The rows of the threat table: id, threat, status, control, evidence.
fn rows() -> Vec<Vec<&'static str>> {
    MODEL
        .lines()
        .filter(|l| l.starts_with("| T-"))
        .map(|l| l.trim_matches('|').split(" | ").map(str::trim).collect())
        .collect()
}

#[test]
fn sections_are_present_and_in_order() {
    let found: Vec<&str> = MODEL
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .collect();
    assert_eq!(found, HEADINGS);
}

#[test]
fn identifiers_are_exactly_the_published_ones() {
    let ids: Vec<&str> = rows().iter().map(|r| r[0]).collect();
    assert_eq!(ids, PUBLISHED, "ids are append-only and listed in order");
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
}

#[test]
fn every_threat_has_a_status_a_control_and_evidence_that_exists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for row in rows() {
        let [id, threat, status, control, evidence] = row.as_slice() else {
            panic!("{row:?}: expected five columns")
        };
        assert!(STATUSES.contains(status), "{id}: status {status:?}");
        assert!(threat.len() > 20 && control.len() > 20, "{id}");
        let tests: Vec<&str> = evidence.split('`').filter(|t| t.contains("::")).collect();
        assert!(
            *status == "retired" || !tests.is_empty(),
            "{id}: a threat that is not retired needs evidence"
        );
        for test in tests {
            let (file, function) = test.split_once("::").unwrap();
            let source = std::fs::read_to_string(root.join(file))
                .unwrap_or_else(|e| panic!("{id}: {file}: {e}"));
            assert!(
                source.contains(&format!("fn {function}(")),
                "{id}: no test `{function}` in {file}"
            );
        }
    }
}

#[test]
fn the_readme_the_security_policy_and_the_crate_docs_link_to_it() {
    assert!(include_str!("../README.md").contains("(THREAT_MODEL.md)"));
    assert!(include_str!("../SECURITY.md").contains("(THREAT_MODEL.md)"));
    assert!(include_str!("../src/lib.rs").contains("[threat model](threat_model)"));
    assert!(
        include_str!("../src/threat_model.rs").contains("include_str!(\"../THREAT_MODEL.md\")")
    );
}
