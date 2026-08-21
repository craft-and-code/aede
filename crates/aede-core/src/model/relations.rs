//! Deriving the typed links between entities.
//!
//! Relations are **inferred**, never read from a file: they follow from the
//! credits and from the track lists. That is what [`RELATION_RULES`] versions,
//! and what lets a stored catalog rebuild them on load without touching the
//! disk — the reason the raw tags are kept per file in the first place.
//!
//! Two families so far. Artists who appear on the same recording are linked to
//! one another, weighted by how often. And an album that is present twice is
//! two releases and one relation between them, qualified as [`DUPLICATE`] or
//! [`OTHER_EDITION`] depending on whether the second copy is encoded the same
//! way — because the folder is what the user acts on, and merging two folders
//! into one release would take that away.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::text;

use super::{Catalog, EntityKind, Id, Relation, is_performing_role};

/// Version of the rules that derive the `relation` table.
///
/// The relations are **inferred**, not read: they follow from the credits and
/// from the track lists. Changing how they are inferred makes a stored catalog
/// out of date without making it invalid, which is why this is not
/// `store::FORMAT_VERSION` — refusing to load would be out of proportion, and
/// would throw away integrity verdicts that cost hours to obtain. Bump this
/// instead, and every catalog rebuilds its relations on the next load.
pub const RELATION_RULES: u32 = 1;

/// Recomputes every inferred relation from the entities already in place.
///
/// Needs no disk access: the credits and the tracks hold everything. That is
/// exactly what keeping the raw tags per file was for.
pub fn rebuild_relations(catalog: &mut Catalog) {
    catalog.relations.clear();
    build_collaboration_graph(catalog);
    build_release_relations(catalog);
}

/// Names of the two relations that tie one album to another copy of itself.
pub const DUPLICATE: &str = "duplicate";
/// Same album, encoded differently: a deliberate second copy.
pub const OTHER_EDITION: &str = "other_edition";

/// Links the releases that are the same album twice.
///
/// The same album legitimately appears twice in a library — a hi-res copy
/// beside the CD rip, a FLAC beside the MP3 for the car — and illegitimately
/// too, when a folder was copied and forgotten. The model keeps them as two
/// releases either way, because they *are* two sets of files in two folders and
/// that is what one needs to act on. What was missing is the link between them,
/// and the reason for it.
///
/// The two are told apart by their audio, not by their folder: same album
/// artist, same title, same track list, and then
///
/// - the same quality on both sides — nothing distinguishes the copies, and one
///   of them is wasted space;
/// - a different quality — the second copy is there on purpose.
///
/// Anything else keeps the weaker `other_edition` link: a deluxe edition with
/// three bonus tracks is not a duplicate, but it is not unrelated either.
fn build_release_relations(catalog: &mut Catalog) {
    let mut groups: BTreeMap<(Option<Id>, String), Vec<Id>> = BTreeMap::new();
    for release in &catalog.releases {
        groups
            .entry((release.album_artist_id, release.key.clone()))
            .or_default()
            .push(release.id);
    }

    let mut links: Vec<(Id, Id, &'static str)> = Vec::new();
    for ids in groups.values().filter(|ids| ids.len() > 1) {
        for (i, &left) in ids.iter().enumerate() {
            for &right in &ids[i + 1..] {
                // Two albums sharing a name are not necessarily the same
                // album: without a matching track list there is nothing
                // reliable to say, and MusicBrainz will settle it at M1.
                if !same_track_list(catalog, left, right) {
                    continue;
                }
                let kind =
                    if quality_fingerprint(catalog, left) == quality_fingerprint(catalog, right) {
                        DUPLICATE
                    } else {
                        OTHER_EDITION
                    };
                links.push((left, right, kind));
            }
        }
    }

    for (left, right, kind) in links {
        // Symmetric, like the collaboration graph: navigation works from
        // either side.
        for (source, target) in [(left, right), (right, left)] {
            catalog.relations.push(Relation {
                source_kind: EntityKind::Release,
                source_id: source,
                target_kind: EntityKind::Release,
                target_id: target,
                kind: kind.to_string(),
                weight: 1,
                source: "tags".into(),
            });
        }
    }
}

/// `true` when two releases hold the same tracks.
///
/// Positions and titles have to match exactly; durations only have to be
/// **close**. Two rips of one disc differ by a few hundred milliseconds, and a
/// transcode to a lossy format shifts the end of a track further still — but a
/// live rendition of the same song differs by minutes. Three seconds is the
/// tolerance the duplicate-track check uses, for the same reason.
fn same_track_list(catalog: &Catalog, left: Id, right: Id) -> bool {
    let (left, right) = (track_list(catalog, left), track_list(catalog, right));
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(a, b)| a.0 == b.0 && a.1 == b.1 && a.2.abs_diff(b.2) <= 3_000)
}

/// Positions, titles and durations of a release, in a comparable order.
fn track_list(catalog: &Catalog, release_id: Id) -> Vec<(u32, String, u64)> {
    let Some(release) = catalog.release(release_id) else {
        return Vec::new();
    };
    let mut out: Vec<(u32, String, u64)> = release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .map(|t| {
            (
                t.track_no.unwrap_or(0),
                text::normalize(&t.title),
                t.duration_ms.unwrap_or(0),
            )
        })
        .collect();
    out.sort();
    out
}

/// How the release is encoded, which is what separates a wasted copy from a
/// second one kept on purpose.
fn quality_fingerprint(catalog: &Catalog, release_id: Id) -> BTreeSet<String> {
    let Some(release) = catalog.release(release_id) else {
        return BTreeSet::new();
    };
    release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .filter_map(|t| catalog.file(t.file_id))
        .map(|f| f.properties.quality_label())
        .collect()
}

/// Two artists credited on the same track are considered to have
/// collaborated. The weight counts the shared tracks: that is what allows
/// ranking by "played the most with".
fn build_collaboration_graph(catalog: &mut Catalog) {
    let mut per_track: HashMap<Id, BTreeSet<Id>> = HashMap::new();
    for credit in &catalog.credits {
        // Only performers: sharing a composer does not mean two artists ever
        // met, let alone played together.
        if credit.entity_kind == EntityKind::Track && is_performing_role(&credit.role) {
            per_track
                .entry(credit.entity_id)
                .or_default()
                .insert(credit.artist_id);
        }
    }

    let mut weights: BTreeMap<(Id, Id), u32> = BTreeMap::new();
    for artists in per_track.values() {
        let list: Vec<Id> = artists.iter().copied().collect();
        for (i, &a) in list.iter().enumerate() {
            for &b in &list[i + 1..] {
                *weights.entry((a, b)).or_insert(0) += 1;
            }
        }
    }

    for ((a, b), weight) in weights {
        // The relation is symmetric: it is stored in both directions so that
        // navigation is direct from either side.
        catalog.relations.push(Relation {
            source_kind: EntityKind::Artist,
            source_id: a,
            target_kind: EntityKind::Artist,
            target_id: b,
            kind: "collaborated".into(),
            weight,
            source: "tags".into(),
        });
        catalog.relations.push(Relation {
            source_kind: EntityKind::Artist,
            source_id: b,
            target_kind: EntityKind::Artist,
            target_id: a,
            kind: "collaborated".into(),
            weight,
            source: "tags".into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::build;
    use crate::model::tests::track;

    #[test]
    fn the_same_album_twice_is_linked_and_qualified() {
        // Same album in two folders: a copy when the encoding matches, another
        // edition when it does not. The model keeps both — they are two sets of
        // files — and says which case it is.
        let make = |folder: &str, codec: &str, rate: u32| {
            let mut f = track(
                &format!("{folder}/01.flac"),
                &[
                    ("title", "Brand New God"),
                    ("artist", "Danzig"),
                    ("albumartist", "Danzig"),
                    ("album", "Danzig 4"),
                    ("tracknumber", "1"),
                ],
                120_000,
            );
            f.tags.properties.codec = codec.to_string();
            f.tags.properties.sample_rate = Some(rate);
            f.tags.properties.bit_depth = Some(if rate > 48_000 { 24 } else { 16 });
            f
        };
        let c = build(
            vec![
                make("/m/A", "flac", 44_100),
                make("/m/B", "flac", 44_100),
                make("/m/C", "flac", 96_000),
            ],
            vec!["/m".into()],
            0,
        );
        assert_eq!(c.releases.len(), 3, "three folders, three releases");
        let a = c.releases[0].id;
        let copies = c.related_releases(a, DUPLICATE);
        assert_eq!(copies.len(), 1, "one identical copy");
        let others = c.related_releases(a, OTHER_EDITION);
        assert_eq!(others.len(), 1, "one differently encoded copy");
        assert_ne!(copies[0], others[0]);
        // Symmetric: the link is navigable from either side.
        assert!(c.related_releases(copies[0], DUPLICATE).contains(&a));
    }

    #[test]
    fn two_editions_in_two_folders_stay_two_releases() {
        // Same title, same artist, different folder: two pressings of one
        // record must not be merged into a single release.
        let fields = [
            ("title", "So What"),
            ("artist", "Miles Davis"),
            ("albumartist", "Miles Davis"),
            ("album", "Kind of Blue"),
        ];
        let c = build(
            vec![
                track("/m/Miles Davis/Kind of Blue/01.flac", &fields, 1000),
                track(
                    "/m/Miles Davis/Kind of Blue (2011 remaster)/01.flac",
                    &fields,
                    1000,
                ),
            ],
            vec!["/m".into()],
            0,
        );
        assert_eq!(c.releases.len(), 2, "the folder tells the editions apart");
        assert_eq!(c.artists.len(), 1, "but the artist is shared");
    }
}
