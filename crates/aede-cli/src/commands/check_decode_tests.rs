use super::*;
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac")
}
#[test]
fn native_full_decode_and_missing_external_tool_verify_without_modifying_audio() {
    let path = fixture();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        verify(&path, true, None).unwrap().method,
        "Rust · FLAC MD5 verified"
    );
    let missing = std::env::temp_dir().join("aede-no-such-flac-test-executable");
    assert_eq!(
        verify(&path, true, Some(&missing)).unwrap().method,
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
            .err()
            .unwrap()
            .contains("rejected")
    );
}

#[cfg(unix)]
#[test]
fn external_success_without_a_reference_md5_is_not_a_verified_hash() {
    use std::io::Write;
    struct OwnedFile(PathBuf);
    impl Drop for OwnedFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let file = OwnedFile(std::env::temp_dir().join(format!(
        "aede-decode-no-md5-{}-{}.flac",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    )));
    let mut bytes = std::fs::read(fixture()).unwrap();
    bytes[26..42].fill(0);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file.0)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
    let external = verify(&file.0, true, Some(Path::new("/usr/bin/true"))).unwrap();
    assert!(external.method.contains("no FLAC MD5"));
    assert!(!external.hash);
    assert!(!verify(&file.0, true, None).unwrap().hash);
    assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
}

#[test]
fn structured_decode_results_preserve_distinct_verdicts_and_escape_exact_paths() {
    use aede_core::json::Json;
    let rows = [
        FileResult {
            path: "/fixture/quoted \"title\".flac",
            outcome: Ok(Verification {
                method: "Rust · FLAC MD5 verified",
                hash: true,
            }),
        },
        FileResult {
            path: "/fixture/no-hash.flac",
            outcome: Ok(Verification {
                method: "Rust · decoded, no FLAC MD5",
                hash: false,
            }),
        },
        FileResult {
            path: "/fixture/bad.flac",
            outcome: Err("digest mismatch".into()),
        },
    ];
    let report = structured(&rows);
    let decoded = aede_core::json::parse(&report.to_string_compact()).unwrap();
    assert_eq!(report, decoded);
    assert_eq!(decoded.get("version"), Some(&Json::Num(1.0)));
    let Json::Arr(files) = decoded.get("files").unwrap() else {
        panic!("file array expected")
    };
    assert_eq!(files[0].get("path"), Some(&Json::Str(rows[0].path.into())));
    for (file, status) in files.iter().zip(["ok", "no_hash", "ko"]) {
        assert_eq!(file.get("status"), Some(&Json::Str(status.into())));
    }
    assert_eq!(
        files[2].get("message"),
        Some(&Json::Str("digest mismatch".into()))
    );
}
