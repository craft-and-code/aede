use super::*;
use crate::audit::integrity::Verdict;
use crate::model::{AudioFile, IntegrityRecord};

fn catalog() -> Catalog {
    Catalog {
        files: vec![AudioFile {
            id: 0,
            path: "/music/track.flac".into(),
            size: 42,
            mtime: 7,
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn verdict() -> IntegrityRecord {
    IntegrityRecord {
        verdict: Verdict::Intact,
        method: "flac-frame-crc".into(),
        checked_at: 10,
    }
}

#[test]
fn results_live_outside_catalog_and_match_bytes() {
    let mut original = catalog();
    original.files[0].integrity = Some(verdict());
    original.files[0].fingerprint = Some(Fingerprint {
        data: "abc".into(),
        seconds: 12,
    });
    let stored = Conclusions::from_catalog(&original);
    let encoded = to_json(&stored);
    let read = from_json(&encoded).unwrap();
    assert_eq!(read.files.len(), 1);
    assert!(
        !crate::store::to_json(&original)
            .to_string_compact()
            .contains("integrity")
    );
    let mut fresh = catalog();
    read.attach(&mut fresh);
    assert_eq!(fresh.files[0].integrity, Some(verdict()));
    assert_eq!(fresh.files[0].fingerprint.as_ref().unwrap().data, "abc");
    fresh.files[0].mtime += 1;
    read.attach(&mut fresh);
    assert!(fresh.files[0].integrity.is_none());
    assert!(fresh.files[0].fingerprint.is_none());
}

#[test]
fn absent_files_wait_and_analyses_survive_a_full_scan() {
    let mut original = catalog();
    original.files[0].integrity = Some(verdict());
    original.analyses.push(FileAnalysis {
        path: "/music/missing.flac".into(),
        ..Default::default()
    });
    let mut gathered = Conclusions::from_catalog(&original);
    gathered.update_from_catalog(&Catalog {
        analyses: original.analyses.clone(),
        ..Default::default()
    });
    assert!(gathered.files.contains_key("/music/track.flac"));
    let mut returned = catalog();
    gathered.attach(&mut returned);
    assert_eq!(returned.files[0].integrity, Some(verdict()));
    assert_eq!(returned.analyses.len(), 1);
}

#[test]
fn unknown_format_is_refused_independently() {
    let mut document = to_json(&Conclusions::default());
    document.set("format_version", 99u32.into());
    assert!(matches!(
        from_json(&document),
        Err(StoreError::ConclusionsVersion { .. })
    ));
}

#[test]
fn playback_loudness_roundtrips_and_survives_catalog_updates() {
    let mut stored = Conclusions::default();
    let imported = FileAnalysis {
        path: "/music/one.wav".into(),
        source: "flaccompagnon".into(),
        source_version: 8,
        ..Default::default()
    };
    stored.analyses.push(imported.clone());
    let file = ProgrammeFile {
        path: "/music/one.wav".into(),
        size: 100,
        mtime: 4,
    };
    let measurement = Measurement {
        integrated_lufs: -19.0,
        true_peak: Some(0.8),
    };
    stored.loudness_tracks.insert(
        file.path.clone(),
        CachedTrack {
            size: file.size,
            mtime: file.mtime,
            measurement: Some(measurement),
        },
    );
    stored.loudness_programmes.push(CachedProgramme {
        files: vec![file],
        measurement: Some(measurement),
    });
    let mut restored = from_json(&to_json(&stored)).unwrap();
    let mut updated = catalog();
    updated.analyses.push(imported.clone());
    restored.update_from_catalog(&updated);
    assert_eq!(restored.loudness_tracks.len(), 1);
    assert_eq!(restored.loudness_programmes.len(), 1);
    assert_eq!(
        restored.loudness_programmes[0].measurement,
        Some(measurement)
    );
    for version in [1u32, 2, 3] {
        let mut old_method = to_json(&restored);
        old_method.set("loudness_method_version", version.into());
        let expired = from_json(&old_method).unwrap();
        assert!(expired.loudness_tracks.is_empty());
        assert!(expired.loudness_programmes.is_empty());
        assert_eq!(expired.analyses, vec![imported.clone()]);
    }
}
