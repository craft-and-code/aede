use super::*;
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac")
}
#[test]
fn native_full_decode_and_missing_external_tool_verify_without_modifying_audio() {
    let path = fixture();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        verify(&path, true, None).unwrap(),
        "Rust · FLAC MD5 verified"
    );
    let missing = std::env::temp_dir().join("aede-no-such-flac-test-executable");
    assert_eq!(
        verify(&path, true, Some(&missing)).unwrap(),
        "Rust · FLAC MD5 verified"
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}
#[test]
fn malformed_sources_and_directories_never_get_a_complete_verdict() {
    assert!(verify(Path::new(env!("CARGO_MANIFEST_DIR")), true, None).is_err());
    assert!(verify(Path::new(file!()), true, None).is_err());
}

#[cfg(unix)]
#[test]
fn an_available_external_verifier_failure_never_retries_with_another_decoder() {
    assert!(
        verify(&fixture(), true, Some(Path::new("/usr/bin/false")))
            .unwrap_err()
            .contains("rejected")
    );
}
