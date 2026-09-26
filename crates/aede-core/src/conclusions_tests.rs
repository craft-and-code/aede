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
