use super::*;

#[test]
fn a_failed_picture_fails_the_batch_without_abandoning_other_tracks() {
    let dir = std::env::temp_dir().join(format!(
        "aede_spectrum_batch_failure_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let work: Vec<_> = ["one", "bad", "three"]
        .into_iter()
        .map(|name| {
            (
                dir.join(format!("{name}.flac")),
                dir.join(format!("{name}.png")),
                String::from("caption"),
            )
        })
        .collect();
    let attempted = Mutex::new(Vec::new());
    let error = draw_pictures(work, 1, |audio, picture, caption| {
        attempted.lock().unwrap().push(audio.to_path_buf());
        assert_eq!(caption, "caption");
        if audio.file_stem().unwrap() == "bad" {
            return Err("decoder failed".into());
        }
        std::fs::write(picture, b"completed picture").unwrap();
        Ok(())
    })
    .unwrap_err();
    assert!(error.to_string().contains("1 spectrogram"), "{error}");
    assert_eq!(attempted.into_inner().unwrap().len(), 3);
    for name in ["one.png", "three.png"] {
        assert_eq!(std::fs::read(dir.join(name)).unwrap(), b"completed picture");
    }
    assert!(!dir.join("bad.png").exists());

    draw_pictures(
        vec![(dir.join("retry.flac"), dir.join("retry.png"), String::new())],
        1,
        |_, picture, _| {
            std::fs::write(picture, b"retry complete").unwrap();
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        std::fs::read(dir.join("retry.png")).unwrap(),
        b"retry complete"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
