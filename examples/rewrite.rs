//! Reads a DATEX II v3 document and writes it again, to show what the strict writer makes of it.
//!
//! ```text
//! cargo run --release --example rewrite -- in.xml > out.xml
//! ```

use std::io::{BufReader, BufWriter, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: rewrite <file>")?;
    let doc = datex2::v3::read(BufReader::new(std::fs::File::open(path)?))?;
    let mut out = BufWriter::new(std::io::stdout().lock());
    for publication in &doc.publications {
        datex2::v3::write(&mut out, publication)?;
    }
    out.flush()?;
    Ok(())
}
