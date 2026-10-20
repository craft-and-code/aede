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
            let id = record.source_id.as_deref()?;
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
        let Facts::Artist(existing) = &mut held.records[index].facts else {
            continue;
        };
        // Merged MBIDs may redirect. Keep the reviewed selector and its score;
        // the requested identifier remains an official alias, not a new match.
        let mut facts = found.facts;
        facts.discography = std::mem::take(&mut existing.discography);
        facts.discography_fetched_at = existing.discography_fetched_at;
        *existing = facts;
        held.records[index].fetched_at = clock::now_seconds();
        sources::save(held, path)?;
    }
    Ok(())
}
