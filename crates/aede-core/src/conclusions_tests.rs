use super::*;

#[test]
fn malformed_present_tables_are_not_read_as_empty_conclusions() {
    for table in ["file", "analysis", "loudness_track", "loudness_programme"] {
        let mut value = to_json(&Conclusions::default());
        value.set(table, Json::obj());
        assert!(
            from_json(&value).is_err(),
            "{table} must be an array when present"
        );
    }
}

#[test]
fn legacy_decoder_loudness_is_expired_without_erasing_attributed_reports() {
    let mut stored = Conclusions::default();
    stored.loudness_tracks.insert(
        "/music/float.wv".into(),
        CachedTrack {
            size: 42,
            mtime: 7,
            mtime_subseconds: Some(0),
            measurement: None,
        },
    );
    stored.loudness_programmes.push(CachedProgramme {
        files: vec![ProgrammeFile {
            path: "/music/float.wv".into(),
            size: 42,
            mtime: 7,
            mtime_subseconds: Some(0),
        }],
        measurement: None,
    });
    let imported = FileAnalysis {
        path: "/music/float.wv".into(),
        source: "flaccompagnon".into(),
        integrated_lufs: Some(-16.),
        ..Default::default()
    };
    stored.analyses.push(imported.clone());
    let mut legacy = to_json(&stored);
    if let Json::Obj(fields) = &mut legacy {
        fields.remove("loudness_decoder_revision");
    }
    let restored = from_json(&legacy).unwrap();
    assert!(restored.loudness_tracks.is_empty());
    assert!(restored.loudness_programmes.is_empty());
    assert_eq!(restored.analyses, vec![imported]);
    assert_eq!(stored.loudness_tracks.len(), 1);
}

#[test]
fn malformed_analysis_rows_are_refused_without_rewriting_either_store() {
    #[cfg(unix)]
    let directory = crate::store_lock::test_support::Directory::new("analysis_rows");
    #[cfg(unix)]
    let catalog_path = store::catalog_path(directory.path());
    #[cfg(unix)]
    let held_path = conclusions_path(directory.path());
    let catalog = Catalog::default();
    #[cfg(unix)]
    let catalog_bytes = store::to_json(&catalog).to_string_compact();
    #[cfg(unix)]
    std::fs::write(&catalog_path, &catalog_bytes).unwrap();
    for row in [
        "null",
        "{}",
        r#"{"path":12}"#,
        r#"{"path":"/music/a.flac","real_bit_depth":65552}"#,
        r#"{"path":"/music/a.flac","size_bytes":"42"}"#,
        r#"{"path":"/music/a.flac","fake_stereo":"false"}"#,
        r#"{"path":"/music/a.flac","source_data":[]}"#,
        r#"{"path":"/music/a.flac","summary":12}"#,
        r#"{"path":"/music/a.flac","result_at_ns":1,"result_at_ns_exact":"2"}"#,
        r#"{"path":"/music/a.flac","result_at_ns":1,"result_at_ns_exact":"+1"}"#,
        r#"{"path":"/music/a.flac","result_at_ns":1,"result_at_ns_exact":null}"#,
        r#"{"path":"/music/a.flac","result_at_ns":1,"result_at_ns_exact":"18446744073709551616"}"#,
    ] {
        let mut document = to_json(&Conclusions::default());
        document.set("analysis", Json::Arr(vec![json::parse(row).unwrap()]));
        assert!(from_json(&document).is_err(), "invalid analysis: {row}");
        #[cfg(unix)]
        {
            let bytes = document.to_string_compact();
            std::fs::write(&held_path, &bytes).unwrap();
            assert!(store::save(&catalog, &catalog_path).is_err());
            assert_eq!(std::fs::read_to_string(&held_path).unwrap(), bytes);
            assert_eq!(
                std::fs::read_to_string(&catalog_path).unwrap(),
                catalog_bytes
            );
        }
        let mut legacy = store::to_json(&catalog);
        legacy.set("analysis", document.get("analysis").unwrap().clone());
        assert!(
            store::from_json(&legacy).is_err(),
            "invalid legacy analysis: {row}"
        );
    }
}

#[cfg(unix)]
#[test]
fn conclusions_save_cannot_follow_a_preexisting_temporary_symlink() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sandbox = std::env::temp_dir().join(format!(
        "aede_conclusions_safety_{}_{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&sandbox).unwrap();
    let path = conclusions_path(&sandbox);
    let protected = sandbox.join("protected.txt");
    std::fs::write(&protected, b"keep these bytes").unwrap();
    let temporary = path.with_extension("json.tmp");
    std::os::unix::fs::symlink(&protected, &temporary).unwrap();
    save(&Conclusions::default(), &path).unwrap();
    assert_eq!(std::fs::read(&protected).unwrap(), b"keep these bytes");
    assert!(
        !std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(load(&path).unwrap().is_some());
    assert_eq!(std::fs::read_link(temporary).unwrap(), protected);
    std::fs::remove_dir_all(sandbox).unwrap();
}

#[test]
fn persisted_analysis_retains_measurements_and_legacy_absence() {
    let analysis = FileAnalysis {
        path: "/music/one.flac".into(),
        source: "flaccompagnon".into(),
        source_version: 8,
        imported_at: 10,
        result_at_ns: 9_007_199_254_740_993,
        size_bytes: 42,
        modified_unix: 7,
        file_md5: Some("0123456789abcdef0123456789abcdef".into()),
        source_data: Some(json::parse(r#"{"future_measurement":[1,2,3]}"#).unwrap()),
        md5_state: Some("Match".into()),
        md5_detail: Some("verified".into()),
        real_bit_depth: Some(24),
        requant_rate: Some(0.0),
        fake_stereo: Some(false),
        ext_mismatch: Some(false),
        transcoding: Some("none".into()),
        upscaling: Some(false),
        upsampling: Some(false),
        summary: Some("lossless".into()),
        detail: Some("retained source wording".into()),
        cutoff_hz: Some(20_000.0),
        cutoff_ratio: Some(0.9),
        dr_db: Some(12.0),
        peak_dbfs: Some(-1.0),
        true_peak_dbtp: Some(-0.5),
        integrated_lufs: Some(-14.0),
        clipped_samples: Some(0),
        clip_events: Some(0),
        clipped: Some(false),
        error: None,
    };
    let stored = Conclusions {
        analyses: vec![analysis.clone()],
        ..Default::default()
    };
    let serialized = to_json(&stored).to_string_compact();
    assert_eq!(
        from_json(&json::parse(&serialized).unwrap())
            .unwrap()
            .analyses,
        vec![analysis]
    );
    let legacy = store::analysis_from_json(
        &json::parse(r#"{"path":"/music/old.flac","fake_stereo":null}"#).unwrap(),
    )
    .unwrap();
    assert!(legacy.fake_stereo.is_none());
    assert!(legacy.real_bit_depth.is_none());
    assert_eq!(legacy.result_at_ns, 0);
    for date in [0, 1, (1 << 53) - 1, (1 << 53) + 1, u64::MAX] {
        let original = FileAnalysis {
            path: "/music/date.flac".into(),
            result_at_ns: date,
            ..Default::default()
        };
        let bytes = store::analysis_to_json(&original).to_string_compact();
        assert_eq!(
            store::analysis_from_json(&json::parse(&bytes).unwrap())
                .unwrap()
                .result_at_ns,
            date
        );
    }
}
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
fn fallback_zero_timestamps_cannot_establish_unchanged_audio() {
    let mut original = catalog();
    original.files[0].mtime = 0;
    original.files[0].integrity = Some(verdict());
    let stored = Conclusions::from_catalog(&original);
    let before = to_json(&stored);
    let mut fresh = original.clone();
    stored.attach(&mut fresh);
    assert!(fresh.files[0].integrity.is_none());
    assert_eq!(
        to_json(&stored),
        before,
        "an uncertain read does not alter held evidence"
    );

    original
        .file_mtime_subseconds
        .insert(original.files[0].path.clone(), 0);
    let precise = Conclusions::from_catalog(&original);
    precise.attach(&mut original);
    assert_eq!(
        original.files[0].integrity,
        Some(verdict()),
        "an exact epoch timestamp remains usable"
    );
}

#[test]
fn subsecond_file_identity_roundtrips_and_expires_same_second_results() {
    let mut original = catalog();
    original
        .file_mtime_subseconds
        .insert(original.files[0].path.clone(), 123_456_789);
    original.files[0].integrity = Some(verdict());
    let stored = from_json(&to_json(&Conclusions::from_catalog(&original))).unwrap();
    let mut fresh = crate::store::from_json(&crate::store::to_json(&original)).unwrap();
    stored.attach(&mut fresh);
    assert_eq!(fresh.files[0].integrity, Some(verdict()));
    fresh
        .file_mtime_subseconds
        .insert(fresh.files[0].path.clone(), 123_456_790);
    stored.attach(&mut fresh);
    assert!(fresh.files[0].integrity.is_none());
    fresh.file_mtime_subseconds.clear();
    stored.attach(&mut fresh);
    assert!(
        fresh.files[0].integrity.is_none(),
        "unknown precision cannot establish the same bytes"
    );
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
        mtime_subseconds: Some(123_456_789),
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
            mtime_subseconds: file.mtime_subseconds,
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
    assert_eq!(
        restored.loudness_programmes[0].files[0].mtime_subseconds,
        Some(123_456_789)
    );
    assert_eq!(
        restored
            .loudness_tracks
            .values()
            .next()
            .unwrap()
            .mtime_subseconds,
        Some(123_456_789)
    );
    for version in [1u32, 2, 3, 4] {
        let mut old_method = to_json(&restored);
        old_method.set("loudness_method_version", version.into());
        let expired = from_json(&old_method).unwrap();
        assert!(expired.loudness_tracks.is_empty());
        assert!(expired.loudness_programmes.is_empty());
        assert_eq!(expired.analyses, vec![imported.clone()]);
    }
}

#[test]
fn playback_loudness_refuses_malformed_optional_precision_without_discarding_legacy_rows() {
    let mut stored = Conclusions::default();
    stored.loudness_tracks.insert(
        "/music/one.wav".into(),
        CachedTrack {
            size: 100,
            mtime: 4,
            mtime_subseconds: None,
            measurement: None,
        },
    );
    stored.loudness_programmes.push(CachedProgramme {
        files: vec![ProgrammeFile {
            path: "/music/one.wav".into(),
            size: 100,
            mtime: 4,
            mtime_subseconds: None,
        }],
        measurement: None,
    });
    let serialized = to_json(&stored);
    let legacy = from_json(&serialized).expect("optional precision is backward-compatible");
    assert_eq!(legacy.loudness_tracks, stored.loudness_tracks);
    assert_eq!(legacy.loudness_programmes, stored.loudness_programmes);
    let current = ProgrammeFile {
        mtime_subseconds: Some(123),
        ..stored.loudness_programmes[0].files[0].clone()
    };
    assert!(!legacy.loudness_tracks["/music/one.wav"].matches(&current));
    for fraction in [
        Json::Null,
        (-1_i64).into(),
        1_000_000_000_u64.into(),
        "invalid".into(),
    ] {
        for programme in [false, true] {
            let mut row = if programme {
                serialized
                    .get("loudness_programme")
                    .unwrap()
                    .as_arr()
                    .unwrap()[0]
                    .get("file")
                    .unwrap()
                    .as_arr()
                    .unwrap()[0]
                    .clone()
            } else {
                serialized.get("loudness_track").unwrap().as_arr().unwrap()[0].clone()
            };
            row.set("mtime_subseconds", fraction.clone());
            let mut malformed = serialized.clone();
            if programme {
                let mut programme = serialized
                    .get("loudness_programme")
                    .unwrap()
                    .as_arr()
                    .unwrap()[0]
                    .clone();
                programme.set("file", Json::Arr(vec![row]));
                malformed.set("loudness_programme", Json::Arr(vec![programme]));
            } else {
                malformed.set("loudness_track", Json::Arr(vec![row]));
            }
            assert!(
                from_json(&malformed).is_err(),
                "invalid precision must be an explicit error"
            );
        }
    }
}
