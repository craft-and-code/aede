#![cfg(feature = "engine-tests")]

#[path = "support/unavailable_roots.rs"]
mod support;

use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::PathBuf};
use support::{Fixture, assert_catalog, available_port, completed_scan, write_wav};

#[test]
fn host_scan_retains_absent_roots_refreshes_available_roots_and_reads_remounted_roots() {
    let fixture = Fixture::new();
    let first = fixture.0.join("first-volume");
    let second = fixture.0.join("second-volume");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let retained = first.join("retained.wav");
    let added = second.join("added.wav");
    write_wav(&retained);
    let mut host = fixture.host();
    host.configure(
        available_port(),
        vec![
            first.to_str().unwrap().into(),
            second.to_str().unwrap().into(),
        ],
    )
    .unwrap();
    host.start().unwrap();
    completed_scan(&mut host);
    let initial = fixture.catalog();
    assert_catalog(&mut host, &initial, std::slice::from_ref(&retained));

    let detached_first = fixture.0.join("detached-first");
    fs::rename(&first, &detached_first).unwrap();
    write_wav(&added);
    completed_scan(&mut host);
    let mixed = fixture.catalog();
    assert_catalog(&mut host, &mixed, &[retained.clone(), added.clone()]);
    assert!(
        mixed["track"]
            .as_array()
            .unwrap()
            .contains(&initial["track"][0])
    );
    let roots: BTreeSet<PathBuf> = mixed["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|root| PathBuf::from(root.as_str().unwrap()))
        .collect();
    assert_eq!(roots, BTreeSet::from([first.clone(), second.clone()]));
    let status = serde_json::to_value(host.status().unwrap()).unwrap();
    assert!(status["roots"].as_array().unwrap().iter().any(|root| {
        std::path::Path::new(root["path"].as_str().unwrap()) == first && root["available"] == false
    }));

    let detached_second = fixture.0.join("detached-second");
    fs::rename(&second, &detached_second).unwrap();
    completed_scan(&mut host);
    let absent = fixture.catalog();
    assert_catalog(&mut host, &absent, &[retained, added.clone()]);
    assert_eq!(absent["file"], mixed["file"]);
    assert_eq!(absent["track"], mixed["track"]);

    fs::remove_file(detached_first.join("retained.wav")).unwrap();
    write_wav(&detached_first.join("returned.wav"));
    fs::rename(&detached_first, &first).unwrap();
    fs::rename(&detached_second, &second).unwrap();
    completed_scan(&mut host);
    let returned = fixture.catalog();
    assert_catalog(&mut host, &returned, &[first.join("returned.wav"), added]);
    assert!(
        serde_json::to_value(host.status().unwrap()).unwrap()["roots"]
            .as_array()
            .unwrap()
            .iter()
            .all(|root| root["available"] == true)
    );
    host.stop().unwrap();
}

#[test]
fn cli_accepts_only_exact_unavailable_roots_in_the_owned_job_profile() {
    let fixture = Fixture::new();
    let missing = fixture.0.join("absent-volume");
    let declaration = json!([missing]).to_string();
    for (child, data) in [(None, false), (Some("1"), false), (Some("0"), true)] {
        let mut command = fixture.command(&missing);
        command.env("AEDE_UNAVAILABLE_ROOTS", &declaration);
        if let Some(value) = child {
            command.env("AEDE_DELEGATED_CHILD", value);
        }
        if data {
            command.env("AEDE_DELEGATED_DATA_DIR", fixture.0.join("cli-data"));
        }
        let output = command.output().unwrap();
        assert!(
            !output.status.success(),
            "incomplete job profile admitted an absent root"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("not a readable folder"));
    }
    let ordinary = fixture.command(&missing).output().unwrap();
    assert!(!ordinary.status.success());
    assert!(String::from_utf8_lossy(&ordinary.stderr).contains("not a readable folder"));

    for target in [
        missing.join("child"),
        fixture.0.join("absent-volume-neighbor"),
    ] {
        let output = fixture
            .command(&target)
            .env("AEDE_DELEGATED_CHILD", "1")
            .env("AEDE_DELEGATED_DATA_DIR", fixture.0.join("cli-data"))
            .env("AEDE_UNAVAILABLE_ROOTS", &declaration)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "undeclared absent path was admitted"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("not a readable folder"));
    }
    let output = fixture
        .command(&missing)
        .env("AEDE_DELEGATED_CHILD", "1")
        .env("AEDE_DELEGATED_DATA_DIR", fixture.0.join("cli-data"))
        .env("AEDE_UNAVAILABLE_ROOTS", declaration)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_refuses_malformed_or_unbounded_unavailable_root_declarations() {
    let fixture = Fixture::new();
    let available = fixture.0.join("available");
    fs::create_dir(&available).unwrap();
    let absolute = fixture.0.join("absent-volume");
    let oversized = fixture.0.join("x".repeat(4097));
    let nul = format!("{}\0suffix", absolute.display());
    let invalid: Vec<String> = vec![
        "[".into(),
        json!({"root": absolute}).to_string(),
        json!([1]).to_string(),
        json!(["relative"]).to_string(),
        json!(vec![absolute; 65]).to_string(),
        json!([oversized]).to_string(),
        json!([nul]).to_string(),
    ];
    for declaration in invalid {
        let output = fixture
            .command(&available)
            .env("AEDE_DELEGATED_CHILD", "1")
            .env("AEDE_DELEGATED_DATA_DIR", fixture.0.join("cli-data"))
            .env("AEDE_UNAVAILABLE_ROOTS", &declaration)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "invalid declaration was admitted: {declaration}"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("invalid unavailable root"));
    }
    let output = fixture
        .command(&available)
        .env("AEDE_DELEGATED_CHILD", "1")
        .env("AEDE_DELEGATED_DATA_DIR", fixture.0.join("cli-data"))
        .env(
            "AEDE_UNAVAILABLE_ROOTS",
            Value::Array(Vec::new()).to_string(),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
