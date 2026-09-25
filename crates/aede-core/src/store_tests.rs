use super::*;
use crate::model::{self, ScannedFile};
use crate::tags::RawTags;

fn example_catalog() -> Catalog {
    let mut tags = RawTags::default();
    tags.insert("title", "So What");
    tags.insert("artist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("date", "1959");
    tags.insert("genre", "Jazz");
    tags.insert("label", "Columbia");
    tags.properties.codec = "flac".into();
    tags.properties.container = "flac".into();
    tags.properties.sample_rate = Some(44_100);
    tags.properties.bit_depth = Some(16);
    tags.properties.channels = Some(2);
    tags.properties.duration_ms = Some(545_000);
    tags.properties.lossless = true;

    model::build(
        vec![ScannedFile {
            path: "/music/Miles Davis/Kind of Blue/01 So What.flac".into(),
            size: 42_000_000,
            mtime: 1_700_000_000,
            tags,
            folder_cover: Some("/music/Miles Davis/Kind of Blue/cover.jpg".into()),
            sidecar: Some("/music/Miles Davis/Kind of Blue/01 So What.lrc".into()),
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".into()],
        1_700_000_100,
        &[],
    )
}

#[test]
fn full_round_trip() {
    let original = example_catalog();
    let encoded = to_json(&original);
    let decoded = from_json(&encoded).expect("read back");

    assert_eq!(decoded.roots, original.roots);
    assert_eq!(decoded.scanned_at, original.scanned_at);
    assert_eq!(decoded.files.len(), original.files.len());
    assert_eq!(decoded.artists.len(), original.artists.len());
    assert_eq!(decoded.tracks.len(), original.tracks.len());
    assert_eq!(decoded.credits.len(), original.credits.len());

    let f = &decoded.files[0];
    assert_eq!(f.path, original.files[0].path);
    assert_eq!(f.properties.sample_rate, Some(44_100));
    assert_eq!(f.properties.bit_depth, Some(16));
    assert!(f.properties.lossless);
    assert_eq!(f.first_tag("artist"), Some("Miles Davis"));

    let (albums, _) = decoded.find_releases("Kind of Blue");
    let album = albums.first().expect("album");
    assert_eq!(album.year, Some(1959));
    assert!(album.cover_path.is_some());
    // The sidecar is a path the walk found, not a fact the file states, so
    // nothing recomputes it on load: it has to survive the round trip.
    assert_eq!(
        decoded.files[0].lyrics_path.as_deref(),
        Some("/music/Miles Davis/Kind of Blue/01 So What.lrc")
    );
}

#[test]
fn rich_local_credit_details_survive_the_round_trip() {
    let mut original = example_catalog();
    let credit = original.credits.first_mut().expect("one local credit");
    credit.credited_as = Some("Miles".into());
    credit.attributes = vec![model::CreditAttribute {
        id: Some("instrument-type".into()),
        name: "instrument".into(),
        value: Some("electric piano".into()),
        credited_as: Some("Fender Rhodes".into()),
    }];
    credit.began = Some("1959-03-02".into());
    credit.ended = Some("1959-04-22".into());
    credit.order = Some(1);
    credit.source = "booklet".into();
    credit.source_id = Some("credit-row-1".into());

    let decoded = from_json(&to_json(&original)).expect("read back");
    let credit = decoded.credits.first().expect("one decoded credit");
    assert_eq!(credit.credited_as.as_deref(), Some("Miles"));
    assert_eq!(credit.attributes, original.credits[0].attributes);
    assert_eq!(credit.began.as_deref(), Some("1959-03-02"));
    assert_eq!(credit.ended.as_deref(), Some("1959-04-22"));
    assert_eq!(credit.order, Some(1));
    assert_eq!(credit.source, "booklet");
    assert_eq!(credit.source_id.as_deref(), Some("credit-row-1"));
}

#[test]
fn round_trip_through_json_text() {
    let original = example_catalog();
    let text = to_json(&original).to_string_compact();
    let read_back = from_json(&json::parse(&text).unwrap()).unwrap();
    assert_eq!(read_back.tracks[0].title, "So What");
}

#[test]
fn works_and_their_recording_links_survive_the_round_trip() {
    let mut original = example_catalog();
    original.works.push(Work {
        id: 0,
        title: "So What".into(),
        key: "so what".into(),
        mbid: "composition-so-what".into(),
        recording_ids: vec![0],
    });
    original.recordings[0].work_ids.push(0);

    let decoded = from_json(&to_json(&original)).expect("read back");
    assert_eq!(decoded.works.len(), 1);
    assert_eq!(decoded.works[0].mbid, "composition-so-what");
    assert_eq!(decoded.works[0].recording_ids, vec![0]);
    assert_eq!(decoded.recordings[0].work_ids, vec![0]);
}

#[test]
fn a_catalog_written_before_recordings_gains_them_without_title_matching() {
    let original = example_catalog();
    let mut encoded = to_json(&original);
    // An empty table is how a pre-canonical document is represented in this
    // fixture; old documents simply have no `recording` key, which the
    // reader treats the same way.
    encoded.set("recording", Json::Arr(Vec::new()));

    let migrated = from_json(&encoded).expect("legacy catalog migrates");
    assert_eq!(migrated.recordings.len(), migrated.tracks.len());
    let placement = &migrated.tracks[0];
    let recording = migrated
        .recording(placement.recording_id)
        .expect("recording");
    assert_eq!(recording.track_ids, vec![placement.id]);
    assert_eq!(recording.title, placement.title);
}

#[test]
fn an_integrity_verdict_survives_the_round_trip() {
    let mut original = example_catalog();
    original.files[0].integrity = Some(IntegrityRecord {
        verdict: audit::integrity::Verdict::Damaged {
            detail: "frame 12: audio checksum mismatch".into(),
        },
        method: audit::integrity::FLAC_METHOD.into(),
        checked_at: 1_700_000_500,
    });
    let dir = std::env::temp_dir().join(format!("aede_verdict_store_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = catalog_path(&dir);
    save(&original, &path).unwrap();
    let read_back = load(&path).unwrap().unwrap();
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(crate::conclusions::conclusions_path(&dir));
    let record = read_back.files[0]
        .integrity
        .as_ref()
        .expect("the verdict is kept");
    assert_eq!(record.method, audit::integrity::FLAC_METHOD);
    assert_eq!(record.checked_at, 1_700_000_500);
    match &record.verdict {
        audit::integrity::Verdict::Damaged { detail } => {
            assert!(detail.contains("frame 12"), "detail kept: {detail}")
        }
        other => panic!("wrong verdict: {other:?}"),
    }
}

#[test]
fn legacy_conclusions_are_migrated_before_catalog_is_rewritten() {
    let original = example_catalog();
    let mut legacy = to_json(&original);
    let mut file = legacy.get("file").unwrap().as_arr().unwrap()[0].clone();
    let mut verdict = Json::obj();
    verdict.set("state", "intact".into());
    verdict.set("method", "flac-frame-crc".into());
    verdict.set("checked_at", 1_700_000_500u64.into());
    file.set("integrity", verdict);
    legacy.set("file", Json::Arr(vec![file]));
    let mut analysis = Json::obj();
    analysis.set("path", "/music/waiting.flac".into());
    analysis.set("source", "flaccompagnon".into());
    legacy.set("analysis", Json::Arr(vec![analysis]));

    let dir = std::env::temp_dir().join(format!("aede_legacy_migration_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = catalog_path(&dir);
    std::fs::write(&path, legacy.to_string_compact()).unwrap();
    let migrated = load(&path).unwrap().unwrap();
    let conclusions_path = crate::conclusions::conclusions_path(&dir);
    assert!(
        conclusions_path.exists(),
        "expensive data is durable before rewrite"
    );
    assert!(migrated.files[0].integrity.is_some());
    assert_eq!(migrated.analyses.len(), 1);
    save(&migrated, &path).unwrap();
    let compact = std::fs::read_to_string(&path).unwrap();
    assert!(!compact.contains("integrity"));
    assert!(!compact.contains("\"analysis\""));
    let reloaded = load(&path).unwrap().unwrap();
    assert!(reloaded.files[0].integrity.is_some());
    assert_eq!(reloaded.analyses.len(), 1);
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(conclusions_path);
}

#[test]
fn a_file_never_checked_stores_nothing() {
    // Absent, not "false": a file that was never looked at must not be
    // confused with one that carries no checksum.
    let text = to_json(&example_catalog()).to_string_compact();
    assert!(!text.contains("integrity"), "nothing is written");
    let read_back = from_json(&json::parse(&text).unwrap()).unwrap();
    assert!(read_back.files[0].integrity.is_none());
}

#[test]
fn relations_inferred_under_older_rules_are_rebuilt_on_load() {
    // Upgrading Aède must not require a rescan to get the benefit of a new
    // inference, and must not cost the integrity verdicts a rebuilt
    // catalog would lose.
    // Two performers on one track, which is what a relation is made of.
    let mut tags = RawTags::default();
    tags.insert("title", "Sous le vent");
    tags.insert("artist", "Garou feat. Céline Dion");
    tags.insert("album", "Duos");
    tags.properties.duration_ms = Some(200_000);
    let original = model::build(
        vec![ScannedFile {
            path: "/music/Duos/01.flac".into(),
            size: 1_000,
            mtime: 0,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".into()],
        0,
        &[],
    );
    assert!(!original.relations.is_empty(), "the fixture has relations");

    let mut encoded = to_json(&original);
    encoded.set("relation_rules", 0u32.into());
    encoded.set("relation", Json::Arr(Vec::new()));

    let read_back = from_json(&encoded).expect("an older catalog still loads");
    assert_eq!(
        read_back.relations.len(),
        original.relations.len(),
        "the relations were recomputed from the credits"
    );

    // A catalog written by this version is left exactly as it is.
    let current = to_json(&read_back);
    assert_eq!(
        current.field_u32("relation_rules"),
        Some(model::RELATION_RULES)
    );
}

#[test]
fn incompatible_version_is_refused() {
    let mut encoded = to_json(&example_catalog());
    encoded.set("format_version", 999u32.into());
    assert!(matches!(
        from_json(&encoded),
        Err(StoreError::Version { .. })
    ));
}

#[test]
fn inconsistent_identifiers_are_refused() {
    let mut encoded = to_json(&example_catalog());
    if let Some(Json::Arr(files)) = encoded.get("file").cloned().as_mut() {
        files[0].set("id", 7u32.into());
        encoded.set("file", Json::Arr(files.clone()));
    }
    assert!(matches!(from_json(&encoded), Err(StoreError::Invalid(_))));
}

#[test]
fn atomic_save() {
    let folder = std::env::temp_dir().join("aede_test_store");
    let _ = std::fs::remove_dir_all(&folder);
    let path = catalog_path(&folder);

    assert!(load(&path).unwrap().is_none(), "no catalog to begin with");
    save(&example_catalog(), &path).expect("write");
    let read_back = load(&path).expect("read").expect("present");
    assert_eq!(read_back.tracks.len(), 1);
    // No temporary file must remain.
    assert!(!path.with_extension("json.tmp").exists());
    let _ = std::fs::remove_dir_all(&folder);
}
