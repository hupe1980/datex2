#![no_main]
//! Feeds arbitrary bytes to the reader. A finding is a crash, a hang or runaway memory; an `Err`
//! is the expected answer to most inputs.
//!
//! ```text
//! cargo +nightly fuzz run read fuzz/seeds -- -max_total_time=3600 -rss_limit_mb=2048 -timeout=10
//! ```

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Every text the reader hands back is built from input and must be safe to log.
    match datex2::v3::read(data) {
        Ok(doc) => {
            for warning in &doc.warnings {
                assert!(!warning.to_string().chars().any(char::is_control));
            }
            assert!(doc.warnings.len() <= datex2::Limits::default().max_warnings);
        }
        Err(error) => assert!(!error.to_string().chars().any(char::is_control)),
    }
});
