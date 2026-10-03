use super::*;

fn args(words: &[&str]) -> Args {
    Args::parse(words.iter().map(|w| w.to_string()))
}

fn recipe(convert: Option<Target>) -> Recipe {
    Recipe {
        extras: Extras::default(),
        restrict_names: false,
        convert,
        quality: None,
    }
}

#[test]
fn how_many_files_at_once_follows_the_work_and_not_the_machine() {
    // A plain copy is a queue at one device — one card, one stick, one slow
    // drive — and several writers on it seek against each other rather than
    // going faster. `--verify`, which reads back what it just wrote, makes
    // that worse still.
    assert_eq!(
        workers(&args(&["copy", "/out"]), &recipe(None), 900).unwrap(),
        1
    );

    // Encoding is arithmetic: each file is an ffmpeg run that no other file
    // waits on, and one at a time leaves most of the machine idle.
    let encoding = workers(&args(&["copy", "/out"]), &recipe(Some(Target::Mp3)), 900).unwrap();
    assert!(encoding >= 1, "at least one");
    assert_eq!(
        encoding,
        aede_core::scan::resolve_threads(0),
        "as many as the machine offers"
    );

    // And the person copying to an NVMe, or encoding on a laptop they still
    // want to use, overrides it in either direction.
    assert_eq!(
        workers(&args(&["copy", "/out", "--threads=6"]), &recipe(None), 900).unwrap(),
        6
    );
    assert_eq!(
        workers(
            &args(&["copy", "/out", "--threads=1"]),
            &recipe(Some(Target::Mp3)),
            900
        )
        .unwrap(),
        1
    );

    // Never more workers than there is work: eight threads over three files
    // is five threads spawned to find an empty queue.
    assert_eq!(
        workers(&args(&["copy", "/out", "--threads=8"]), &recipe(None), 3).unwrap(),
        3
    );
    // And an empty plan still asks for one, rather than for none.
    assert_eq!(
        workers(&args(&["copy", "/out", "--threads=8"]), &recipe(None), 0).unwrap(),
        1
    );
}

#[test]
fn quality_is_validated_for_the_selected_encoder() {
    for (target, setting) in [
        (Target::Mp3, "V10"),
        (Target::Mp3, "q6"),
        (Target::Vorbis, "V0"),
        (Target::Opus, "V0"),
        (Target::Aac, "q10"),
    ] {
        assert!(
            quality(&args(&["copy", "/out", "--quality", setting]), Some(target)).is_err(),
            "{target:?}: {setting}"
        );
    }
    assert_eq!(
        quality(
            &args(&["copy", "/out", "--quality", "V9"]),
            Some(Target::Mp3)
        )
        .unwrap(),
        Some(Quality::Variable(9))
    );
}

#[test]
fn a_complete_copy_can_resume_when_only_a_little_space_remains() {
    let directory =
        std::env::temp_dir().join(format!("aede_copy_full_resume_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("track.flac"), [42; 1024]).unwrap();
    let plan = Plan {
        items: vec![Item {
            source: PathBuf::from("source.flac"),
            relative: PathBuf::from("track.flac"),
            size: 1024,
            kind: ItemKind::Audio,
            convert: None,
            contents: None,
        }],
        ..Default::default()
    };
    let options = args(&["copy", "/out"]);
    assert!(room_for_available(&plan, &directory, &options, 1, Some(100)).is_ok());
    let verified = args(&["copy", "/out", "--verify-existing"]);
    assert!(room_for_available(&plan, &directory, &verified, 1, Some(100)).is_ok());
    let replaced = args(&["copy", "/out", "--replace"]);
    assert!(room_for_available(&plan, &directory, &replaced, 1, Some(100)).is_err());
    std::fs::remove_file(directory.join("track.flac")).unwrap();
    assert!(room_for_available(&plan, &directory, &options, 1, Some(100)).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn existing_encoded_comparisons_budget_concurrent_temporary_outputs() {
    let directory =
        std::env::temp_dir().join(format!("aede_copy_compare_space_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir(&directory).unwrap();
    let mut plan = Plan::default();
    for size in [1000, 2000, 3000] {
        let relative = PathBuf::from(format!("{size}.mp3"));
        std::fs::write(directory.join(&relative), b"existing encoded file").unwrap();
        plan.items.push(Item {
            source: PathBuf::from("source.flac"),
            relative,
            size,
            kind: ItemKind::Audio,
            convert: Some(Target::Mp3),
            contents: None,
        });
    }
    let options = args(&["copy", "/out", "--verify-existing"]);
    assert_eq!(pending_bytes(&plan, &directory, &options, 1).unwrap(), 3000);
    assert_eq!(pending_bytes(&plan, &directory, &options, 2).unwrap(), 5000);
    assert_eq!(pending_bytes(&plan, &directory, &options, 3).unwrap(), 6000);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn generated_playlists_can_be_verified_published_and_resumed_without_an_encoder() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "aede_copy_verified_playlist_{}_{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    let directory = directory.canonicalize().unwrap();
    let mut item = Item {
        source: PathBuf::from("selected tracks"),
        relative: PathBuf::from("Album").join("selection.m3u8"),
        size: 0,
        kind: ItemKind::Other,
        convert: None,
        contents: Some("#EXTM3U\n01.mp3\n".into()),
    };
    let target = directory.join(&item.relative);
    assert_eq!(
        write_one(&item, &directory, true, false, false, &recipe(None), None).unwrap(),
        copy::Wrote::Copied
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"#EXTM3U\n01.mp3\n");
    assert_eq!(
        write_one(&item, &directory, true, false, false, &recipe(None), None).unwrap(),
        copy::Wrote::Skipped
    );
    item.contents = Some("#EXTM3U\n02.mp3\n".into());
    assert!(write_one(&item, &directory, true, false, false, &recipe(None), None).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"#EXTM3U\n01.mp3\n");
    assert_eq!(
        write_one(&item, &directory, true, true, false, &recipe(None), None).unwrap(),
        copy::Wrote::Copied
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"#EXTM3U\n02.mp3\n");
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1
    );
    std::fs::remove_dir_all(directory).unwrap();
}
