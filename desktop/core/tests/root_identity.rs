#![cfg(feature = "engine-tests")]

#[path = "support/root_identity.rs"]
mod identity;
#[path = "support/unavailable_roots.rs"]
mod support;

#[cfg(unix)]
use identity::assert_failed_without_publication;
use identity::{assert_roots, assert_unavailable, profile_bytes};
use std::fs;
#[cfg(unix)]
use std::path::Path;
use support::{Fixture, assert_catalog, available_port, completed_scan, write_wav};

#[cfg(unix)]
#[test]
fn host_alias_keeps_canonical_identity_while_its_target_is_absent_and_returns() {
    let fixture = Fixture::new();
    let real = fixture.0.join("real-volume");
    let alias = fixture.0.join("selected-volume");
    let neighbor = fixture.0.join("z-available-volume");
    fs::create_dir(&real).unwrap();
    fs::create_dir(&neighbor).unwrap();
    std::os::unix::fs::symlink("real-volume", &alias).unwrap();
    let retained = real.join("retained.wav");
    write_wav(&retained);
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
    assert_catalog(&mut host, &initial, std::slice::from_ref(&retained));
    assert_roots(&initial, &[real.clone(), neighbor.clone()]);
    let profile = profile_bytes(&fixture);

    let detached = fixture.0.join("detached-volume");
    fs::rename(&real, &detached).unwrap();
    let added = neighbor.join("added.wav");
    write_wav(&added);
    completed_scan(&mut host);
    let absent = fixture.catalog();
    assert_catalog(&mut host, &absent, &[retained.clone(), added.clone()]);
    assert_roots(&absent, &[real.clone(), neighbor.clone()]);
    assert!(
        absent["file"]
            .as_array()
            .unwrap()
            .contains(&initial["file"][0])
    );
    assert!(
        absent["track"]
            .as_array()
            .unwrap()
            .contains(&initial["track"][0])
    );
    assert_unavailable(&mut host, &alias);
    assert_eq!(profile_bytes(&fixture), profile);

    let returned = real.join("returned.wav");
    write_wav(&detached.join("returned.wav"));
    fs::rename(detached, &real).unwrap();
    completed_scan(&mut host);
    let restored = fixture.catalog();
    assert_catalog(&mut host, &restored, &[retained, returned, added]);
    assert_roots(&restored, &[real, neighbor]);
    assert!(
        restored["track"]
            .as_array()
            .unwrap()
            .contains(&initial["track"][0])
    );
    assert_eq!(profile_bytes(&fixture), profile);
    host.stop().unwrap();
}

#[test]
fn host_lexical_parent_components_keep_the_same_unavailable_root_identity() {
    let fixture = Fixture::new();
    let real = fixture.0.join("real-volume");
    let hop = fixture.0.join("hop");
    fs::create_dir(&real).unwrap();
    fs::create_dir(&hop).unwrap();
    let selected = hop.join("..").join("real-volume");
    identity::retains_noncanonical_selection(&fixture, &real, &selected);
}

#[cfg(unix)]
#[test]
fn host_parent_symlink_keeps_the_same_unavailable_root_identity() {
    let fixture = Fixture::new();
    let real = fixture.0.join("real-volume");
    fs::create_dir(&real).unwrap();
    let parent_alias = fixture.0.join("parent-alias");
    std::os::unix::fs::symlink(&fixture.0, &parent_alias).unwrap();
    identity::retains_noncanonical_selection(&fixture, &real, &parent_alias.join("real-volume"));
}

#[cfg(unix)]
#[test]
fn host_missing_alias_refuses_an_unknown_previous_identity_without_publication() {
    identity::refuses_changed_alias(false);
}

#[cfg(unix)]
#[test]
fn host_retargeted_dangling_alias_refuses_an_unknown_identity_without_publication() {
    identity::refuses_changed_alias(true);
}

#[test]
fn host_replacement_still_removes_an_intentionally_deselected_root() {
    let fixture = Fixture::new();
    let removed = fixture.0.join("removed-volume");
    let kept = fixture.0.join("kept-volume");
    fs::create_dir(&removed).unwrap();
    fs::create_dir(&kept).unwrap();
    write_wav(&removed.join("removed.wav"));
    let kept_file = kept.join("kept.wav");
    write_wav(&kept_file);
    let mut host = fixture.host();
    let port = available_port();
    host.configure(
        port,
        vec![
            removed.to_str().unwrap().into(),
            kept.to_str().unwrap().into(),
        ],
    )
    .unwrap();
    host.start().unwrap();
    completed_scan(&mut host);
    host.configure(port, vec![kept.to_str().unwrap().into()])
        .unwrap();
    completed_scan(&mut host);
    assert_catalog(&mut host, &fixture.catalog(), &[kept_file]);
    assert_roots(&fixture.catalog(), &[kept]);
    host.stop().unwrap();
}

#[test]
fn ordinary_cli_still_refuses_an_absent_root_without_owned_identity_evidence() {
    let fixture = Fixture::new();
    let missing = fixture.0.join("absent-volume");
    let output = fixture.command(&missing).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not a readable folder"));
    assert!(!fixture.0.join("cli-data").join("catalog.json").exists());
    #[cfg(unix)]
    {
        let alias = fixture.0.join("selected-volume");
        std::os::unix::fs::symlink(Path::new("absent-volume"), &alias).unwrap();
        let output = fixture.command(&alias).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("not a readable folder"));
    }
}

#[test]
fn host_can_add_a_missing_root_when_every_previous_root_remains_selected() {
    let fixture = Fixture::new();
    let real = fixture.0.join("real-volume");
    let missing = fixture.0.join("new-absent-volume");
    fs::create_dir(&real).unwrap();
    let retained = real.join("retained.wav");
    write_wav(&retained);
    let mut host = fixture.host();
    let port = available_port();
    host.configure(port, vec![real.to_str().unwrap().into()])
        .unwrap();
    host.start().unwrap();
    completed_scan(&mut host);
    let initial = fixture.catalog();
    host.configure(
        port,
        vec![
            real.to_str().unwrap().into(),
            missing.to_str().unwrap().into(),
        ],
    )
    .unwrap();
    completed_scan(&mut host);
    let updated = fixture.catalog();
    assert_catalog(&mut host, &updated, &[retained]);
    assert_eq!(updated["track"], initial["track"]);
    assert_roots(&updated, &[real, missing]);
    host.stop().unwrap();
}

#[test]
fn host_can_select_a_known_missing_child_previously_deduplicated_under_its_parent() {
    let fixture = Fixture::new();
    let parent = fixture.0.join("parent-volume");
    let child = parent.join("child-volume");
    fs::create_dir_all(&child).unwrap();
    let retained = child.join("retained.wav");
    write_wav(&retained);
    write_wav(&parent.join("deselected.wav"));
    let mut host = fixture.host();
    let port = available_port();
    host.configure(
        port,
        vec![
            parent.to_str().unwrap().into(),
            child.to_str().unwrap().into(),
        ],
    )
    .unwrap();
    host.start().unwrap();
    completed_scan(&mut host);
    let initial = fixture.catalog();
    assert_roots(&initial, &[parent]);
    fs::rename(&child, fixture.0.join("detached-child")).unwrap();
    host.configure(port, vec![child.to_str().unwrap().into()])
        .unwrap();
    completed_scan(&mut host);
    let updated = fixture.catalog();
    assert_catalog(&mut host, &updated, &[retained]);
    assert_roots(&updated, &[child]);
    assert!(
        initial["track"]
            .as_array()
            .unwrap()
            .contains(&updated["track"][0])
    );
    host.stop().unwrap();
}

#[cfg(unix)]
#[test]
fn host_retargeted_alias_cannot_claim_a_different_known_unavailable_root() {
    let fixture = Fixture::new();
    let real = fixture.0.join("real-volume");
    let other = fixture.0.join("other-volume");
    let alias = fixture.0.join("selected-volume");
    fs::create_dir(&real).unwrap();
    fs::create_dir(&other).unwrap();
    write_wav(&real.join("retained.wav"));
    write_wav(&other.join("other.wav"));
    std::os::unix::fs::symlink("real-volume", &alias).unwrap();
    let mut host = fixture.host();
    host.configure(
        available_port(),
        vec![
            alias.to_str().unwrap().into(),
            other.to_str().unwrap().into(),
        ],
    )
    .unwrap();
    host.start().unwrap();
    completed_scan(&mut host);
    let initial = fixture.catalog();
    let before = identity::catalog_bytes(&fixture);
    let profile = profile_bytes(&fixture);
    fs::rename(&other, fixture.0.join("detached-other")).unwrap();
    fs::remove_file(&alias).unwrap();
    std::os::unix::fs::symlink("other-volume", &alias).unwrap();
    assert_failed_without_publication(&mut host, &fixture, &before, &initial);
    assert_eq!(profile_bytes(&fixture), profile);
    host.stop().unwrap();
}

#[cfg(unix)]
#[test]
fn host_alias_cycle_refuses_before_publication() {
    identity::refuses_unresolvable_selection(false);
}

#[cfg(unix)]
#[test]
fn host_missing_prefix_before_parent_component_refuses_before_publication() {
    identity::refuses_unresolvable_selection(true);
}
