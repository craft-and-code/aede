#![cfg(all(feature = "engine-tests", unix))]

#[path = "support/unavailable_roots.rs"]
mod support;

use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use support::{Fixture, assert_catalog, available_port, completed_scan, write_wav};

#[test]
fn a_removed_alias_under_a_retained_parent_cannot_silently_drop_its_previous_files() {
    let fixture = Fixture::new();
    let parent = fixture.0.join("parent-volume");
    let real = parent.join("real-volume");
    let alias = fixture.0.join("selected-volume");
    fs::create_dir_all(&real).unwrap();
    std::os::unix::fs::symlink(PathBuf::from("parent-volume").join("real-volume"), &alias).unwrap();
    let retained = real.join("retained.wav");
    write_wav(&retained);
    let mut host = fixture.host();
    host.configure(
        available_port(),
        vec![
            alias.to_str().unwrap().into(),
            parent.to_str().unwrap().into(),
        ],
    )
    .unwrap();
    host.start().unwrap();
    completed_scan(&mut host);
    let initial = fixture.catalog();
    assert_catalog(&mut host, &initial, std::slice::from_ref(&retained));
    let roots = initial["roots"].as_array().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(std::path::Path::new(roots[0].as_str().unwrap()), parent);
    let catalog_path = fixture
        .0
        .join("profile")
        .join("engine")
        .join("catalog.json");
    let profile_path = fixture.0.join("profile").join("server.json");
    let before = fs::read(&catalog_path).unwrap();
    let profile = fs::read(&profile_path).unwrap();

    fs::remove_file(&alias).unwrap();
    fs::rename(&real, fixture.0.join("detached-volume")).unwrap();
    let added = parent.join("added.wav");
    write_wav(&added);
    let ordinary = fixture.command(&alias).output().unwrap();
    assert!(!ordinary.status.success());
    assert!(String::from_utf8_lossy(&ordinary.stderr).contains("not a readable folder"));
    let accepted = host.scan().unwrap();
    let id = accepted["task_id"].as_u64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let terminal = loop {
        let task = host.task(id, false).unwrap();
        match task["status"].as_str().unwrap() {
            "queued" | "running" => {}
            _ => break task,
        }
        assert!(Instant::now() < deadline, "scan timed out: {task}");
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(terminal["status"], "failed", "{terminal}");
    let diagnostic = terminal["result"]["stderr"].as_str().unwrap();
    assert!(diagnostic.contains("identity"), "{terminal}");
    assert!(
        diagnostic.contains("reconnect") || diagnostic.contains("select"),
        "{terminal}"
    );
    assert_eq!(fs::read(&catalog_path).unwrap(), before);
    assert_eq!(fs::read(&profile_path).unwrap(), profile);
    assert_eq!(fixture.catalog(), initial);
    assert_catalog(&mut host, &initial, std::slice::from_ref(&retained));

    fs::rename(fixture.0.join("detached-volume"), &real).unwrap();
    std::os::unix::fs::symlink(PathBuf::from("parent-volume").join("real-volume"), &alias).unwrap();
    completed_scan(&mut host);
    assert_catalog(&mut host, &fixture.catalog(), &[retained, added]);
    assert_eq!(fs::read(&profile_path).unwrap(), profile);
    host.stop().unwrap();
}
