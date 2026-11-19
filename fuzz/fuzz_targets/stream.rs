#![no_main]
//! Streams arbitrary bytes. A finding is a crash, a hang, a stream that does not end, or text that
//! is not safe to log.
//!
//! ```text
//! cargo +nightly fuzz run stream corpus seeds -- -max_total_time=600 -rss_limit_mb=2048 -timeout=10
//! ```

use datex2::v3::{self, situation::SituationRecord, Publication, Record};
use libfuzzer_sys::fuzz_target;

fn drive<T: Record>(data: &[u8]) -> usize {
    let mut stream = match v3::stream::<T>(data) {
        Ok(stream) => stream,
        Err(error) => {
            assert!(!error.to_string().chars().any(char::is_control));
            return 0;
        }
    };
    let (mut records, mut errors) = (0, 0);
    for item in stream.by_ref() {
        match item {
            Ok(_) => records += 1,
            Err(error) => {
                assert!(!error.to_string().chars().any(char::is_control));
                errors += 1;
            }
        }
        // Every step consumes input, so a document cannot hold more records than bytes.
        assert!(records <= data.len());
    }
    assert!(errors <= 1);
    assert!(stream.next().is_none());
    for warning in stream.take_warnings() {
        assert!(!warning.to_string().chars().any(char::is_control));
    }
    records
}

fuzz_target!(|data: &[u8]| {
    drive::<SituationRecord>(data);
    let publications = drive::<Publication>(data);
    // The whole read is the stream of publications, collected.
    if let Ok(doc) = v3::read(data) {
        assert_eq!(doc.publications.len(), publications);
    }
});
