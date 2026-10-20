//! Complete accepted artist-search identities without upgrading their confidence or changing tags.
use super::*;
use aede_core::{model::Catalog, musicbrainz};

pub(super) fn complete(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    held: &mut sources::Sources,
    path: &std::path::Path,
    asked: &super::super::fetch::Asked,
    catalog: Option<&Catalog>,
) -> Res {
    let empty = Catalog::default();
    let catalog = catalog.unwrap_or(&empty);
    let targets: Vec<_> = held
        .records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            let Facts::Artist(facts) = &record.facts else {
                return None;
            };
            let id = super::super::fetch::lookup_selector(catalog, record)?;
            (record.source == sources::MUSICBRAINZ
                && musicbrainz::valid_id(id)
                && held.is_trusted(catalog, record)
                && asked.scope.has_artist(&record.key)
                && super::super::fetch::reaches(asked.names, &[&record.key])
                && facts
                    .wikidata
                    .as_deref()
                    .and_then(wikipedia::entity_id)
                    .is_none()
                && (!facts.relationships_complete || asked.again))
                .then(|| (index, id.to_string(), record.key.clone()))
        })
        .collect();
    if targets.is_empty() {
        return Ok(());
    }
    if super::super::fetch::asked_nothing(
        asked,
        &targets
            .iter()
            .map(|(_, _, name)| name.clone())
            .collect::<Vec<_>>(),
    ) {
        return Ok(());
    }
    for (index, id, name) in targets {
        println!("  completing MusicBrainz links for {}", ui::literal(&name));
        let url = format!(
            "{}/artist/{id}?fmt=json&inc={}",
            musicbrainz::WEB_SERVICE,
            musicbrainz::ARTIST_INCLUDES
        );
        let answer = match ask_with_backoff(transport, &url, backoff) {
            Ok(answer) => answer,
            Err(failure) if failure.must_stop() => return Err(failure.into()),
            Err(failure) => {
                super::super::fetch::report_failed(&name, &failure);
                continue;
            }
        };
        let Some(found) =
            musicbrainz::artist(&answer).filter(|candidate| musicbrainz::valid_id(&candidate.mbid))
        else {
            super::super::fetch::report_failed(
                &name,
                &"MusicBrainz artist details are unreadable; existing claims retained",
            );
            continue;
        };
        let previous = &held.records[index];
        let review = super::super::fetch::continued_lookup_review(
            held,
            &previous.entity(),
            &found.mbid,
            Some(&id),
        );
        let previous = &mut held.records[index];
        let Facts::Artist(existing) = &mut previous.facts else {
            continue;
        };
        // A redirect changes its canonical address, not its score or decision.
        let mut facts = found.facts;
        facts.lookup_mbid = (id != found.mbid)
            .then_some(id)
            .or_else(|| existing.lookup_mbid.take());
        facts.discography = std::mem::take(&mut existing.discography);
        facts.discography_fetched_at = existing.discography_fetched_at;
        let updated = SourceRecord {
            key: previous.key.clone(),
            source: previous.source.clone(),
            source_id: Some(found.mbid),
            confidence: previous.confidence,
            fetched_at: clock::now_seconds(),
            facts: Facts::Artist(facts),
        };
        held.set(updated);
        if let Some(review) = review {
            held.set_review(review);
        }
        sources::save(held, path)?;
    }
    Ok(())
}
