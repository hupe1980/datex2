use datex2::v3::{self, Publication};
use std::{fs::File, io::BufReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = v3::read(BufReader::new(File::open("incidents.xml")?))?;
    for publication in &doc.publications {
        if let Publication::Situation(p) = publication {
            println!("{} situations", p.situation.len());
        }
    }
    for warning in &doc.warnings {
        eprintln!("{warning}");
    }
    Ok(())
}
