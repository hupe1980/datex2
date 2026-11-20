//! Reads a DATEX II v3 document, XML or JSON, and writes it again, to show what the strict writers
//! make of it: as XML, as DATEX II 3.7 JSON (`--json`) or for the Mobilithek (`--mobilithek`).
//!
//! ```text
//! cargo run --release --example rewrite -- in.xml > out.xml
//! cargo run --release --example rewrite -- --json in.xml > out.json
//! ```

use std::io::{BufReader, BufWriter, Write};

use datex2::v3::{self, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let format = match args.first().map(String::as_str) {
        Some(f @ ("--json" | "--mobilithek")) => {
            let f = f.to_owned();
            args.remove(0);
            f
        }
        _ => String::new(),
    };
    let path = args
        .first()
        .ok_or("usage: rewrite [--json | --mobilithek] <file>")?;
    let input = BufReader::new(std::fs::File::open(path)?);
    let doc = if path.ends_with(".json") {
        json::read(input)?
    } else {
        v3::read(input)?
    };
    let mut out = BufWriter::new(std::io::stdout().lock());
    for publication in &doc.publications {
        match format.as_str() {
            "--json" => json::write(&mut out, publication)?,
            "--mobilithek" => json::mobilithek::write(&mut out, publication)?,
            _ => v3::write(&mut out, publication)?,
        }
    }
    out.flush()?;
    Ok(())
}
