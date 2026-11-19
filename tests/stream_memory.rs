//! stream-records/SR-001: memory held while streaming does not grow with the number of records.
//!
//! The test binary counts live heap bytes with its own allocator and reads documents that are
//! produced on the fly, so nothing but the stream can hold on to them.

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicUsize, Ordering};

use datex2::v3::{self, situation::Situation};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call is forwarded unchanged to the system allocator; the counters are only read.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK.fetch_max(live, Ordering::Relaxed);
        // SAFETY: the caller upholds the contract of `GlobalAlloc::alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: `ptr` came from `System.alloc` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const HEAD: &str = r#"<d2:payload xmlns:d2="http://datex2.eu/schema/3/d2Payload" xmlns:com="http://datex2.eu/schema/3/common" xmlns:sit="http://datex2.eu/schema/3/situation" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="sit:SituationPublication" lang="en"><com:publicationTime>2026-11-19T12:00:00Z</com:publicationTime>"#;
const RECORD: &str = r#"<sit:situation id="S"><sit:situationVersionTime>2026-11-19T11:58:00Z</sit:situationVersionTime><sit:unknownHere>text that is passed over</sit:unknownHere><sit:situationRecord xsi:type="sit:Accident" id="R" version="1"><sit:situationRecordCreationTime>2026-11-19T11:40:00Z</sit:situationRecordCreationTime><sit:accidentType>collision</sit:accidentType></sit:situationRecord></sit:situation><sit:betweenRecords>also passed over</sit:betweenRecords>"#;
const TAIL: &str = "</d2:payload>";

/// Produces a publication with `records` situations without ever holding it.
struct Generated {
    records: usize,
    sent: usize,
    current: &'static [u8],
}

impl Read for Generated {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.current.is_empty() {
            self.current = match self.sent {
                0 => HEAD.as_bytes(),
                n if n <= self.records => RECORD.as_bytes(),
                n if n == self.records + 1 => TAIL.as_bytes(),
                _ => return Ok(0),
            };
            self.sent += 1;
        }
        let n = self.current.len().min(out.len());
        out[..n].copy_from_slice(&self.current[..n]);
        self.current = &self.current[n..];
        Ok(n)
    }
}

/// Streams `records` situations, taking the warnings after each, and returns the peak of live heap
/// bytes above the level before the stream existed.
fn peak_while_streaming(records: usize) -> usize {
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let source = BufReader::new(Generated {
        records,
        sent: 0,
        current: b"",
    });
    let mut stream = v3::stream::<Situation>(source).unwrap();
    let mut count = 0;
    while let Some(situation) = stream.next() {
        let situation = situation.unwrap();
        assert_eq!(situation.situation_record.len(), 1);
        // Each record produces a handful of warnings; taking them keeps the list short.
        let warnings = stream.take_warnings().len();
        assert!((1..20).contains(&warnings), "{warnings}");
        count += 1;
    }
    assert_eq!(count, records);
    assert_eq!(stream.suppressed_warnings(), 0);
    PEAK.load(Ordering::Relaxed) - before
}

#[test]
fn stream_sr001_peak_memory_does_not_depend_on_the_number_of_records() {
    let small = peak_while_streaming(200);
    let large = peak_while_streaming(20_000);
    // About 9 MB of XML in the large run; a whole read would hold several megabytes.
    assert!(
        large <= small + 64 * 1024,
        "200 records: {small} bytes, 20,000 records: {large} bytes"
    );
    assert!(large < 1 << 20, "{large} bytes");
}
