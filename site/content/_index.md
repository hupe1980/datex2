+++
title = "datex2"
description = "DATEX II for Rust: read, stream and write European road traffic and travel information (DATEX II v3, XML and JSON) as typed data, including AFIR charging data for the Mobilithek. Tolerant reader, strict writer, tested on national feeds."
template = "index.html"
+++

```rust,no_run
use datex2::v3::{self, Publication};
use std::{fs::File, io::BufReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = BufReader::new(File::open("incidents.xml")?);
    let doc = v3::read(file)?;

    for publication in &doc.publications {
        if let Publication::Situation(p) = publication {
            println!("{} situations", p.situation.len());
        }
    }
    for warning in &doc.warnings {
        eprintln!("{warning}"); // what was tolerated, and where
    }
    Ok(())
}
```
