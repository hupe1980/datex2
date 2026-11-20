#![no_main]
//! Reads arbitrary bytes as DATEX II JSON; whatever either writer then accepts, the official 3.7
//! mapping or the Mobilithek profile, must read back as the same publication with nothing left
//! to tolerate. A message container is also written as XML and must read back the same way.
//!
//! ```text
//! cargo +nightly fuzz run json corpus_json seeds_json -- -max_total_time=600 -rss_limit_mb=2048 -timeout=10
//! ```

use datex2::v3::message_container::MessageContainer;
use datex2::v3::{Root, json};
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
    let writers: [fn(Root<'_>) -> Result<String, datex2::Error>; 2] = [
        |root| json::to_string(root),
        |root| json::mobilithek::to_string(root),
    ];
    for publication in &doc.publications {
        for write in writers {
            match write(publication.into()) {
                Err(error) => assert!(!error.to_string().chars().any(char::is_control)),
                Ok(text) => {
                    let again = json::read(text.as_bytes()).expect("what was written must read");
                    assert_eq!(again.publications.len(), 1);
                    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
                    // Not-a-number values are never equal to themselves; compare what is written.
                    assert_eq!(write((&again.publications[0]).into()).expect("writes again"), text);
                }
            }
        }
    }
    // A container: the same writers, and XML, must keep its publications and exchange information.
    if doc.exchange_information.is_some() {
        let container = MessageContainer {
            payload: doc.publications.clone(),
            exchange_information: doc.exchange_information.clone(),
            ..Default::default()
        };
        let xml = |root: Root<'_>| datex2::v3::to_string(root);
        for write in [writers[0], writers[1], xml] {
            match write((&container).into()) {
                Err(error) => assert!(!error.to_string().chars().any(char::is_control)),
                Ok(text) => {
                    let again = if text.starts_with('<') {
                        datex2::v3::read(text.as_bytes())
                    } else {
                        json::read(text.as_bytes())
                    }
                    .expect("what was written must read");
                    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
                    let rewritten = MessageContainer {
                        payload: again.publications,
                        exchange_information: again.exchange_information,
                        ..Default::default()
                    };
                    assert_eq!(write((&rewritten).into()).expect("writes again"), text);
                }
            }
        }
    }
});
