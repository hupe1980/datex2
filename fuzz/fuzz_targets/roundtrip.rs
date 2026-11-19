#![no_main]
//! Reads arbitrary bytes; whatever the strict writer then accepts must read back as the same
//! publication, without anything left to tolerate.
//!
//! ```text
//! cargo +nightly fuzz run roundtrip corpus seeds -- -max_total_time=600 -rss_limit_mb=2048 -timeout=10
//! ```

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(doc) = datex2::v3::read(data) else { return };
    for publication in &doc.publications {
        match datex2::v3::to_string(publication) {
            Err(error) => assert!(!error.to_string().chars().any(char::is_control)),
            Ok(xml) => {
                let again = datex2::v3::read(xml.as_bytes()).expect("what was written must read");
                assert_eq!(again.publications.len(), 1);
                // Not-a-number values are never equal to themselves; compare what is written instead.
                if again.publications[0] != *publication {
                    assert_eq!(datex2::v3::to_string(&again.publications[0]).expect("writes again"), xml);
                }
                assert!(again.warnings.is_empty(), "{:?}", again.warnings);
            }
        }
    }
});
