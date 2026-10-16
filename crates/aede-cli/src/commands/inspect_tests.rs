use super::*;

#[test]
fn file_inspection_refuses_additional_paths_and_retains_single_file_behaviour() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures");
    let first = fixtures.join("track.flac");
    let second = fixtures.join("track.mp3");
    let first_before = std::fs::read(&first).unwrap();
    let second_before = std::fs::read(&second).unwrap();
    for path in [first.clone(), fixtures.join("missing-inspection-test.flac")] {
        let args = Args::parse([
            "file".to_string(),
            path.to_str().unwrap().to_string(),
            second.to_str().unwrap().to_string(),
        ]);
        let error = inspect(&args)
            .expect_err("multiple file paths must be refused before inspecting either")
            .to_string();
        assert!(error.contains("exactly one file"), "{error}");
    }
    let missing = Args::parse(["file".to_string()]);
    assert!(
        inspect(&missing)
            .unwrap_err()
            .to_string()
            .contains("give a file")
    );
    let single = Args::parse(["file".to_string(), first.to_str().unwrap().to_string()]);
    inspect(&single).expect("the single-file inspection still reads a valid file");
    assert_eq!(std::fs::read(first).unwrap(), first_before);
    assert_eq!(std::fs::read(second).unwrap(), second_before);
}
