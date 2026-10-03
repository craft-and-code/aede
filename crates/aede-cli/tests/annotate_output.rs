//! Personal text is literal in a terminal and exact in machine exports.

use aede_core::model::{self, EntityKind, ScannedFile};
use aede_core::user::{self, EntityRef, LOCAL_USER, UserData};
use aede_core::{json, store, tags};
use std::process::Command;

#[test]
fn imported_notes_and_waiting_references_cannot_send_terminal_instructions() {
    let dir = std::env::temp_dir().join(format!("aede-annotation-output-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let catalog = model::build(
        ["/new/a.flac", "/new/b.flac"]
            .into_iter()
            .map(|path| ScannedFile {
                path: path.into(),
                size: 1,
                mtime: 1,
                tags: tags::RawTags::default(),
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            })
            .collect(),
        vec!["/new".into()],
        1,
        &[],
    );
    store::save(&catalog, &store::catalog_path(&dir)).unwrap();
    let instructions = "\x1b]52;c;dGVzdA==\x07";
    let written = format!("first\n\n{instructions}\rsecond\tparagraph\u{85}");
    let current = EntityRef::new(EntityKind::Track, "/new/a.flac");
    let waiting = EntityRef::new(EntityKind::Track, format!("/old/{instructions}b.flac"));
    let mut data = UserData::default();
    let entry = data.entry(LOCAL_USER, &current, 1);
    entry.note = Some(written.clone());
    entry.tags.insert(instructions.into());
    data.entry(LOCAL_USER, &waiting, 1).note = Some("keep this note".into());
    user::save(&data, &user::user_path(&dir)).unwrap();
    let initial = std::fs::read(user::user_path(&dir)).unwrap();
    let run = |arguments: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(arguments)
            .arg(format!("--data={}", dir.display()))
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    for arguments in [
        vec!["note", "track:/new/a.flac"],
        vec!["notes", "--waiting", "--all"],
        vec![
            "notes",
            "--relink",
            &waiting.to_token(),
            "--to",
            "track:/new/b.flac",
            "--dry-run",
        ],
    ] {
        let output = run(&arguments);
        assert!(
            !output
                .chars()
                .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t')),
            "terminal instructions leaked from {arguments:?}: {output:?}"
        );
        assert!(output.contains("\\u{1b}]52;c;dGVzdA==\\u{7}"));
    }
    let exported = json::parse(&run(&["notes", "--export"])).unwrap();
    let restored = user::from_json(&exported).unwrap();
    assert_eq!(
        restored.find(LOCAL_USER, &current).unwrap().note.as_deref(),
        Some(written.as_str())
    );
    assert_eq!(
        restored.find(LOCAL_USER, &current).unwrap().tags,
        data.find(LOCAL_USER, &current).unwrap().tags
    );
    assert!(restored.find(LOCAL_USER, &waiting).is_some());
    assert_eq!(std::fs::read(user::user_path(&dir)).unwrap(), initial);
    std::fs::remove_dir_all(dir).unwrap();
}
