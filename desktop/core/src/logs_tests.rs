use super::*;
#[test]
fn bounded_entries_and_clear_preserve_incremental_cursor() {
    let logs = Logs::default();
    for n in 0..CAPACITY + 4 {
        logs.push("info", n.to_string());
    }
    let snapshot = logs.snapshot(0).unwrap();
    assert_eq!(snapshot.entries.len(), CAPACITY);
    assert_eq!(snapshot.entries[0].id, 5);
    assert_eq!(logs.snapshot(snapshot.next_id).unwrap().entries.len(), 0);
    logs.clear().unwrap();
    logs.push("info", "after clear".into());
    assert_eq!(
        logs.snapshot(snapshot.next_id).unwrap().entries[0].message,
        "after clear"
    );
}
#[test]
fn process_diagnostics_never_publish_raw_output_or_oversized_lines() {
    let logs = Logs::default();
    let mut input = vec![b's'; 2 * LINE_LIMIT];
    input.extend_from_slice(b"\npassword=secret\nAEDE_HTTP {\"method\":\"GET\",\"route\":\"/api/tracks/{id}\",\"status\":500,\"elapsedMs\":12}\n");
    input.extend_from_slice(b"AEDE_HTTP {\"method\":\"GET\",\"route\":\"/api?token=secret\",\"status\":200,\"elapsedMs\":1}\n");
    logs.drain(std::io::Cursor::new(input));
    let snapshot = logs.snapshot(0).unwrap();
    assert_eq!(snapshot.entries.len(), 2);
    assert_eq!(snapshot.entries[1].level, "error");
    assert!(snapshot.entries[1].message.contains("500"));
    let text = serde_json::to_string(&snapshot).unwrap();
    assert!(!text.contains("secret"));
    assert!(!text.contains("password"));
    let export = logs.export_json().unwrap();
    assert!(!export.contains("secret"));
    assert!(!export.contains("password"));
}

#[test]
fn export_contains_the_whole_retained_safe_tail_and_survives_view_cursors() {
    let logs = Logs::default();
    assert!(logs.export_json().is_err());
    for n in 0..CAPACITY + 3 {
        logs.push("info", format!("event {n}"));
    }
    let cursor = logs.snapshot(0).unwrap().next_id;
    assert!(logs.snapshot(cursor).unwrap().entries.is_empty());
    let export: serde_json::Value = serde_json::from_str(&logs.export_json().unwrap()).unwrap();
    assert_eq!(export["format"], "aede-logs");
    assert_eq!(export["version"], 1);
    assert_eq!(export["entries"].as_array().unwrap().len(), CAPACITY);
    assert_eq!(export["entries"][0]["message"], "event 3");
    assert_eq!(export["entries"][CAPACITY - 1]["id"], cursor);
    logs.clear().unwrap();
    assert!(logs.export_json().is_err());
}
