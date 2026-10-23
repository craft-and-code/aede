use super::*;

pub(super) struct AliasFixture {
    pub(super) sandbox: Sandbox,
    pub(super) root: PathBuf,
    pub(super) target: PathBuf,
    pub(super) alias: PathBuf,
}

impl AliasFixture {
    pub(super) fn new() -> Self {
        let sandbox = canonical_sandbox();
        sandbox.track(r"parent/z-target/kept\native.flac");
        sandbox.track("parent/z-target/excluded/removed.flac");
        sandbox.track("parent/z-target/excluded-neighbor/kept.flac");
        sandbox.track("parent/z-neighbor/live.flac");
        let root = sandbox.0.join("parent");
        let target = root.join("z-target");
        let alias = root.join("a-alias");
        std::os::unix::fs::symlink("z-target", &alias).unwrap();
        Self {
            sandbox,
            root,
            target,
            alias,
        }
    }

    pub(super) fn seed(&self, follow_symlinks: bool) -> Catalog {
        let options = ScanOptions {
            follow_symlinks,
            threads: 1,
            ..Default::default()
        };
        let (catalog, report) =
            scan(std::slice::from_ref(&self.root), None, &options, |_| {}).unwrap();
        let published = if follow_symlinks {
            &self.alias
        } else {
            &self.target
        };
        assert_paths(
            &catalog,
            &[
                published.join(r"kept\native.flac"),
                published.join("excluded/removed.flac"),
                published.join("excluded-neighbor/kept.flac"),
                self.root.join("z-neighbor/live.flac"),
            ],
        );
        assert!(report.failures.is_empty());
        catalog
    }

    pub(super) fn detach(&self) {
        std::fs::rename(&self.target, self.sandbox.0.join("detached-target")).unwrap();
    }

    pub(super) fn remount(&self) {
        std::fs::rename(self.sandbox.0.join("detached-target"), &self.target).unwrap();
    }

    pub(super) fn expected_partial(&self, published: &Path) -> Vec<PathBuf> {
        vec![
            published.join(r"kept\native.flac"),
            published.join("excluded-neighbor/kept.flac"),
            self.root.join("z-neighbor/live.flac"),
            self.root.join("z-neighbor/added.flac"),
        ]
    }

    pub(super) fn add_neighbor(&self) {
        self.sandbox.track("parent/z-neighbor/added.flac");
    }
}
