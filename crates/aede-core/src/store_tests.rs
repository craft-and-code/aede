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
fn windows_catalog_paths_round_trip_without_normalization() {
    for root in [
        r"C:\Music",
        r"\\?\C:\Music",
        r"\\nas\music",
        r"\\?\UNC\nas\music",
    ] {
        let mut original = example_catalog();
        original.roots = vec![root.into()];
        original.excluded = vec![format!("{root}\\Skip")];
        original.files[0].path = format!("{root}\\Kind of Blue\\01.flac");
        original.files[0].lyrics_path = Some(format!("{root}\\Kind of Blue\\01.lrc"));
        original.releases[0].folder = format!("{root}\\Kind of Blue");
        original.releases[0].cover_path = Some(format!("{root}\\Kind of Blue\\cover.jpg"));
        let decoded = from_json(&to_json(&original)).unwrap();
        assert_eq!(decoded.roots, original.roots);
        assert_eq!(decoded.excluded, original.excluded);
        assert_eq!(decoded.files[0].path, original.files[0].path);
        assert_eq!(decoded.files[0].lyrics_path, original.files[0].lyrics_path);
        assert_eq!(decoded.releases[0].folder, original.releases[0].folder);
        assert_eq!(
            decoded.releases[0].cover_path,
            original.releases[0].cover_path
        );
    }
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
fn sidecar_write_is_new_only_and_publishes_complete_content() {
    let folder = std::env::temp_dir().join(format!(
        "aede_atomic_sidecar_{}_{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    std::fs::create_dir_all(&folder).expect("sidecar test folder");
    let path = folder.join("cover.jpg");
    assert!(write_new_atomic(&path, b"complete image").expect("write sidecar"));
    assert_eq!(std::fs::read(&path).expect("sidecar"), b"complete image");
    assert!(!write_new_atomic(&path, b"replacement").expect("preserve sidecar"));
    assert_eq!(std::fs::read(&path).expect("sidecar"), b"complete image");
    std::fs::remove_file(path).expect("remove sidecar");
    std::fs::remove_dir(folder).expect("remove sidecar folder");
}

#[cfg(unix)]
#[test]
fn sidecar_write_preserves_a_dangling_destination_symlink() {
    let sandbox = StoreSandbox::new();
    let path = sandbox.0.join("cover.jpg");
    std::os::unix::fs::symlink("missing.jpg", &path).unwrap();
    assert!(!write_new_atomic(&path, b"replacement").unwrap());
    assert_eq!(std::fs::read_link(&path).unwrap(), Path::new("missing.jpg"));
    assert_eq!(std::fs::read_dir(&sandbox.0).unwrap().count(), 1);
}

struct StoreSandbox(PathBuf);

impl StoreSandbox {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aede_store_safety_{}_{nonce}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for StoreSandbox {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn legacy_catalog_with_conclusions() -> Json {
    let mut legacy = to_json(&example_catalog());
    let mut file = legacy.get("file").unwrap().as_arr().unwrap()[0].clone();
    let mut verdict = Json::obj();
    verdict.set("state", "intact".into());
    verdict.set("method", "flac-frame-crc".into());
    verdict.set("checked_at", 1_700_000_500u64.into());
    file.set("integrity", verdict);
    legacy.set("file", Json::Arr(vec![file]));
    legacy
}

#[test]
fn concurrent_legacy_reads_do_not_write_or_wait_for_the_writer_lock() {
    let sandbox = StoreSandbox::new();
    let path = catalog_path(&sandbox.0);
    let original = legacy_catalog_with_conclusions().to_string_compact();
    std::fs::write(&path, &original).unwrap();
    let _writer = crate::store_lock::StoreLock::acquire(&sandbox.0).unwrap();
    std::thread::scope(|scope| {
        let readers: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| load(&path).unwrap().unwrap()))
            .collect();
        for reader in readers {
            assert!(reader.join().unwrap().files[0].integrity.is_some());
        }
    });
    assert!(!conclusions::conclusions_path(&sandbox.0).exists());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn a_new_sidecar_cannot_replace_a_file_created_before_publication() {
    let sandbox = StoreSandbox::new();
    let path = sandbox.0.join("cover.jpg");
    let temporary = sandbox.0.join(".complete-download");
    std::fs::write(&temporary, b"downloaded image").unwrap();
    // A separate creator wins after the download, before publication.
    std::fs::write(&path, b"user image").unwrap();
    assert!(!publish_new(&temporary, &path).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), b"user image");
    assert!(!temporary.exists());
}

#[test]
fn competing_sidecar_writers_publish_exactly_one_complete_file() {
    let sandbox = StoreSandbox::new();
    let path = sandbox.0.join("cover.jpg");
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        let writers: Vec<_> = (0..8)
            .map(|id| {
                let path = &path;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    write_new_atomic(path, &[id; 4096]).unwrap()
                })
            })
            .collect();
        let published = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .filter(|published| *published)
            .count();
        assert_eq!(published, 1);
    });
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 4096);
    assert!(bytes.iter().all(|byte| *byte == bytes[0]));
    assert_eq!(std::fs::read_dir(&sandbox.0).unwrap().count(), 1);
}

#[test]
fn first_save_preserves_legacy_conclusions_for_files_absent_from_the_new_scan() {
    let sandbox = StoreSandbox::new();
    let path = catalog_path(&sandbox.0);
    std::fs::write(&path, legacy_catalog_with_conclusions().to_string_compact()).unwrap();
    let _writer = crate::store_lock::StoreLock::acquire(&sandbox.0).unwrap();
    save(&Catalog::default(), &path).unwrap();
    let gathered = conclusions::load(&conclusions::conclusions_path(&sandbox.0))
        .unwrap()
        .unwrap();
    assert_eq!(gathered.files.len(), 1);
    assert!(gathered.files.values().next().unwrap().integrity.is_some());
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
        !conclusions_path.exists(),
        "reading must not migrate on disk"
    );
    assert!(migrated.files[0].integrity.is_some());
    assert_eq!(migrated.analyses.len(), 1);
    save(&migrated, &path).unwrap();
    assert!(conclusions_path.exists(), "expensive data survives rewrite");
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
fn an_unreadable_conclusions_store_does_not_rewrite_a_legacy_catalog() {
    let mut legacy = to_json(&example_catalog());
    let mut file = legacy.get("file").unwrap().as_arr().unwrap()[0].clone();
    let mut verdict = Json::obj();
    verdict.set("state", "intact".into());
    verdict.set("method", "flac-frame-crc".into());
    file.set("integrity", verdict);
    legacy.set("file", Json::Arr(vec![file]));
    let original = legacy.to_string_compact();

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "aede_failed_legacy_migration_{}_{}",
        std::process::id(),
        nonce
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = catalog_path(&dir);
    let conclusions_path = crate::conclusions::conclusions_path(&dir);
    std::fs::write(&path, &original).unwrap();
    std::fs::write(&conclusions_path, "not-json").unwrap();

    assert!(load(&path).is_err());
    assert!(save(&example_catalog(), &path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert_eq!(
        std::fs::read_to_string(&conclusions_path).unwrap(),
        "not-json"
    );

    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(conclusions_path).unwrap();
    std::fs::remove_dir(dir).unwrap();
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
