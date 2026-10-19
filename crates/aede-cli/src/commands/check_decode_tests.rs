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

#[cfg(unix)]
#[test]
fn failed_external_decode_retains_the_actual_diagnostic_without_retrying_or_writing_audio() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    struct Program(PathBuf);
    impl Drop for Program {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let program = Program(std::env::temp_dir().join(format!(
        "aede-decode-diagnostic-{}-{nonce}",
        std::process::id()
    )));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&program.0)
        .unwrap();
    file.write_all(b"#!/bin/sh\nprintf 'ERROR: decoded MD5 signature mismatch\n' >&2\nexit 1\n")
        .unwrap();
    file.set_permissions(std::fs::Permissions::from_mode(0o700))
        .unwrap();
    drop(file);
    let source = fixture();
    let before = std::fs::read(&source).unwrap();
    let error = verify(&source, true, Some(&program.0)).err().unwrap();
    assert!(error.contains("decoded MD5 signature mismatch"), "{error}");
    assert_eq!(std::fs::read(source).unwrap(), before);
}

#[test]
fn decoder_diagnostics_are_bounded_utf8_plain_text_while_the_entire_pipe_is_drained() {
    let mut bytes = std::io::Cursor::new("\u{1b}音\r".repeat(12000).into_bytes());
    let text = decode_diagnostic(&mut bytes).unwrap();
    assert_eq!(bytes.position(), bytes.get_ref().len() as u64);
    assert!(text.len() < 4096);
    assert!(!text.contains('\u{1b}'));
    assert!(!text.contains('\r'));
    assert!(text.ends_with("[diagnostic truncated]"));
    assert_eq!(
        decode_diagnostic(&b"ERROR: MD5 mismatch\n"[..]).unwrap(),
        "ERROR: MD5 mismatch"
    );
}

#[test]
fn json_flushes_each_verified_file_before_starting_the_next_decode() {
    use std::cell::RefCell;
    use std::rc::Rc;
    struct Writer {
        data: Rc<RefCell<Vec<u8>>>,
        flushed: Rc<RefCell<usize>>,
    }
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.data.borrow_mut().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            *self.flushed.borrow_mut() = self.data.borrow().len();
            Ok(())
        }
    }
    let data = Rc::new(RefCell::new(Vec::new()));
    let flushed = Rc::new(RefCell::new(0));
    let mut writer = Writer {
        data: data.clone(),
        flushed: flushed.clone(),
    };
    let rows = ["/first.flac", "/second.flac"]
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            if index == 1 {
                assert!(*flushed.borrow() > 0);
                let text = String::from_utf8(data.borrow().clone()).unwrap();
                assert!(text.contains("/first.flac"));
                assert!(!text.contains("/second.flac"));
            }
            FileResult {
                path,
                outcome: Ok(Verification {
                    method: "Rust",
                    hash: true,
                }),
            }
        });
    assert_eq!(write_json(&mut writer, rows).unwrap(), 0);
    let parsed =
        aede_core::json::parse(&String::from_utf8(data.borrow().clone()).unwrap()).unwrap();
    assert_eq!(
        parsed
            .get("version")
            .and_then(aede_core::json::Json::as_u64),
        Some(1)
    );
    assert_eq!(parsed.get("files").unwrap().as_arr().unwrap().len(), 2);
}
