//! Deriving the typed links between entities.
//!
//! Relations are **inferred**, never read from a file: they follow from the
//! credits and from the track lists. That is what [`RELATION_RULES`] versions,
//! and what lets a stored catalog rebuild them on load without touching the
//! disk — the reason the raw tags are kept per file in the first place.
//!
//! Structural links are derived alongside the two original inferred families:
//! placements, recordings, works, editions, release groups, labels and album
//! artists can all be traversed in either direction. Artist participation is
//! derived from credits without copying credit details into the relation row.
//! Collaborations and duplicate/other-edition links remain weighted summaries.

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
pub const RELATION_RULES: u32 = 2;

/// Recomputes every inferred relation from the entities already in place.
///
/// Needs no disk access: the credits and the tracks hold everything. That is
/// exactly what keeping the raw tags per file was for.
pub fn rebuild_relations(catalog: &mut Catalog) {
    catalog.relations.clear();
    build_entity_graph(catalog);
    build_collaboration_graph(catalog);
    build_release_relations(catalog);
}

/// Names of the two relations that tie one album to another copy of itself.
pub const DUPLICATE: &str = "duplicate";
/// Same album, encoded differently: a deliberate second copy.
pub const OTHER_EDITION: &str = "other_edition";
/// A local track placement points to the abstract recording it carries.
pub const PLACEMENT_OF: &str = "placement_of";
/// Reverse of [`PLACEMENT_OF`].
pub const PLACED_AS: &str = "placed_as";
/// A local track belongs to one edition.
pub const PART_OF_RELEASE: &str = "part_of_release";
/// Reverse of [`PART_OF_RELEASE`].
pub const HAS_TRACK: &str = "has_track";
/// A recording realizes a musical work.
pub const PERFORMANCE_OF: &str = "performance_of";
/// Reverse of [`PERFORMANCE_OF`].
pub const HAS_RECORDING: &str = "has_recording";
/// A release is one edition of a release group.
pub const EDITION_OF: &str = "edition_of";
/// Reverse of [`EDITION_OF`].
pub const HAS_EDITION: &str = "has_edition";
/// A release was issued by a label.
pub const RELEASED_BY: &str = "released_by";
/// Reverse of [`RELEASED_BY`].
pub const RELEASED: &str = "released";
/// A release is primarily credited to an album artist.
pub const ALBUM_BY: &str = "album_by";
/// Reverse of [`ALBUM_BY`].
pub const DISCOGRAPHY: &str = "discography";
/// A guest performer appears on another artist's release.
pub const APPEARS_ON: &str = "appears_on";
/// A performer appears on a compilation.
pub const COMPILATION_APPEARANCE: &str = "compilation_appearance";
/// A non-performing credit contributes to a release.
pub const CONTRIBUTED_TO: &str = "contributed_to";

/// Adds the structural and credit-derived edges of the local graph.
fn build_entity_graph(catalog: &mut Catalog) {
    let mut links: Vec<(EntityKind, Id, EntityKind, Id, String, u32)> = Vec::new();
    let mut pair = |a_kind, a, forward: &str, b_kind, b, backward: &str| {
        links.push((a_kind, a, b_kind, b, forward.to_string(), 1));
        links.push((b_kind, b, a_kind, a, backward.to_string(), 1));
    };

    for track in &catalog.tracks {
        pair(
            EntityKind::Track,
            track.id,
            PLACEMENT_OF,
            EntityKind::Recording,
            track.recording_id,
            PLACED_AS,
        );
        if let Some(release_id) = track.release_id {
            pair(
                EntityKind::Track,
                track.id,
                PART_OF_RELEASE,
                EntityKind::Release,
                release_id,
                HAS_TRACK,
            );
        }
    }
    for recording in &catalog.recordings {
        for &work_id in &recording.work_ids {
            pair(
                EntityKind::Recording,
                recording.id,
                PERFORMANCE_OF,
                EntityKind::Work,
                work_id,
                HAS_RECORDING,
            );
        }
    }
    for release in &catalog.releases {
        if let Some(group_id) = release.release_group_id {
            pair(
                EntityKind::Release,
                release.id,
                EDITION_OF,
                EntityKind::ReleaseGroup,
                group_id,
                HAS_EDITION,
            );
        }
        for &label_id in &release.label_ids {
            pair(
                EntityKind::Release,
                release.id,
                RELEASED_BY,
                EntityKind::Label,
                label_id,
                RELEASED,
            );
        }
        if let Some(artist_id) = release.album_artist_id {
            pair(
                EntityKind::Release,
                release.id,
                ALBUM_BY,
                EntityKind::Artist,
                artist_id,
                DISCOGRAPHY,
            );
        }
    }

    let mut recording_credits: BTreeMap<(Id, Id, String), u32> = BTreeMap::new();
    let mut release_participations: BTreeMap<(Id, Id, &'static str), BTreeSet<Id>> =
        BTreeMap::new();
    for credit in &catalog.credits {
        if credit.entity_kind != EntityKind::Track {
            continue;
        }
        let Some(track) = catalog.track(credit.entity_id) else {
            continue;
        };
        *recording_credits
            .entry((credit.artist_id, track.recording_id, credit.role.clone()))
            .or_insert(0) += 1;
        let Some(release_id) = track.release_id else {
            continue;
        };
        let Some(release) = catalog.release(release_id) else {
            continue;
        };
        let kind = if is_performing_role(&credit.role) {
            if release.album_artist_id == Some(credit.artist_id) {
                continue;
            }
            if release.is_compilation {
                COMPILATION_APPEARANCE
            } else {
                APPEARS_ON
            }
        } else {
            CONTRIBUTED_TO
        };
        release_participations
            .entry((credit.artist_id, release_id, kind))
            .or_default()
            .insert(track.id);
    }
    for ((artist_id, recording_id, role), weight) in recording_credits {
        let kind = format!("credit:{role}");
        links.push((
            EntityKind::Artist,
            artist_id,
            EntityKind::Recording,
            recording_id,
            kind.clone(),
            weight,
        ));
        links.push((
            EntityKind::Recording,
            recording_id,
            EntityKind::Artist,
            artist_id,
            kind,
            weight,
        ));
    }
    for ((artist_id, release_id, kind), tracks) in release_participations {
        let weight = tracks.len() as u32;
        links.push((
            EntityKind::Artist,
            artist_id,
            EntityKind::Release,
            release_id,
            kind.to_string(),
            weight,
        ));
        links.push((
            EntityKind::Release,
            release_id,
            EntityKind::Artist,
            artist_id,
            kind.to_string(),
            weight,
        ));
    }

    catalog.relations.extend(links.into_iter().map(
        |(source_kind, source_id, target_kind, target_id, kind, weight)| Relation {
            source_kind,
            source_id,
            target_kind,
            target_id,
            kind,
            weight,
            source: "tags".into(),
        },
    ));
}

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
            &[],
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
            &[],
        );
        assert_eq!(c.releases.len(), 2, "the folder tells the editions apart");
        assert_eq!(c.artists.len(), 1, "but the artist is shared");
    }

    #[test]
    fn every_canonical_object_link_is_navigable_both_ways() {
        let c = build(
            vec![track(
                "/m/Band/Record/01.flac",
                &[
                    ("title", "Song"),
                    ("artist", "Band"),
                    ("albumartist", "Band"),
                    ("album", "Record"),
                    ("label", "A Label"),
                    ("musicbrainz_recordingid", "recording-id"),
                    ("musicbrainz_workid", "work-id"),
                    ("musicbrainz_releasegroupid", "group-id"),
                    ("performer", "Guest"),
                    ("composer", "Writer"),
                ],
                1000,
            )],
            vec!["/m".into()],
            0,
            &[],
        );
        let track = &c.tracks[0];
        let recording = &c.recordings[0];
        let release = &c.releases[0];
        let work = &c.works[0];
        let group = &c.release_groups[0];
        let label = &c.labels[0];
        let band = c.find_artist("Band").expect("band");
        let guest = c.find_artist("Guest").expect("guest");
        let writer = c.find_artist("Writer").expect("writer");

        assert_eq!(
            c.related_entities(
                EntityKind::Track,
                track.id,
                PLACEMENT_OF,
                EntityKind::Recording
            ),
            vec![recording.id]
        );
        assert_eq!(
            c.related_entities(
                EntityKind::Recording,
                recording.id,
                PLACED_AS,
                EntityKind::Track
            ),
            vec![track.id]
        );
        assert_eq!(
            c.related_entities(
                EntityKind::Recording,
                recording.id,
                PERFORMANCE_OF,
                EntityKind::Work
            ),
            vec![work.id]
        );
        assert_eq!(
            c.related_entities(
                EntityKind::Work,
                work.id,
                HAS_RECORDING,
                EntityKind::Recording
            ),
            vec![recording.id]
        );
        assert_eq!(
            c.related_entities(
                EntityKind::Release,
                release.id,
                EDITION_OF,
                EntityKind::ReleaseGroup
            ),
            vec![group.id]
        );
        assert_eq!(
            c.related_entities(
                EntityKind::ReleaseGroup,
                group.id,
                HAS_EDITION,
                EntityKind::Release
            ),
            vec![release.id]
        );
        assert_eq!(
            c.related_entities(
                EntityKind::Release,
                release.id,
                RELEASED_BY,
                EntityKind::Label
            ),
            vec![label.id]
        );
        assert_eq!(
            c.related_entities(EntityKind::Label, label.id, RELEASED, EntityKind::Release),
            vec![release.id]
        );
        assert_eq!(
            c.related_entities(
                EntityKind::Artist,
                band.id,
                DISCOGRAPHY,
                EntityKind::Release
            ),
            vec![release.id]
        );
        assert_eq!(
            c.related_entities(
                EntityKind::Artist,
                guest.id,
                APPEARS_ON,
                EntityKind::Release
            ),
            vec![release.id]
        );
        assert_eq!(
            c.related_entities(
                EntityKind::Artist,
                writer.id,
                CONTRIBUTED_TO,
                EntityKind::Release
            ),
            vec![release.id]
        );
        assert_eq!(
            c.related_entities(
                EntityKind::Artist,
                writer.id,
                "credit:composer",
                EntityKind::Recording
            ),
            vec![recording.id]
        );
    }

    #[test]
    fn a_compilation_appearance_is_not_a_discography_or_guest_album() {
        let c = build(
            vec![track(
                "/m/Various/Collection/01.flac",
                &[
                    ("title", "Song"),
                    ("artist", "Guest"),
                    ("albumartist", "Various Artists"),
                    ("album", "Collection"),
                    ("compilation", "1"),
                ],
                1000,
            )],
            vec!["/m".into()],
            0,
            &[],
        );
        let guest = c.find_artist("Guest").expect("guest");
        let release = &c.releases[0];
        assert_eq!(
            c.related_entities(
                EntityKind::Artist,
                guest.id,
                COMPILATION_APPEARANCE,
                EntityKind::Release
            ),
            vec![release.id]
        );
        assert!(
            c.related_entities(
                EntityKind::Artist,
                guest.id,
                APPEARS_ON,
                EntityKind::Release
            )
            .is_empty()
        );
        assert!(
            c.related_entities(
                EntityKind::Artist,
                guest.id,
                DISCOGRAPHY,
                EntityKind::Release
            )
            .is_empty()
        );
    }
}
