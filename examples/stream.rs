use datex2::v3::{self, situation::Situation};
use std::{fs::File, io::BufReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = BufReader::new(File::open("roadworks.xml")?);
    for situation in v3::stream::<Situation>(file)? {
        let situation = situation?;
        println!(
            "{:?}: {} records",
            situation.id,
            situation.situation_record.len()
        );
    }
    Ok(())
}
