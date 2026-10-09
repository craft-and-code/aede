//! Missing studio albums are derived from trusted discographies, never catalog entities.
use crate::model::Catalog;
use crate::sources::{self, Facts, KnownRelease};
use crate::text;

/// One record credited to an artist that this catalog does not hold.
pub struct Absent<'a> {
    /// The artist, as the catalog spells them.
    pub artist: String,
    /// Why this row is not normally on the report, and `None` when it is.
    ///
    /// Two quite different reasons, in one field because they answer one
    /// question — *why am I not being shown this* — and because a record can
    /// be both: a live album the reader also set aside. The words for the
    /// first are MusicBrainz's own; see [`KnownRelease::stated_type`].
    pub held_back: Option<String>,
    /// `true` when the reader set this one aside and asked to see it anyway.
    pub set_aside: bool,
    /// Which artist, for a caller asking about one rather than about all.
    pub artist_id: crate::model::Id,
    /// The record MusicBrainz credits to them.
    pub known: &'a KnownRelease,
}

/// What is credited to your artists and not on your shelf.
///
/// **Derived, never stored.** The comparison happens here, at the moment of
/// reading, so the day an album is added to the library it stops being listed —
/// with nothing to update and nothing to go stale.
///
/// Two ways of recognising a record you already have, in this order:
///
/// 1. the **release-group identifier**, when your tags carry one. Exact, and
///    the only comparison that cannot be wrong.
/// 2. the **normalised title**, otherwise. `text::normalize` is the same
///    function that decides two spellings are one name everywhere else in this
///    program, so a shelf that says "Kind Of Blue" is not told it is missing
///    "Kind of Blue".
///
/// Two things are held back, and `everything` lifts both.
///
/// Only studio albums are reported — see [`KnownRelease::is_studio_album`] for
/// why a complete discography would be a true and useless answer — and only
/// records the reader has not set aside. Those are the report's two filters,
/// and `--all` is one word for "hold nothing back", so it lifts both and every
/// row that came back this way carries the reason it would not normally be
/// here. **A filter is only honest when the reader can turn it off**: saying
/// what is left out and offering no way to see it is a locked door with a
/// label on it.
pub fn absent<'a>(
    catalog: &Catalog,
    held: &'a sources::Sources,
    aside: &[crate::user::SetAside],
    everything: bool,
) -> Vec<Absent<'a>> {
    let mut out: Vec<Absent<'a>> = Vec::new();
    for record in &held.records {
        if record.source != sources::MUSICBRAINZ {
            continue;
        }
        if !held.is_trusted(catalog, record) {
            continue;
        }
        let Facts::Artist(facts) = &record.facts else {
            continue;
        };
        if facts.discography.is_empty() {
            continue;
        }
        let entity = record.entity();
        // An artist the catalog cannot place has no shelf to be missing from.
        let Some(artist_id) = entity.resolve(catalog) else {
            continue;
        };
        let Some(artist) = catalog.artist(artist_id) else {
            continue;
        };

        // What is on the shelf, in both spellings the comparison uses.
        let mut ids: Vec<&str> = Vec::new();
        let mut titles: Vec<String> = Vec::new();
        for release in &catalog.releases {
            if release.album_artist_id != Some(artist_id) {
                continue;
            }
            if let Some(group) = release.release_group_mbid.as_deref() {
                ids.push(group);
            }
            titles.push(text::normalize(&release.title));
        }

        // **No album of their own, no shelf to have gaps in.** The catalog
        // holds an artist for every credit it reads — a guest on one track, a
        // composer, one name on a compilation — and being in the catalog is
        // not the same as having a place in the library. The first version
        // missed that distinction and answered a question nobody asked: one
        // Rolling Stones track on a compilation produced their entire studio
        // discography as "missing", which is true and worthless, and it did
        // so for every passing credit at once until the report was mostly
        // that. What this reports is an *incomplete* discography, which means
        // one that was started.
        //
        // The same rule decides who is browsed at all — see the discography fetch selection.
        if titles.is_empty() {
            continue;
        }

        for known in &facts.discography {
            // Set aside by hand. Filtered here rather than by dropping it from
            // the layer, because the two are different claims and this program
            // keeps them apart everywhere else: MusicBrainz still says this is
            // an album, and the user still says it is not one they want listed.
            // Deleting the record would lose the first to record the second.
            let put_aside = aside.iter().any(|a| a.release_group == known.mbid);

            // Both reasons, in the order a reader meets them: what it is, then
            // what they decided about it. A record can be both, and naming only
            // one would answer half the question.
            let mut why: Vec<String> = Vec::new();
            if !known.is_studio_album() {
                why.push(known.stated_type());
            }
            if put_aside {
                why.push("set aside".to_string());
            }
            let held_back = (!why.is_empty()).then(|| why.join(", "));
            if held_back.is_some() && !everything {
                continue;
            }

            let have = ids.iter().any(|id| *id == known.mbid)
                || titles.contains(&text::normalize(&known.title));
            if !have {
                out.push(Absent {
                    artist: artist.name.clone(),
                    held_back,
                    set_aside: put_aside,
                    artist_id,
                    known,
                });
            }
        }
    }
    // Oldest first within an artist, so a discography reads as one.
    out.sort_by(|a, b| {
        a.artist
            .cmp(&b.artist)
            .then_with(|| a.known.first_released.cmp(&b.known.first_released))
    });
    out
}

/// How many records one artist has that this shelf does not.
///
/// For a page to say so at the moment somebody is looking at that artist —
/// which is the moment they wonder. `aede missing` was in the help, in the
/// README and in the manual, and still could not be found, because it was
/// named nowhere near the question it answers. The same fault `aede extract`
/// had, and the same fix.
///
/// Derived like everything else here: nothing is stored, so the number falls
/// the day an album is added.
pub fn absent_for(
    catalog: &Catalog,
    held: &sources::Sources,
    aside: &[crate::user::SetAside],
    artist_id: crate::model::Id,
) -> usize {
    absent(catalog, held, aside, false)
        .iter()
        .filter(|record| record.artist_id == artist_id)
        .count()
}
