#![cfg(feature = "engine-tests")]

#[path = "support/nested_roots.rs"]
mod nested_support;
#[path = "support/unavailable_roots.rs"]
#[allow(dead_code)]
mod support;

use nested_support::{assert_retained_identity, assert_selected_roots};
use std::{fs, path::Path};
use support::{Fixture, assert_catalog, available_port, completed_scan, write_wav};

#[test]
fn selected_parent_and_child_retain_offline_child_then_refresh_without_restart() {
    let fixture = Fixture::new();
    let parent = fixture.0.join("parent");
    let child = parent.join("child");
    fs::create_dir_all(&child).unwrap();
    let retained = child.join("retained.wav");
    write_wav(&retained);
    let port = available_port();
    let mut host = fixture.host();
    host.configure(
        port,
        vec![
            parent.to_str().unwrap().into(),
            child.to_str().unwrap().into(),
        ],
    )
    .unwrap();
    host.start().unwrap();
    let connection = host.connection_profile().unwrap();
    completed_scan(&mut host);
    let initial = fixture.catalog();
    assert_catalog(&mut host, &initial, std::slice::from_ref(&retained));

    let detached = fixture.0.join("detached-child");
    fs::rename(&child, &detached).unwrap();
    let added = parent.join("z-added.wav");
    write_wav(&added);
    completed_scan(&mut host);
    let offline = fixture.catalog();
    assert_catalog(&mut host, &offline, &[retained.clone(), added.clone()]);
    assert_retained_identity(&initial, &offline, &retained);
    assert_selected_roots(&mut host, &[(&parent, true), (&child, false)]);
    assert_eq!(host.connection_profile().unwrap(), connection);

    fs::remove_file(detached.join("retained.wav")).unwrap();
    write_wav(&detached.join("returned.wav"));
    fs::rename(&detached, &child).unwrap();
    completed_scan(&mut host);
    let returned = fixture.catalog();
    assert_catalog(
        &mut host,
        &returned,
        &[child.join("returned.wav"), added.clone()],
    );
    assert_selected_roots(&mut host, &[(&parent, true), (&child, true)]);
    assert_eq!(host.connection_profile().unwrap(), connection);

    fs::rename(&child, &detached).unwrap();
    host.configure(port, vec![parent.to_str().unwrap().into()])
        .unwrap();
    completed_scan(&mut host);
    let removed = fixture.catalog();
    assert_catalog(&mut host, &removed, &[added]);
    assert_selected_roots(&mut host, &[(&parent, true)]);
    assert!(
        removed["file"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| !Path::new(file["path"].as_str().unwrap()).starts_with(&child))
    );
    assert_eq!(host.connection_profile().unwrap(), connection);
    host.stop().unwrap();
}
