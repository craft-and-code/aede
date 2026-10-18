use super::*;

#[test]
fn an_unavailable_album_does_not_block_other_catalogued_folders() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let dir =
        std::env::temp_dir().join(format!("aede-analysis-unavailable-{}", std::process::id()));
    let offline = dir.join("a-offline");
    let online = dir.join("z-online");
    let bytes = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    for album in [&offline, &online] {
        std::fs::create_dir_all(album).unwrap();
        std::fs::write(album.join("track.flac"), bytes).unwrap();
    }
    let (catalog, _) = scan::scan(
        &[offline.clone(), online.clone()],
        None,
        &scan::ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(acoustic::album_files(&catalog, &[]).len(), 2);
    let path = store::catalog_path(&dir);
    store::save(&catalog, &path).unwrap();
    let parked = dir.join("unmounted");
    std::fs::rename(&offline, &parked).unwrap();
    let args = Args::parse([
        "analyze".to_string(),
        "--json".to_string(),
        "--data".to_string(),
        dir.to_string_lossy().into_owned(),
    ]);
    let decoded_online = AtomicUsize::new(0);
    let result = analyze_with_progress(&args, |_, _, selected| {
        if selected.starts_with(&online) {
            decoded_online.fetch_add(1, Ordering::Relaxed);
        }
    });
    assert!(result.is_err(), "the unavailable album is still a failure");
    assert_eq!(decoded_online.load(Ordering::Relaxed), 1);
    let report = fc::report::read_json(&report_path(&online)).unwrap();
    assert_eq!(report.files.len(), 1);
    assert!(report.files[0].error.is_none());
    let saved = store::load(&path).unwrap().unwrap();
    let online_path = online.join("track.flac").to_string_lossy().into_owned();
    assert!(
        saved
            .analyses
            .iter()
            .any(|file| file.path == online_path && file.error.is_none())
    );
    assert_eq!(saved.roots, catalog.roots);
    assert_eq!(saved.files.len(), catalog.files.len());
    assert_eq!(std::fs::read(online.join("track.flac")).unwrap(), bytes);
    assert_eq!(std::fs::read(parked.join("track.flac")).unwrap(), bytes);
    assert!(
        !offline.exists(),
        "analysis must not recreate an unavailable mount"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn forced_artist_reports_cannot_replace_aede_state_files() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let dir = std::env::temp_dir().join(format!("aede-analysis-protected-{}", std::process::id()));
    let album = dir.join("catalog");
    std::fs::create_dir_all(&album).unwrap();
    let audio = album.join("track.flac");
    let bytes = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    std::fs::write(&audio, bytes).unwrap();
    let (catalog, _) = scan::scan(
        std::slice::from_ref(&album),
        None,
        &scan::ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    let path = store::catalog_path(&dir);
    store::save(&catalog, &path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let args = Args::parse([
        "analyze".to_string(),
        "--json-layout=artist".to_string(),
        "--force".to_string(),
        "--data".to_string(),
        dir.to_string_lossy().into_owned(),
    ]);
    let decoded = AtomicUsize::new(0);
    let result = analyze_with_progress(&args, |_, _, _| {
        decoded.fetch_add(1, Ordering::Relaxed);
    });
    assert!(result.is_err());
    assert_eq!(decoded.load(Ordering::Relaxed), 0, "refuse before analysis");
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(store::load(&path).unwrap().is_some());
    let root = dir.canonicalize().unwrap();
    for target in [
        store::catalog_path(&root),
        aede_core::conclusions::conclusions_path(&root),
        aede_core::sources::sources_path(&root),
        aede_core::user::user_path(&root),
        aede_core::accounts::accounts_path(&root),
        aede_core::parental::path(&root),
        root.join("Catalog.json"),
        root.join("catalog").join("..").join("user.json"),
    ] {
        assert!(validate_report_destination(&target, &root).is_err());
    }
    assert!(validate_report_destination(&root.join("album-results.json"), &root).is_ok());
    assert!(validate_report_destination(&album.join("catalog.json"), &root).is_ok());
    assert_eq!(std::fs::read(audio).unwrap(), bytes);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cache_changes_during_another_decode_fail_without_replacing_the_existing_report() {
    let dir =
        std::env::temp_dir().join(format!("aede-analysis-cache-change-{}", std::process::id()));
    let album = dir.join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let cached = album.join("01.flac");
    let pending = album.join("02.flac");
    let bytes = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    std::fs::write(&cached, bytes).unwrap();
    std::fs::write(&pending, bytes).unwrap();
    let (catalog, _) = scan::scan(
        std::slice::from_ref(&album),
        None,
        &scan::ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    let file = fc::analyze_file(&cached, &ScanOptions::default());
    assert!(file.error.is_none());
    let destination = report_path(&album);
    fc::report::write_json(
        &destination,
        &fc::FolderReport {
            root: album.to_string_lossy().into_owned(),
            files: vec![file],
            has_flac: true,
        },
    )
    .unwrap();
    let original = std::fs::read(&destination).unwrap();
    store::save(&catalog, &store::catalog_path(&dir)).unwrap();
    let args = Args::parse([
        "analyze".to_string(),
        "--json".to_string(),
        "--data".to_string(),
        dir.to_string_lossy().into_owned(),
    ]);
    let result = analyze_with_progress(&args, |_, _, selected| {
        assert_eq!(selected, pending.as_path());
        std::fs::remove_file(&cached).unwrap();
    });
    assert!(
        result.is_err(),
        "a selected file must not silently disappear"
    );
    assert_eq!(std::fs::read(&destination).unwrap(), original);
    let saved = store::load(&store::catalog_path(&dir)).unwrap().unwrap();
    assert_eq!(saved.analyses.len(), 1);
    assert_eq!(
        saved.analyses[0].path,
        pending.to_string_lossy().into_owned()
    );
    assert_eq!(std::fs::read(pending).unwrap(), bytes);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_cache_that_becomes_reusable_during_decode_cannot_replace_the_new_measurement() {
    let dir =
        std::env::temp_dir().join(format!("aede-analysis-cache-return-{}", std::process::id()));
    let album = dir.join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let path = album.join("track.flac");
    let bytes = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    std::fs::write(&path, bytes).unwrap();
    let (catalog, _) = scan::scan(
        std::slice::from_ref(&album),
        None,
        &scan::ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    let original_mtime = metadata.modified().unwrap();
    let old = fc::analyze_file(&path, &ScanOptions::default());
    assert!(old.error.is_none());
    let destination = report_path(&album);
    fc::report::write_json(
        &destination,
        &fc::FolderReport {
            root: album.to_string_lossy().into_owned(),
            files: vec![old],
            has_flac: true,
        },
    )
    .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&destination)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(100)),
        )
        .unwrap();
    store::save(&catalog, &store::catalog_path(&dir)).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH))
        .unwrap();
    let args = Args::parse([
        "analyze".to_string(),
        "--json".to_string(),
        "--data".to_string(),
        dir.to_string_lossy().into_owned(),
    ]);
    analyze_with_progress(&args, |_, _, selected| {
        assert_eq!(selected, path.as_path());
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(original_mtime))
            .unwrap();
    })
    .unwrap();
    let report = fc::report::read_json(&destination).unwrap();
    assert_eq!(report.files.len(), 1, "each selected file is emitted once");
    let imported = analysis::read_report(&destination).unwrap();
    let date = imported.files[0].result_at_ns;
    assert!(
        date > 100_000_000_000,
        "the newly decoded result keeps its date"
    );
    let saved = store::load(&store::catalog_path(&dir)).unwrap().unwrap();
    assert_eq!(saved.analyses.len(), 1);
    assert_eq!(saved.analyses[0].dr_db, imported.files[0].dr_db);
    assert_eq!(saved.analyses[0].result_at_ns, date);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn partial_reports_do_not_suppress_a_requested_complete_analysis() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let dir = std::env::temp_dir().join(format!("aede-analysis-partial-{}", std::process::id()));
    let album = dir.join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let path = album.join("track.flac");
    let bytes = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    std::fs::write(&path, bytes).unwrap();
    let (catalog, _) = scan::scan(
        std::slice::from_ref(&album),
        None,
        &scan::ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    let full = fc::analyze_file(&path, &ScanOptions::default());
    assert!(full.error.is_none());
    assert!(
        reusable(&full, &path),
        "legacy complete coverage remains reusable"
    );
    let mut explicit_full = full.clone();
    explicit_full.analyses_run = Some(fc::AnalysisSelection::all().names());
    assert!(reusable(&explicit_full, &path));
    let mut partial = full.clone();
    partial.restrict_to(fc::AnalysisSelection::from_names(["flac-md5"]).unwrap());
    assert!(!reusable(&partial, &path));
    let mut unknown = full;
    unknown.analyses_run = Some(vec!["future-analysis".into()]);
    assert!(!reusable(&unknown, &path));
    let destination = report_path(&album);
    fc::report::write_json(
        &destination,
        &fc::FolderReport {
            root: album.to_string_lossy().into_owned(),
            files: vec![partial],
            has_flac: true,
        },
    )
    .unwrap();
    store::save(&catalog, &store::catalog_path(&dir)).unwrap();
    let args = Args::parse([
        "analyze".to_string(),
        "--json".to_string(),
        "--data".to_string(),
        dir.to_string_lossy().into_owned(),
    ]);
    let decoded = AtomicUsize::new(0);
    analyze_with_progress(&args, |_, _, selected| {
        assert_eq!(selected, path.as_path());
        decoded.fetch_add(1, Ordering::Relaxed);
    })
    .unwrap();
    assert_eq!(decoded.load(Ordering::Relaxed), 1);
    let report = fc::report::read_json(&destination).unwrap();
    assert!(report.files[0].covers_selection(fc::AnalysisSelection::all()));
    assert!(report.files[0].error.is_none());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reused_measurements_keep_unknown_source_fields_in_the_store_and_rewritten_report() {
    use aede_core::json::{self, Json};

    let dir = std::env::temp_dir().join(format!("aede-analysis-source-{}", std::process::id()));
    let album = dir.join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let first = album.join("01.flac");
    let second = album.join("02.flac");
    let bytes = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    std::fs::write(&first, bytes).unwrap();
    std::fs::write(&second, bytes).unwrap();
    let (catalog, _) = scan::scan(
        std::slice::from_ref(&album),
        None,
        &scan::ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    let file = fc::analyze_file(&first, &ScanOptions::default());
    assert!(file.error.is_none());
    let text = fc::report::build_json(&fc::FolderReport {
        root: album.to_string_lossy().into_owned(),
        files: vec![file],
        has_flac: true,
    })
    .unwrap();
    let mut entry = analysis::parse_report(&text)
        .unwrap()
        .files
        .remove(0)
        .source_data
        .unwrap();
    entry.set("future_measurement", "retained source evidence".into());
    if let Json::Obj(fields) = &mut entry {
        fields
            .get_mut("clipping")
            .unwrap()
            .set("future_peak", Json::Num(0.125));
    }
    let mut inner = Json::obj();
    inner.set("root", album.to_string_lossy().into_owned().into());
    inner.set("files", Json::Arr(vec![entry.clone()]));
    inner.set("has_flac", Json::Bool(true));
    let mut wrapper = Json::obj();
    wrapper.set("format", "flaccompagnon-report".into());
    wrapper.set("version", Json::Num(1.0));
    wrapper.set("report", inner);
    let destination = report_path(&album);
    std::fs::write(&destination, wrapper.to_string_pretty()).unwrap();
    let path = store::catalog_path(&dir);
    store::save(&catalog, &path).unwrap();
    let args = Args::parse([
        "analyze".to_string(),
        "--json".to_string(),
        "--data".to_string(),
        dir.to_string_lossy().into_owned(),
    ]);
    analyze_with_progress(&args, |_, _, decoded| assert_eq!(decoded, second.as_path())).unwrap();
    let saved = store::load(&path).unwrap().unwrap();
    let first_path = first.to_string_lossy().into_owned();
    let held = saved
        .analyses
        .iter()
        .find(|a| a.path == first_path)
        .unwrap();
    entry.set(
        analysis::RESULT_DATE_FIELD,
        held.result_at_ns.to_string().into(),
    );
    assert_eq!(held.source_data.as_ref(), Some(&entry));
    let source = analysis::read_report(&destination)
        .unwrap()
        .files
        .remove(0)
        .source_data
        .unwrap();
    assert_eq!(source, entry);
    assert!(fc::report::parse_json(&std::fs::read_to_string(&destination).unwrap()).is_ok());
    analyze_with_progress(&args, |_, _, _| panic!("all tracks are cached")).unwrap();
    let again = store::load(&path).unwrap().unwrap();
    let held = again
        .analyses
        .iter()
        .find(|a| a.path == first_path)
        .unwrap();
    assert_eq!(held.source_data.as_ref(), Some(&entry));
    assert_eq!(std::fs::read(first).unwrap(), bytes);
    assert_eq!(std::fs::read(second).unwrap(), bytes);
    assert_eq!(
        json::parse(&std::fs::read_to_string(destination).unwrap())
            .unwrap()
            .field_u32("version"),
        Some(1)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn human_analysis_output_renders_report_and_path_controls_literally() {
    let folder = std::env::temp_dir().join(format!("aede-analysis-literal-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("track.flac");
    std::fs::write(
        &path,
        include_bytes!("../../../aede-core/tests/fixtures/track.flac"),
    )
    .unwrap();
    let mut file = fc::analyze_file(&path, &ScanOptions::default());
    assert!(file.error.is_none());
    assert!(result_line(&file).contains("track.flac"));
    file.file_name = "track\x1b[2J.flac".into();
    file.detections.detail = "detail\x1b]0;foreign title\x07".into();
    let report = fc::FolderReport {
        root: folder.to_string_lossy().into_owned(),
        files: vec![file.clone()],
        has_flac: true,
    };
    let imported = fc::report::parse_json(&fc::report::build_json(&report).unwrap()).unwrap();
    let line = result_line(&imported.files[0]);
    assert!(!line.contains('\x1b') && !line.contains('\x07'));
    assert!(line.contains("track\\u{1b}[2J.flac"));
    assert!(line.contains("detail\\u{1b}]0;foreign title\\u{7}"));
    file.error = Some("decoder\rerror\x1b[31m".into());
    let line = result_line(&file);
    assert!(!line.contains('\r') && !line.contains('\x1b'));
    assert!(line.contains("decoder\\rerror\\u{1b}[31m"));
    assert_eq!(
        progress_line(1, 2, Path::new("track\x1b[2J.flac")),
        "  ▸ [1/2] Starting track\\u{1b}[2J.flac"
    );
    assert!(imported.files[0].file_name.contains('\x1b'));
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn reused_reports_do_not_replace_newer_measurements_during_publication_or_rescan() {
    for conflicting_cache in [false, true] {
        let dir = std::env::temp_dir().join(format!(
            "aede-analysis-reused-dates-{}-{conflicting_cache}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let album = dir.join("Album");
        std::fs::create_dir_all(&album).unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac");
        let first = album.join("01.flac");
        let second = album.join("02.flac");
        std::fs::copy(&fixture, &first).unwrap();
        std::fs::copy(&fixture, &second).unwrap();
        let (mut catalog, _) = scan::scan(
            std::slice::from_ref(&album),
            None,
            &scan::ScanOptions::default(),
            |_| {},
        )
        .unwrap();
        let mut old = fc::analyze_file(&first, &ScanOptions::default());
        assert!(old.error.is_none());
        old.dr_db = Some(1.0);
        let saved_report = fc::FolderReport {
            root: album.to_string_lossy().into_owned(),
            files: vec![old.clone()],
            has_flac: true,
        };
        let destination = report_path(&album);
        fc::report::write_json(&destination, &saved_report).unwrap();
        let set_date =
            |path: &Path, seconds| {
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(path)
                    .unwrap()
                    .set_times(std::fs::FileTimes::new().set_modified(
                        std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds),
                    ))
                    .unwrap();
            };
        set_date(&destination, 100);
        let mut held = analysis::read_report(&destination).unwrap().files.remove(0);
        held.result_at_ns = 200_000_000_000;
        held.dr_db = Some(9.0);
        held.source_data
            .as_mut()
            .unwrap()
            .set("dr_db", aede_core::json::Json::Num(9.0));
        catalog.analyses.push(held.clone());
        if conflicting_cache {
            let mut stale = saved_report.clone();
            stale.files[0].size_bytes += 1;
            let stale_path = album.join("stale.json");
            fc::report::write_json(&stale_path, &stale).unwrap();
            set_date(&stale_path, 300);
        }
        let path = store::catalog_path(&dir);
        store::save(&catalog, &path).unwrap();
        let args = Args::parse([
            "analyze".to_string(),
            "--json".to_string(),
            "--data".to_string(),
            dir.to_str().unwrap().to_string(),
        ]);
        analyze_with_progress(&args, |_, _, decoded| {
            assert_eq!(
                decoded,
                second.as_path(),
                "the valid cached first track is reused"
            );
        })
        .unwrap();
        let mut saved = store::load(&path).unwrap().unwrap();
        let retained = saved.analyses.iter().find(|a| a.path == held.path).unwrap();
        assert_eq!(retained.dr_db, Some(9.0));
        assert_eq!(retained.result_at_ns, held.result_at_ns);
        analyze_with_progress(&args, |_, _, _| panic!("every result is reusable")).unwrap();
        let again = store::load(&path).unwrap().unwrap();
        assert_eq!(
            again
                .analyses
                .iter()
                .find(|a| a.path == held.path)
                .unwrap()
                .dr_db,
            Some(9.0)
        );
        let rewritten = analysis::read_report(&destination).unwrap();
        let reused = rewritten
            .files
            .iter()
            .find(|a| a.path == held.path)
            .unwrap();
        assert_eq!(reused.result_at_ns, 100_000_000_000);
        assert_eq!(reused.dr_db, Some(1.0));
        let outcome = analysis::merge_into(&mut saved, rewritten.files, now_seconds());
        assert_eq!(outcome.older, 1);
        assert_eq!(
            saved
                .analyses
                .iter()
                .find(|a| a.path == held.path)
                .unwrap()
                .dr_db,
            Some(9.0)
        );
        let (rescanned, _) = scan::scan(
            &[album],
            Some(&saved),
            &scan::ScanOptions::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(
            rescanned
                .analyses
                .iter()
                .find(|a| a.path == held.path)
                .unwrap()
                .dr_db,
            Some(9.0)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn report_path_uses_the_album_folder_name_without_a_tool_suffix() {
    assert_eq!(
        report_path(Path::new("/music/Artist/Album")),
        PathBuf::from("/music/Artist/Album/Album.json")
    );
}

#[test]
fn report_path_keeps_dots_in_an_album_name() {
    assert_eq!(
        report_path(Path::new("/music/Album.v1")),
        PathBuf::from("/music/Album.v1/Album.v1.json")
    );
}

#[test]
fn invalid_analysis_options_fail_before_catalog_loading() {
    for options in [
        vec!["analyze", "--json-layout", "wrong"],
        vec!["analyze", "--json-layout"],
        vec!["analyze", "--force=wrong"],
    ] {
        let args = Args::parse(options.into_iter().map(str::to_string));
        assert!(analyze(&args).is_err());
    }
}

#[test]
fn analysis_publication_preserves_a_writer_that_ran_while_audio_was_decoding() {
    let dir =
        std::env::temp_dir().join(format!("aede-analysis-publication-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let album = dir.join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac");
    std::fs::copy(fixture, album.join("Track.flac")).unwrap();
    let (catalog, _) = scan::scan(&[album], None, &scan::ScanOptions::default(), |_| {}).unwrap();
    let path = store::catalog_path(&dir);
    store::save(&catalog, &path).unwrap();
    let args = Args::parse([
        "analyze".to_string(),
        "--data".to_string(),
        dir.to_str().unwrap().to_string(),
    ]);
    analyze_with_progress(&args, |_, _, _| {
        let _guard = aede_core::store_lock::StoreLock::try_acquire(&dir)
            .expect("decoder must not hold writer lock");
        let mut current = store::load(&path).unwrap().unwrap();
        current
            .excluded
            .push(dir.join("another-writer").to_str().unwrap().to_string());
        store::save(&current, &path).unwrap();
    })
    .unwrap();
    let saved = store::load(&path).unwrap().unwrap();
    assert_eq!(
        saved.excluded,
        [dir.join("another-writer").to_str().unwrap()]
    );
    assert_eq!(saved.analyses.len(), 1);
    assert_eq!(saved.analyses[0].md5_state.as_deref(), Some("Match"));
    std::fs::remove_dir_all(dir).unwrap();
}
