use datex2::v3::{self, Publication, common, situation::SituationPublication};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let publication = Publication::Situation(Box::new(SituationPublication {
        lang: Some("en".into()),
        publication_time: Some("2026-11-19T12:00:00Z".parse()?),
        publication_creator: Some(Box::new(common::InternationalIdentifier {
            country: Some("NL".into()),
            national_identifier: Some("EXAMPLE".into()),
            ..Default::default()
        })),
        ..Default::default()
    }));
    v3::write(std::io::stdout().lock(), &publication)?;
    Ok(())
}
