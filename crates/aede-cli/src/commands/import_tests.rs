use super::*;

struct Directory(std::path::PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn import_and_forget_refuse_listing_options_before_changing_stores() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = Directory(std::env::temp_dir().join(format!(
        "aede-import-options-{}-{nonce}",
        std::process::id()
    )));
    std::fs::create_dir_all(&directory.0).unwrap();
    let catalog = Catalog {
        analyses: ["/waiting/a/01.flac", "/waiting/b/02.flac"]
            .into_iter()
            .map(|path| FileAnalysis {
                path: path.into(),
                source: "flaccompagnon".into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let catalog_path = store::catalog_path(&directory.0);
    store::save(&catalog, &catalog_path).unwrap();
    let mut personal = aede_core::user::UserData::default();
    let target =
        aede_core::user::EntityRef::new(aede_core::model::EntityKind::Track, "/waiting/a/01.flac");
    for owner in [aede_core::user::LOCAL_USER, "another-owner"] {
        personal.entry(owner, &target, 1).note = Some(format!("preserve {owner}"));
    }
    aede_core::user::save(&personal, &aede_core::user::user_path(&directory.0)).unwrap();
    aede_core::sources::save(
        &Default::default(),
        &aede_core::sources::sources_path(&directory.0),
    )
    .unwrap();
    let report = directory.0.join("empty-report.json");
    let report_text = r#"{"format":"flaccompagnon-report","version":1,"report":{"files":[]}}"#;
    std::fs::write(&report, report_text).unwrap();
    let report_name = report.to_str().unwrap();
    let nonempty_report = directory.0.join("new-report.json");
    let nonempty_text = r#"{"format":"flaccompagnon-report","version":1,"report":{"files":[{"path":"/waiting/c/03.flac"}]}}"#;
    std::fs::write(&nonempty_report, nonempty_text).unwrap();
    let nonempty_name = nonempty_report.to_str().unwrap();
    let arguments = |words: &[&str]| {
        Args::parse(
            [
                "import".to_owned(),
                format!("--data={}", directory.0.display()),
            ]
            .into_iter()
            .chain(words.iter().map(|word| word.to_string())),
        )
    };
    let snapshots = || {
        [
            "catalog.json",
            "conclusions.json",
            "sources.json",
            "user.json",
        ]
        .map(|name| std::fs::read(directory.0.join(name)).ok())
    };
    let before = snapshots();
    for words in [
        vec!["--forget", "--limit=0"],
        vec!["--forget", "--limit=1"],
        vec!["--forget", "--pending", "--offset=invalid"],
        vec!["--forget", "--pending", "--offset=1"],
        vec!["--forget", "--all"],
        vec!["--forget", "--list"],
        vec!["--forget", "--pending", "--list"],
        vec![report_name, "--limit=0"],
        vec![report_name, "--limit=1"],
        vec![report_name, "--offset=invalid"],
        vec![report_name, "--offset=1"],
        vec![report_name, "--all"],
        vec![report_name, "--all", "--limit=1"],
        vec![nonempty_name, "--source=another-tool"],
        vec![nonempty_name, "--source=flaccompagnon"],
    ] {
        assert!(import(&arguments(&words)).is_err(), "accepted {words:?}");
        assert_eq!(snapshots(), before, "changed stores for {words:?}");
        assert_eq!(std::fs::read_to_string(&report).unwrap(), report_text);
        assert_eq!(
            std::fs::read_to_string(&nonempty_report).unwrap(),
            nonempty_text
        );
    }
    for words in [
        vec!["--list", "--limit=1"],
        vec!["--pending", "--offset=1"],
        vec!["--list", "--pending", "--all"],
        vec![report_name],
    ] {
        import(&arguments(&words)).unwrap();
        assert_eq!(snapshots(), before, "changed stores for {words:?}");
    }
    import(&arguments(&[
        "--forget",
        "--yes",
        "--pending",
        "--source=flaccompagnon",
    ]))
    .unwrap();
    assert!(
        store::load(&catalog_path)
            .unwrap()
            .unwrap()
            .analyses
            .is_empty()
    );
    let after = snapshots();
    assert_eq!(after[2..], before[2..], "personal/source data changed");
    assert_eq!(std::fs::read_to_string(&report).unwrap(), report_text);
}

#[test]
fn empty_import_lists_still_refuse_invalid_windows() {
    let populated = Catalog {
        analyses: vec![FileAnalysis {
            path: "/not-yet-scanned/Album/01.flac".into(),
            source: "flaccompagnon".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    for catalog in [&Catalog::default(), &populated] {
        for mode in ["--pending", "--list"] {
            for source in [None, Some("--source=another-tool")] {
                let list = |window: &[&str]| {
                    let words = ["import", mode]
                        .into_iter()
                        .chain(source)
                        .chain(window.iter().copied())
                        .map(str::to_owned);
                    let args = Args::parse(words);
                    if mode == "--pending" {
                        list_pending(catalog, &args)
                    } else {
                        list_all(catalog, &args)
                    }
                };
                for invalid in [
                    vec!["--limit=0"],
                    vec!["--offset=-1"],
                    vec!["--all", "--limit=1"],
                ] {
                    assert!(
                        list(&invalid).is_err(),
                        "accepted {mode} {source:?} {invalid:?}"
                    );
                }
                for valid in [vec!["--limit=1"], vec!["--offset=99"], vec!["--all"]] {
                    list(&valid).unwrap();
                }
            }
        }
    }
}
