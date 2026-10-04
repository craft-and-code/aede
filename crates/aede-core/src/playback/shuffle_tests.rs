use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Artist, Credit, EntityKind, Genre, GenreLink, Release, Track};

use super::*;

fn fixture(specs: &[(&[&str], Id, Id)]) -> Catalog {
    let mut catalog = Catalog::default();
    let mut genres = BTreeMap::new();
    let mut release_links = BTreeSet::new();
    let last_artist = specs.iter().map(|s| s.1).max().unwrap_or(0);
    let last_album = specs.iter().map(|s| s.2).max().unwrap_or(0);
    catalog.artists = (0..=last_artist)
        .map(|id| Artist {
            id,
            name: format!("Artist {id}"),
            ..Artist::default()
        })
        .collect();
    catalog.releases = (0..=last_album)
        .map(|id| Release {
            id,
            title: format!("Album {id}"),
            ..Release::default()
        })
        .collect();
    for (index, &(names, artist, album)) in specs.iter().enumerate() {
        let id = index as Id;
        catalog.tracks.push(Track {
            id,
            release_id: Some(album),
            title: format!("Track {id}"),
            ..Track::default()
        });
        catalog.releases[album as usize].track_ids.push(id);
        catalog.credits.push(Credit {
            artist_id: artist,
            entity_kind: EntityKind::Track,
            entity_id: id,
            role: "main".into(),
            credited_as: None,
            attributes: Vec::new(),
            began: None,
            ended: None,
            order: None,
            source: "tags".into(),
            source_id: None,
        });
        for &name in names {
            let genre_id = *genres.entry(name.to_string()).or_insert_with(|| {
                let genre_id = catalog.genres.len() as Id;
                catalog.genres.push(Genre {
                    id: genre_id,
                    name: name.into(),
                    key: crate::text::normalize(name),
                });
                genre_id
            });
            catalog.genre_links.push(GenreLink {
                genre_id,
                entity_kind: EntityKind::Track,
                entity_id: id,
            });
            if release_links.insert((album, genre_id)) {
                catalog.genre_links.push(GenreLink {
                    genre_id,
                    entity_kind: EntityKind::Release,
                    entity_id: album,
                });
            }
        }
    }
    catalog
}

fn selection(catalog: &Catalog) -> Vec<Option<Id>> {
    catalog.tracks.iter().map(|t| Some(t.id)).collect()
}

#[test]
fn journey_reproduces_every_occurrence_including_duplicate_and_unknown_tracks() {
    let catalog = fixture(&[
        (&["classical"], 0, 0),
        (&["third stream"], 1, 1),
        (&["jazz"], 2, 2),
        (&["soul"], 3, 3),
        (&["funk"], 4, 4),
        (&["hip hop"], 5, 5),
        (&["rap"], 6, 6),
    ]);
    let mut tracks = selection(&catalog);
    tracks.extend([Some(2), None, Some(9_999)]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &tracks);
    for seed in 0..32 {
        let one = shuffle.plan(seed, None, None);
        assert_eq!(one, shuffle.plan(seed, None, None));
        let mut indices = one.order;
        indices.sort_unstable();
        assert_eq!(indices, (0..tracks.len()).collect::<Vec<_>>());
        assert_eq!(one.report.unknown_tracks, 2);
    }
}

#[test]
fn unknown_selection_uses_classic_uniform_order_and_reports_missing_information() {
    let catalog = Catalog::default();
    let shuffle = SmartShuffle::from_catalog(&catalog, &[None; 20]);
    let planned = shuffle.plan(42, None, None);
    let mut classic: Vec<_> = (0..20).collect();
    super::super::shuffle(&mut classic, 42);
    assert_eq!(planned.order, classic);
    assert_eq!(planned.report.unknown_tracks, 20);
    assert_eq!(planned.report.unknown_transitions, 19);
    assert_eq!(planned.report.style_breaks, 0);
}

#[test]
fn empty_and_single_track_journeys_have_no_fictitious_transition() {
    let catalog = Catalog::default();
    let empty = SmartShuffle::from_catalog(&catalog, &[]);
    assert!(empty.is_empty());
    assert!(empty.plan(0, None, None).order.is_empty());
    let single = SmartShuffle::from_catalog(&catalog, &[None]);
    let planned = single.plan(0, None, None);
    assert_eq!(single.len(), 1);
    assert_eq!(planned.order, [0]);
    assert!(planned.breaks.is_empty());
    assert_eq!(planned.report.unknown_transitions, 0);
}

#[test]
fn canonical_aliases_share_style_without_merging_distinct_genres() {
    let catalog = fixture(&[
        (&["HIP-HOP"], 0, 0),
        (&["hiphop"], 1, 1),
        (&["R&B"], 2, 2),
        (&["rhythm and blues"], 3, 3),
        (&["synth-pop"], 4, 4),
        (&["synthpop"], 5, 5),
        (&["popcorn"], 6, 6),
        (&["pop"], 7, 7),
        (&["musique classique"], 8, 8),
        (&["classical"], 9, 9),
        (&["R'n'B"], 10, 10),
    ]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    assert_eq!(shuffle.transition_distance(0, 1), Some(0));
    assert_eq!(shuffle.transition_distance(2, 3), Some(0));
    assert_eq!(shuffle.transition_distance(4, 5), Some(0));
    assert_eq!(shuffle.transition_distance(6, 7), Some(1_000));
    assert_eq!(shuffle.transition_distance(0, 9_999), None);
    assert_eq!(shuffle.transition_distance(8, 9), Some(0));
    assert_eq!(shuffle.transition_distance(10, 2), Some(0));
}

#[test]
fn genre_backbone_is_symmetric_and_classical_to_rap_is_a_large_step() {
    let catalog = fixture(&[
        (&["classical"], 0, 0),
        (&["third stream"], 1, 1),
        (&["jazz"], 2, 2),
        (&["rap"], 3, 3),
    ]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    for a in 0..4 {
        for b in 0..4 {
            assert_eq!(
                shuffle.transition_distance(a, b),
                shuffle.transition_distance(b, a)
            );
        }
    }
    assert!(shuffle.transition_distance(0, 1).unwrap() <= MAX_STYLE_STEP);
    assert!(shuffle.transition_distance(0, 3).unwrap() > MAX_STYLE_STEP);
    for seed in 0..32 {
        let planned = shuffle.plan(seed, Some(0), None);
        assert_eq!(&planned.order[..2], &[0, 1]);
    }
}

#[test]
fn missing_bridges_are_reported_without_omitting_distant_styles() {
    let catalog = fixture(&[(&["classical"], 0, 0), (&["rap"], 1, 1)]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    let planned = shuffle.plan(12, Some(0), None);
    assert_eq!(planned.order, [0, 1]);
    assert_eq!(planned.report.style_breaks, 1);
    assert_eq!(planned.breaks[0].position, 1);
    assert!(planned.breaks[0].distance.unwrap() > MAX_STYLE_STEP);
}

#[test]
fn unknown_track_cannot_be_claimed_as_a_bridge_between_distant_styles() {
    let catalog = fixture(&[(&["classical"], 0, 0), (&["rap"], 1, 1)]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &[Some(0), None, Some(1)]);
    let planned = shuffle.plan(12, Some(0), None);
    assert_eq!(planned.order, [0, 2, 1]);
    assert_eq!(planned.report.style_breaks, 1);
    assert_eq!(planned.report.unknown_transitions, 1);
    assert_eq!(planned.breaks[1].distance, None);
}

#[test]
fn mixed_compilation_does_not_copy_its_genre_union_to_each_track() {
    let mut catalog = fixture(&[(&["classical"], 0, 0), (&["rap"], 1, 0), (&[], 2, 0)]);
    catalog.releases[0].is_compilation = true;
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    assert_eq!(
        shuffle.profiles[0]
            .genre_names
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>(),
        ["classical"]
    );
    assert_eq!(
        shuffle.profiles[1]
            .genre_names
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>(),
        ["rap"]
    );
    assert_eq!(shuffle.transition_distance(0, 2), None);
    assert!(shuffle.transition_distance(0, 1).unwrap() > MAX_STYLE_STEP);
}

#[test]
fn multiple_genres_preserve_both_memberships_and_discount_one_accidental_overlap() {
    let catalog = fixture(&[
        (&["classical", "rap"], 0, 0),
        (&["classical"], 1, 1),
        (&["rap"], 2, 2),
    ]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    assert_eq!(shuffle.profiles[0].genres.len(), 2);
    assert!(shuffle.transition_distance(0, 1).unwrap() < MAX_STYLE_STEP);
    assert!(shuffle.transition_distance(0, 2).unwrap() < MAX_STYLE_STEP);
    assert!(shuffle.transition_distance(0, 1).unwrap() > 0);
}

#[test]
fn direct_local_cotags_connect_unrecognized_styles_without_a_network_ontology() {
    let catalog = fixture(&[
        (&["glacial ensemble", "rustic pulse"], 0, 0),
        (&["glacial ensemble"], 1, 1),
        (&["rustic pulse"], 2, 2),
    ]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    assert!(shuffle.transition_distance(1, 2).unwrap() <= MAX_STYLE_STEP);
}

#[test]
fn release_affinity_requires_independent_corroboration() {
    let specs = [
        (&["glacial ensemble"][..], 0, 0),
        (&["rustic pulse"][..], 1, 0),
        (&["glacial ensemble"][..], 2, 1),
        (&["rustic pulse"][..], 3, 1),
    ];
    let one = fixture(&specs[..2]);
    let one = SmartShuffle::from_catalog(&one, &selection(&one));
    assert_eq!(one.transition_distance(0, 1), Some(1_000));
    let two = fixture(&specs);
    let two = SmartShuffle::from_catalog(&two, &selection(&two));
    assert!(two.transition_distance(0, 1).unwrap() <= MAX_STYLE_STEP);
}

#[test]
fn broad_artist_or_album_repertoires_do_not_shortcut_classical_to_rap() {
    let specs = [
        (&["classical"][..], 0, 0),
        (&["rap"][..], 0, 0),
        (&["classical"][..], 1, 1),
        (&["rap"][..], 1, 1),
    ];
    let catalog = fixture(&specs);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    assert!(shuffle.transition_distance(0, 1).unwrap() > MAX_STYLE_STEP);
    let planned = shuffle.plan(12, Some(0), None);
    assert_eq!(planned.report.style_breaks, 1);
    assert_eq!(
        planned.order[..2].iter().copied().collect::<BTreeSet<_>>(),
        [0, 2].into()
    );
}

#[test]
fn available_style_chain_prevents_a_premature_jump_from_classical_to_hip_hop() {
    let catalog = fixture(&[
        (&["classical"], 0, 0),
        (&["third stream"], 1, 1),
        (&["jazz"], 2, 2),
        (&["soul"], 3, 3),
        (&["funk"], 4, 4),
        (&["hip hop"], 5, 5),
        (&["rap"], 6, 6),
    ]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    for seed in 0..64 {
        let planned = shuffle.plan(seed, Some(0), None);
        assert_eq!(&planned.order[..3], &[0, 1, 2]);
        assert!(planned.order.iter().position(|&i| i == 5).unwrap() >= 4);
        assert!(planned.order.iter().position(|&i| i == 6).unwrap() >= 4);
    }
}

#[test]
fn recent_artist_and_album_diversity_are_preferences_not_track_deletion() {
    let catalog = fixture(&[(&["jazz"], 0, 0), (&["jazz"], 0, 0), (&["jazz"], 1, 1)]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    let mut diverse = 0;
    for seed in 0..256 {
        let planned = shuffle.plan(seed, Some(0), None);
        diverse += usize::from(planned.order[1] == 2);
        assert_eq!(planned.order.len(), 3);
    }
    assert!(diverse > 200, "only {diverse} diverse second tracks");
}

#[test]
fn onward_lookahead_prefers_a_candidate_with_a_remaining_safe_exit() {
    let catalog = fixture(&[
        (&["glacial ensemble"], 0, 0),
        (&["rustic pulse"], 1, 1),
        (&["luminous strings"], 2, 2),
        (&["tactile drone"], 3, 3),
        (&["glacial ensemble", "rustic pulse"], 4, 4),
        (&["glacial ensemble", "rustic pulse"], 5, 5),
        (&["glacial ensemble", "luminous strings"], 6, 6),
        (&["glacial ensemble", "luminous strings"], 7, 7),
        (&["luminous strings", "tactile drone"], 8, 8),
        (&["luminous strings", "tactile drone"], 9, 9),
    ]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &[Some(0), Some(1), Some(2), Some(3)]);
    let mut safe_exit = 0;
    for seed in 0..256 {
        let planned = shuffle.plan(seed, Some(0), None);
        safe_exit += usize::from(planned.order[1] == 2);
    }
    assert!(
        safe_exit > 200,
        "only {safe_exit} choices with a safe onward step"
    );
}

#[test]
fn repeat_boundary_and_pinned_runtime_occurrence_are_preserved_and_reported() {
    let catalog = fixture(&[(&["classical"], 0, 0), (&["rap"], 1, 1)]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    let planned = shuffle.plan(12, Some(1), Some(0));
    assert_eq!(planned.order[0], 1);
    assert_eq!(planned.breaks[0].position, 0);
    assert_eq!(planned.breaks[0].previous, 0);
    assert_eq!(planned.breaks[0].next, 1);
}

#[test]
fn runtime_remainder_does_not_plan_through_an_already_played_bridge() {
    let catalog = fixture(&[
        (&["classical"], 0, 0),
        (&["third stream"], 1, 1),
        (&["jazz"], 2, 2),
        (&["rap"], 3, 3),
    ]);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    let planned = shuffle.plan_remaining(12, &[3], Some(0)).unwrap();
    assert_eq!(planned.order, [3]);
    assert_eq!(planned.report.style_breaks, 1);
    assert_eq!(planned.breaks[0].position, 0);
    assert!(shuffle.plan_remaining(12, &[1, 1], None).is_err());
    assert!(shuffle.plan_remaining(12, &[10], None).is_err());
    assert!(
        shuffle
            .plan_remaining(12, &[], Some(0))
            .unwrap()
            .order
            .is_empty()
    );
}

#[test]
fn bounded_genre_graph_reports_metadata_loss_and_preserves_the_whole_selection() {
    let names: Vec<_> = (0..600).map(|n| format!("custom style {n}")).collect();
    let tags: Vec<_> = names.iter().map(|s| vec![s.as_str()]).collect();
    let specs: Vec<_> = tags
        .iter()
        .enumerate()
        .map(|(id, tags)| (tags.as_slice(), id as Id, id as Id))
        .collect();
    let catalog = fixture(&specs);
    let shuffle = SmartShuffle::from_catalog(&catalog, &selection(&catalog));
    assert!(shuffle.graph.len() <= 512);
    let planned = shuffle.plan(9, None, None);
    assert!(planned.report.omitted_genres > 0);
    assert!(planned.report.limited_tracks > 0);
    assert!(planned.report.unknown_tracks > 0);
    let mut indices = planned.order;
    indices.sort_unstable();
    assert_eq!(indices, (0..600).collect::<Vec<_>>());
}

#[test]
fn capped_artists_and_labels_keep_sorted_unique_metadata_and_report_each_occurrence() {
    let mut catalog = fixture(&[(&["jazz"], 0, 0), (&["jazz"], 1, 0), (&["jazz"], 2, 1)]);
    for artist in 3..14 {
        catalog.artists.push(Artist {
            id: artist,
            name: format!("Artist {artist}"),
            ..Artist::default()
        });
        let mut credit = catalog.credits[0].clone();
        credit.artist_id = artist;
        catalog.credits.push(credit);
    }
    catalog.releases[0].label_ids = vec![8, 5, 3, 7, 2, 2, 6, 4, 1, 0, 10, 8];
    catalog.releases[1].label_ids = vec![3, 1, 3, 2, 1];
    let shuffle = SmartShuffle::from_catalog(&catalog, &[Some(0), Some(1), Some(2), Some(0)]);
    assert_eq!(shuffle.profiles[0].artists, [0, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(shuffle.profiles[0].labels, [0, 1, 2, 3]);
    assert_eq!(shuffle.profiles[1].labels, [0, 1, 2, 3]);
    assert_eq!(shuffle.profiles[2].labels, [1, 2, 3]);
    assert_eq!(shuffle.profiles[3].labels, [0, 1, 2, 3]);
    assert_eq!(shuffle.plan(42, None, None).report.limited_tracks, 3);
    let remaining = shuffle.plan_remaining(42, &[2, 3], None).unwrap();
    assert_eq!(remaining.report.limited_tracks, 1);
    let mut indices = remaining.order;
    indices.sort_unstable();
    assert_eq!(indices, [2, 3]);
}

#[test]
fn oversized_genre_labels_are_reported_and_do_not_inherit_a_release_style() {
    let long = "large genre label ".repeat(10_000);
    let wide = "é".repeat(129);
    let expanding = "Ⱥ".repeat(128);
    let boundary = "x".repeat(256);
    assert_eq!(expanding.len(), 256);
    assert!(crate::text::normalize(&expanding).len() > 256);
    let catalog = fixture(&[
        (&[long.as_str()], 0, 0),
        (&["jazz"], 1, 0),
        (&[wide.as_str()], 2, 1),
        (&[expanding.as_str()], 3, 2),
        (&[boundary.as_str()], 4, 3),
    ]);
    let shuffle = SmartShuffle::from_catalog(
        &catalog,
        &[Some(0), Some(1), Some(2), Some(3), Some(4), Some(0)],
    );
    assert_eq!(shuffle.transition_distance(0, 1), None);
    assert_eq!(shuffle.transition_distance(2, 1), None);
    assert_eq!(shuffle.transition_distance(3, 1), None);
    assert_eq!(shuffle.transition_distance(4, 4), Some(0));
    let planned = shuffle.plan(42, None, None);
    assert_eq!(planned.report.unknown_tracks, 4);
    assert_eq!(planned.report.limited_tracks, 4);
    let mut indices = planned.order;
    indices.sort_unstable();
    assert_eq!(indices, [0, 1, 2, 3, 4, 5]);
}

#[test]
fn large_selection_keeps_all_occurrences_and_remainder_can_use_sparse_indices() {
    let styles = [
        "classical",
        "third stream",
        "jazz",
        "blues",
        "rhythm and blues",
        "soul",
        "funk",
        "hip hop",
        "rap",
        "trip hop",
        "electronic",
        "ambient",
        "rock",
        "hard rock",
        "heavy metal",
        "folk",
        "country",
        "pop",
        "reggae",
        "dub",
    ];
    let tags: Vec<_> = styles.iter().map(|s| vec![*s]).collect();
    let specs: Vec<_> = (0..400)
        .map(|id| (tags[id % tags.len()].as_slice(), (id % 100) as Id, id as Id))
        .collect();
    let catalog = fixture(&specs);
    let tracks: Vec<_> = (0..10_000).map(|id| Some((id % 400) as Id)).collect();
    let start = std::time::Instant::now();
    let shuffle = SmartShuffle::from_catalog(&catalog, &tracks);
    let prepared = start.elapsed();
    let start = std::time::Instant::now();
    let planned = shuffle.plan(42, None, None);
    let planning = start.elapsed();
    eprintln!("smart shuffle 10000: preparation={prepared:?} planning={planning:?}");
    let mut sorted = planned.order;
    sorted.sort_unstable();
    assert_eq!(sorted, (0..10_000).collect::<Vec<_>>());
    let remainder = shuffle
        .plan_remaining(2, &[9_999, 400, 76], Some(20))
        .unwrap();
    let mut sorted = remainder.order;
    sorted.sort_unstable();
    assert_eq!(sorted, [76, 400, 9_999]);
}
