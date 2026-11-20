#![no_main]
//! Reads arbitrary bytes as DATEX II JSON; whatever either writer then accepts, the official 3.7
//! mapping or the Mobilithek profile, must read back as the same publication with nothing left
//! to tolerate.
//!
//! ```text
//! cargo +nightly fuzz run json corpus_json seeds_json -- -max_total_time=600 -rss_limit_mb=2048 -timeout=10
//! ```

use datex2::v3::json;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let doc = match json::read(data) {
        Ok(doc) => doc,
        Err(error) => {
            assert!(!error.to_string().chars().any(char::is_control));
            return;
        }
    };
    for w in &doc.warnings {
        assert!(!w.to_string().chars().any(char::is_control));
    }
    for publication in &doc.publications {
        let writers: [fn(&datex2::v3::Publication) -> Result<String, datex2::Error>; 2] =
            [json::to_string, json::mobilithek::to_string];
        for write in writers {
            match write(publication) {
                Err(error) => assert!(!error.to_string().chars().any(char::is_control)),
                Ok(text) => {
                    let again = json::read(text.as_bytes()).expect("what was written must read");
                    assert_eq!(again.publications.len(), 1);
                    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
                    // Not-a-number values are never equal to themselves; compare what is written.
                    assert_eq!(write(&again.publications[0]).expect("writes again"), text);
                }
            }
        }
    }
});
