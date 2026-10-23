use super::*;

pub(super) struct FileAliasFixture {
    pub(super) sandbox: Sandbox,
    pub(super) root: PathBuf,
    pub(super) target: PathBuf,
    pub(super) kept: PathBuf,
    pub(super) removed: PathBuf,
    pub(super) kept_alias: PathBuf,
    pub(super) removed_alias: PathBuf,
}

impl FileAliasFixture {
    pub(super) fn new() -> Self {
        let sandbox = canonical_sandbox();
        let kept = sandbox.track(r"parent/z-target/skip-neighbor/kept\native.flac");
        let removed = sandbox.track("parent/z-target/skip/removed.flac");
        sandbox.track("parent/z-neighbor/live.flac");
        let root = sandbox.0.join("parent");
        let target = root.join("z-target");
        let kept_alias = root.join(r"a-kept\native.flac");
        let removed_alias = root.join("b-removed.flac");
        std::os::unix::fs::symlink(
            Path::new("z-target")
                .join("skip-neighbor")
                .join(r"kept\native.flac"),
            &kept_alias,
        )
        .unwrap();
        std::os::unix::fs::symlink("z-target/skip/removed.flac", &removed_alias).unwrap();
        Self {
            sandbox,
            root,
            target,
            kept,
            removed,
            kept_alias,
            removed_alias,
        }
    }

    pub(super) fn seed(&self, follow_symlinks: bool) -> Catalog {
        let (catalog, report) = scan(
            std::slice::from_ref(&self.root),
            None,
            &ScanOptions {
                follow_symlinks,
                threads: 1,
                ..Default::default()
            },
            |_| {},
        )
        .unwrap();
        let mut expected = vec![
            self.kept.clone(),
            self.removed.clone(),
            self.root.join("z-neighbor/live.flac"),
        ];
        if follow_symlinks {
            expected.extend([self.kept_alias.clone(), self.removed_alias.clone()]);
        }
        assert_paths(&catalog, &expected);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        catalog
    }

    pub(super) fn scan(&self, previous: &Catalog, options: &ScanOptions) -> (Catalog, ScanReport) {
        scan(
            std::slice::from_ref(&self.root),
            Some(previous),
            options,
            |_| {},
        )
        .unwrap()
    }

    pub(super) fn options(&self, declared_offline: bool, excluded: Vec<PathBuf>) -> ScanOptions {
        ScanOptions {
            follow_symlinks: true,
            force_read: true,
            threads: 1,
            unavailable_roots: if declared_offline {
                vec![self.target.clone()]
            } else {
                Vec::new()
            },
            excluded,
            ..Default::default()
        }
    }

    pub(super) fn detach(&self) {
        std::fs::rename(&self.target, self.sandbox.0.join("detached-target")).unwrap();
    }

    pub(super) fn remount(&self) {
        std::fs::rename(self.sandbox.0.join("detached-target"), &self.target).unwrap();
    }

    pub(super) fn replace_kept(&self) {
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compilation.flac"),
            &self.kept,
        )
        .unwrap();
    }

    pub(super) fn add_neighbor(&self) {
        self.sandbox.track("parent/z-neighbor/added.flac");
    }

    pub(super) fn expected(&self, follow: bool, partial: bool, whole: bool) -> Vec<PathBuf> {
        let mut paths = vec![
            self.root.join("z-neighbor/live.flac"),
            self.root.join("z-neighbor/added.flac"),
        ];
        if !whole {
            paths.push(self.kept.clone());
            if follow {
                paths.push(self.kept_alias.clone());
            }
            if !partial {
                paths.push(self.removed.clone());
                if follow {
                    paths.push(self.removed_alias.clone());
                }
            }
        }
        paths
    }

    pub(super) fn assert_cached(&self, previous: &Catalog, current: &Catalog, partial: bool) {
        let mut paths = vec![&self.kept, &self.kept_alias];
        if !partial {
            paths.extend([&self.removed, &self.removed_alias]);
        }
        for path in paths {
            assert_cached_file(previous, current, path);
            let old = file(previous, path);
            let new = file(current, path);
            assert_eq!(new.properties, old.properties);
            assert_eq!(new.has_embedded_art, old.has_embedded_art);
            assert_eq!(new.lyrics_path, old.lyrics_path);
            assert_eq!(new.integrity, old.integrity);
            assert_eq!(new.fingerprint, old.fingerprint);
        }
    }
}

pub(super) fn file<'a>(catalog: &'a Catalog, path: &Path) -> &'a model::AudioFile {
    catalog
        .files
        .iter()
        .find(|file| Path::new(&file.path) == path)
        .unwrap_or_else(|| panic!("missing catalog file {path:?}"))
}

pub(super) fn assert_removed(report: &ScanReport, expected: &[PathBuf]) {
    let mut paths: Vec<&Path> = report.removed_paths.iter().map(Path::new).collect();
    paths.sort();
    let mut expected: Vec<&Path> = expected.iter().map(PathBuf::as_path).collect();
    expected.sort();
    assert_eq!(paths, expected);
    assert_eq!(report.removed, expected.len());
}
