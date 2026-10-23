use super::support::{Fixture, assert_catalog, available_port, completed_scan, write_wav};
use aede_desktop_core::Host;
use serde_json::Value;
#[cfg(unix)]
use std::time::{Duration, Instant};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub(super) fn profile_bytes(fixture: &Fixture) -> Vec<u8> {
    fs::read(fixture.0.join("profile").join("server.json")).unwrap()
}

#[cfg(unix)]
pub(super) fn catalog_bytes(fixture: &Fixture) -> Vec<u8> {
    fs::read(
        fixture
            .0
            .join("profile")
            .join("engine")
            .join("catalog.json"),
    )
    .unwrap()
}

pub(super) fn assert_roots(catalog: &Value, expected: &[PathBuf]) {
    let roots: BTreeSet<_> = catalog["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|root| PathBuf::from(root.as_str().unwrap()))
        .collect();
    assert_eq!(roots, expected.iter().cloned().collect());
}

pub(super) fn assert_unavailable(host: &mut Host, selected: &Path) {
    let status = serde_json::to_value(host.status().unwrap()).unwrap();
    assert!(
        status["roots"].as_array().unwrap().iter().any(|root| {
            Path::new(root["path"].as_str().unwrap()) == selected && root["available"] == false
        }),
        "{status}"
    );
}

pub(super) fn retains_noncanonical_selection(fixture: &Fixture, real: &Path, selected: &Path) {
    let retained = real.join("retained.wav");
    write_wav(&retained);
    let mut host = fixture.host();
    host.configure(available_port(), vec![selected.to_str().unwrap().into()])
        .unwrap();
    host.start().unwrap();
    completed_scan(&mut host);
    let initial = fixture.catalog();
    assert_roots(&initial, &[real.to_path_buf()]);
    let profile = profile_bytes(fixture);
    let detached = fixture.0.join("detached-volume");
    fs::rename(real, &detached).unwrap();
    completed_scan(&mut host);
    let absent = fixture.catalog();
    assert_catalog(&mut host, &absent, std::slice::from_ref(&retained));
    assert_roots(&absent, &[real.to_path_buf()]);
    assert_eq!(absent["file"], initial["file"]);
    assert_eq!(absent["track"], initial["track"]);
    assert_unavailable(&mut host, selected);
    fs::rename(detached, real).unwrap();
    completed_scan(&mut host);
    assert_catalog(&mut host, &fixture.catalog(), &[retained]);
    assert_eq!(fixture.catalog()["track"], initial["track"]);
    assert_eq!(profile_bytes(fixture), profile);
    host.stop().unwrap();
}

#[cfg(unix)]
pub(super) fn assert_failed_without_publication(
    host: &mut Host,
    fixture: &Fixture,
    before: &[u8],
    catalog: &Value,
) {
    let task = host.scan().unwrap();
    let id = task["task_id"].as_u64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let failed = loop {
        let task = host.task(id, false).unwrap();
        match task["status"].as_str().unwrap() {
            "queued" | "running" => {}
            _ => break task,
        }
        assert!(Instant::now() < deadline, "scan timed out: {task}");
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(
        failed["status"], "failed",
        "ambiguous root identity was accepted: {failed}"
    );
    let diagnostic = failed["result"]["stderr"].as_str().unwrap();
    assert!(diagnostic.contains("identity"), "{failed}");
    assert!(
        diagnostic.contains("reconnect") || diagnostic.contains("select"),
        "{failed}"
    );
    assert_eq!(
        fs::read(
            fixture
                .0
                .join("profile")
                .join("engine")
                .join("catalog.json")
        )
        .unwrap(),
        before
    );
    assert_eq!(&fixture.catalog(), catalog);
    let paths = catalog["file"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| PathBuf::from(file["path"].as_str().unwrap()))
        .collect::<Vec<_>>();
    assert_catalog(host, catalog, &paths);
}

#[cfg(unix)]
pub(super) fn refuses_changed_alias(retarget: bool) {
    let fixture = Fixture::new();
    let real = fixture.0.join("real-volume");
    let alias = fixture.0.join("selected-volume");
    let neighbor = fixture.0.join("available-volume");
    fs::create_dir(&real).unwrap();
    fs::create_dir(&neighbor).unwrap();
    std::os::unix::fs::symlink("real-volume", &alias).unwrap();
    write_wav(&real.join("retained.wav"));
    let mut host = fixture.host();
    host.configure(
        available_port(),
        vec![
            alias.to_str().unwrap().into(),
            neighbor.to_str().unwrap().into(),
        ],
    )
    .unwrap();
    host.start().unwrap();
    completed_scan(&mut host);
    let initial = fixture.catalog();
    let before = fs::read(
        fixture
            .0
            .join("profile")
            .join("engine")
            .join("catalog.json"),
    )
    .unwrap();
    let profile = profile_bytes(&fixture);
    fs::remove_file(&alias).unwrap();
    if retarget {
        std::os::unix::fs::symlink("unknown-volume", &alias).unwrap();
    }
    write_wav(&neighbor.join("added.wav"));
    assert_failed_without_publication(&mut host, &fixture, &before, &initial);
    assert_unavailable(&mut host, &alias);
    assert_eq!(profile_bytes(&fixture), profile);
    if retarget {
        fs::remove_file(&alias).unwrap();
    }
    std::os::unix::fs::symlink("real-volume", &alias).unwrap();
    completed_scan(&mut host);
    assert_catalog(
        &mut host,
        &fixture.catalog(),
        &[real.join("retained.wav"), neighbor.join("added.wav")],
    );
    host.stop().unwrap();
}

#[cfg(unix)]
pub(super) fn refuses_unresolvable_selection(parent_component: bool) {
    let fixture = Fixture::new();
    let real = fixture.0.join("real-volume");
    let pivot = fixture.0.join("pivot");
    fs::create_dir(&real).unwrap();
    fs::create_dir(&pivot).unwrap();
    let alias = fixture.0.join("selected-volume");
    std::os::unix::fs::symlink(
        if parent_component {
            "pivot"
        } else {
            "real-volume"
        },
        &alias,
    )
    .unwrap();
    let selected = if parent_component {
        alias.join("..").join("real-volume")
    } else {
        alias.clone()
    };
    let retained = real.join("retained.wav");
    write_wav(&retained);
    let mut host = fixture.host();
    host.configure(available_port(), vec![selected.to_str().unwrap().into()])
        .unwrap();
    host.start().unwrap();
    completed_scan(&mut host);
    let initial = fixture.catalog();
    let before = catalog_bytes(&fixture);
    let profile = profile_bytes(&fixture);
    fs::remove_file(&alias).unwrap();
    if !parent_component {
        std::os::unix::fs::symlink("selected-volume", &alias).unwrap();
    }
    assert_failed_without_publication(&mut host, &fixture, &before, &initial);
    assert_eq!(profile_bytes(&fixture), profile);
    host.stop().unwrap();
}
