use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aede-root-identity-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn catalog_with_files(root: &Path, paths: &[PathBuf]) -> Catalog {
    aede_core::model::build(
        paths
            .iter()
            .map(|path| aede_core::model::ScannedFile {
                path: path.to_str().unwrap().into(),
                size: 10,
                mtime: 20,
                tags: Default::default(),
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            })
            .collect(),
        vec![root.to_str().unwrap().into()],
        30,
        &[],
    )
}

#[test]
fn absent_root_keeps_the_canonical_spelling_of_its_existing_parent() {
    let fixture = Fixture::new();
    let hop = fixture.0.join("hop");
    fs::create_dir(&hop).unwrap();
    let root = hop.join("..").join("absent").join("child");
    assert_eq!(
        resolve(&root).unwrap(),
        fixture.0.join("absent").join("child")
    );
}

#[cfg(unix)]
#[test]
fn a_parent_component_is_applied_after_resolving_the_existing_link() {
    let fixture = Fixture::new();
    let branch = fixture.0.join("branch");
    fs::create_dir_all(branch.join("pivot")).unwrap();
    let alias = fixture.0.join("alias");
    std::os::unix::fs::symlink("branch/pivot", &alias).unwrap();
    let root = alias.join("..").join("absent");
    assert_eq!(resolve(&root).unwrap(), branch.join("absent"));
}

#[cfg(unix)]
#[test]
fn dangling_link_resolution_has_an_explicit_finite_link_budget() {
    let fixture = Fixture::new();
    for index in (0..=MAX_LINKS).rev() {
        let target = if index == MAX_LINKS {
            PathBuf::from("absent")
        } else {
            PathBuf::from(format!("alias-{}", index + 1))
        };
        std::os::unix::fs::symlink(target, fixture.0.join(format!("alias-{index}"))).unwrap();
    }
    assert_eq!(
        resolve(&fixture.0.join("alias-1")).unwrap(),
        fixture.0.join("absent")
    );
    let error = resolve(&fixture.0.join("alias-0")).unwrap_err().to_string();
    assert!(error.contains("too many symbolic links"), "{error}");
}

#[cfg(unix)]
#[test]
fn a_link_cycle_is_refused_with_actionable_identity_guidance() {
    let fixture = Fixture::new();
    let alias = fixture.0.join("alias");
    std::os::unix::fs::symlink("alias", &alias).unwrap();
    let error = resolve(&alias).unwrap_err().to_string();
    assert!(error.contains("identity"), "{error}");
    assert!(error.contains("reconnect"), "{error}");
    assert!(error.contains("too many symbolic links"), "{error}");
}

#[test]
fn an_absent_prefix_cannot_be_flattened_through_a_parent_component() {
    let fixture = Fixture::new();
    let root = fixture.0.join("absent").join("..").join("other");
    let error = resolve(&root).unwrap_err().to_string();
    assert!(
        error.contains("parent of an absent component is unknown"),
        "{error}"
    );
}

#[test]
fn a_file_cannot_be_flattened_through_a_parent_component() {
    let fixture = Fixture::new();
    let file = fixture.0.join("file");
    fs::write(&file, b"fixture").unwrap();
    let error = resolve(&file.join("..").join("absent"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("not a directory"), "{error}");
}

#[test]
fn an_unknown_unavailable_identity_cannot_replace_an_old_root() {
    let fixture = Fixture::new();
    let previous = Catalog {
        roots: vec![fixture.0.join("old").to_str().unwrap().into()],
        ..Default::default()
    };
    let missing = fixture.0.join("unknown");
    let error = validate_selection(
        Some(&previous),
        std::slice::from_ref(&missing),
        &[(missing.clone(), missing.clone())],
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("previous root association is unknown"),
        "{error}"
    );
}

#[test]
fn canonical_unavailable_identity_allows_an_intentional_root_removal() {
    let fixture = Fixture::new();
    let retained = fixture.0.join("retained");
    let previous = Catalog {
        roots: vec![
            retained.to_str().unwrap().into(),
            fixture.0.join("removed").to_str().unwrap().into(),
        ],
        ..Default::default()
    };
    validate_selection(
        Some(&previous),
        std::slice::from_ref(&retained),
        &[(retained.clone(), retained.clone())],
    )
    .unwrap();
}

#[test]
fn a_new_missing_root_is_safe_when_every_old_root_remains_selected() {
    let fixture = Fixture::new();
    let old = fixture.0.join("old");
    let missing = fixture.0.join("new");
    let previous = Catalog {
        roots: vec![old.to_str().unwrap().into()],
        ..Default::default()
    };
    validate_selection(
        Some(&previous),
        &[old, missing.clone()],
        &[(missing.clone(), missing)],
    )
    .unwrap();
}

#[test]
fn an_unavailable_alias_cannot_claim_another_known_root_after_retargeting() {
    let fixture = Fixture::new();
    let other = fixture.0.join("other");
    let previous = Catalog {
        roots: vec![
            fixture.0.join("old").to_str().unwrap().into(),
            other.to_str().unwrap().into(),
        ],
        ..Default::default()
    };
    let error = validate_selection(
        Some(&previous),
        std::slice::from_ref(&other),
        &[(fixture.0.join("alias"), other.clone())],
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("previous root association is unknown"),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn an_unrepresentable_dangling_link_target_is_refused_losslessly() {
    use std::os::unix::ffi::OsStringExt;
    let fixture = Fixture::new();
    let alias = fixture.0.join("alias");
    std::os::unix::fs::symlink(OsString::from_vec(vec![b'x', 0xff]), &alias).unwrap();
    let error = resolve(&alias).unwrap_err().to_string();
    assert!(error.contains("cannot be represented as UTF-8"), "{error}");
}

#[test]
fn an_ambiguous_unavailable_alias_cannot_remove_files_beneath_a_retained_parent() {
    let fixture = Fixture::new();
    let parent = fixture.0.join("parent");
    let old = catalog_with_files(&parent, &[parent.join("real").join("retained.wav")]);
    let current = catalog_with_files(&parent, &[parent.join("added.wav")]);
    let unknown = fixture.0.join("alias");
    let error = validate_result(Some(&old), &current, &[(unknown.clone(), unknown)], &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("remove files"), "{error}");
    assert!(error.contains("identity"), "{error}");
}

#[test]
fn a_new_missing_root_allows_results_that_retain_every_previous_file() {
    let fixture = Fixture::new();
    let parent = fixture.0.join("parent");
    let retained = parent.join("retained.wav");
    let old = catalog_with_files(&parent, std::slice::from_ref(&retained));
    let current = catalog_with_files(&parent, &[retained, parent.join("added.wav")]);
    let unknown = fixture.0.join("new-missing-root");
    validate_result(Some(&old), &current, &[(unknown.clone(), unknown)], &[]).unwrap();
}

#[test]
fn explicit_exclusions_win_over_ambiguous_unavailable_retention() {
    let fixture = Fixture::new();
    let parent = fixture.0.join("parent");
    let excluded = parent.join("excluded");
    let old = catalog_with_files(&parent, &[excluded.join("removed.wav")]);
    let current = catalog_with_files(&parent, &[]);
    let unknown = fixture.0.join("alias");
    let unavailable = [(unknown.clone(), unknown)];
    validate_result(
        Some(&old),
        &current,
        &unavailable,
        std::slice::from_ref(&excluded),
    )
    .unwrap();
    let neighbor = catalog_with_files(
        &parent,
        &[parent.join("excluded-neighbor").join("retained.wav")],
    );
    assert!(validate_result(Some(&neighbor), &current, &unavailable, &[excluded]).is_err());
}

#[test]
fn a_known_canonical_unavailable_root_allows_intentionally_deselected_files_to_leave() {
    let fixture = Fixture::new();
    let retained = fixture.0.join("retained-root");
    let removed = fixture.0.join("removed-root");
    let mut old = catalog_with_files(
        &fixture.0,
        &[retained.join("retained.wav"), removed.join("removed.wav")],
    );
    old.roots = vec![
        retained.to_str().unwrap().into(),
        removed.to_str().unwrap().into(),
    ];
    let current = catalog_with_files(&retained, &[retained.join("retained.wav")]);
    validate_result(Some(&old), &current, &[(retained.clone(), retained)], &[]).unwrap();
}
