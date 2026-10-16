//! Piped note text retains its complete UTF-8 representation.

#[path = "annotation_command_support/mod.rs"]
#[allow(dead_code)]
mod support;

use aede_core::user::LOCAL_USER;
use std::io::Write;
use std::process::{Command, Stdio};
use support::PersonalLibrary;

#[test]
fn piped_notes_preserve_trailing_blank_lines_and_crlf() {
    let library = PersonalLibrary::new();
    for text in ["# Heading\n\nBody\n\n", "# Heading\r\n\r\nBody\r\n"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(["note", &library.track.to_token(), "--file", "-"])
            .arg(format!("--data={}", library.dir.display()))
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            library
                .data()
                .find(LOCAL_USER, &library.track)
                .unwrap()
                .note
                .as_deref(),
            Some(text)
        );
    }
}
