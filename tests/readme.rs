//! The first example in the README is the file `examples/read.rs`, and it runs as written.

use std::process::Command;

mod common;

/// The `n`th block of the README that is fenced as `rust,no_run`.
fn runnable_block(readme: &str, n: usize) -> &str {
    let fence = "```rust,no_run\n";
    let start = readme.match_indices(fence).nth(n).expect("a rust block").0 + fence.len();
    &readme[start..start + readme[start..].find("```").expect("the end of the block")]
}

/// Runs an example from a directory that holds `file`, exactly as a user would.
fn run_example(example: &str, file: &str) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!("datex2-readme-{example}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), include_str!("fixtures/situation.xml")).unwrap();
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let out = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "--example",
            example,
            "--manifest-path",
            manifest,
        ])
        .current_dir(&dir)
        .output()
        .expect("cargo runs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

#[test]
fn readme_examples_are_the_example_files() {
    let readme = include_str!("../README.md");
    for (n, example) in [
        include_str!("../examples/read.rs"),
        include_str!("../examples/stream.rs"),
        include_str!("../examples/write.rs"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(runnable_block(readme, n), example);
        assert!(example.lines().filter(|l| !l.trim().is_empty()).count() <= 16);
    }
}

#[test]
fn readme_read_example_runs_as_written() {
    let out = run_example("read", "incidents.xml");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "1 situations\n");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "",
        "the fixture conforms, so there is nothing to warn about"
    );
}

#[test]
fn readme_stream_example_runs_as_written() {
    let out = run_example("stream", "roadworks.xml");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Some(\"S1\"): 1 records\n"
    );
}

#[test]
fn readme_write_example_runs_and_its_output_is_valid() {
    let out = run_example("write", "unused.xml");
    let xml = String::from_utf8(out.stdout).unwrap();
    assert!(xml.contains("<com:country>NL</com:country>"), "{xml}");
    common::assert_valid(&xml);
    assert_eq!(datex2::v3::read(xml.as_bytes()).unwrap().warnings, []);
}
