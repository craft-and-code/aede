use aede_core::json::{self, Json};
use aede_core::model::Catalog;
use aede_core::{conclusions, sources, store, user};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub struct Fixture {
    pub dir: PathBuf,
    pub root: PathBuf,
    pub target: PathBuf,
    pub alias: PathBuf,
    pub data: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "aede_alias_exclusions_{}_{nonce}_{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let root = dir.join("parent");
        let target = root.join("z-target");
        let alias = root.join("a-alias");
        let data = dir.join("data");
        let fixture = Self {
            dir,
            root,
            target,
            alias,
            data,
        };
        for relative in [
            r"parent/z-target/kept\native.flac",
            "parent/z-target/excluded/removed.flac",
            "parent/z-target/excluded-neighbor/kept.flac",
            "parent/z-neighbor/live.flac",
        ] {
            fixture.track(relative);
        }
        std::os::unix::fs::symlink("z-target", &fixture.alias).unwrap();
        fixture
    }

    pub fn track(&self, relative: &str) -> PathBuf {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
            &path,
        )
        .unwrap();
        path
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aede"));
        command.arg("--data").arg(&self.data).env("NO_COLOR", "1");
        for key in [
            "AEDE_DESKTOP_LEASE",
            "AEDE_DELEGATED_CHILD",
            "AEDE_DELEGATED_DATA_DIR",
            "AEDE_UNAVAILABLE_ROOTS",
            "AEDE_ADMIN_TOKEN",
        ] {
            command.env_remove(key);
        }
        command
    }

    pub fn seed(&self, follow: bool, selected_alias: Option<&Path>) -> Catalog {
        let mut command = self.command();
        command
            .arg("scan")
            .arg(&self.root)
            .args(["--threads=1", "--json"]);
        if follow {
            command.arg("--follow-symlinks");
        }
        if let Some(alias) = selected_alias {
            command.arg(alias);
        }
        success(command.output().unwrap());
        let catalog = self.catalog();
        let published = if follow { &self.alias } else { &self.target };
        assert_paths(
            &catalog,
            &[
                published.join(r"kept\native.flac"),
                published.join("excluded/removed.flac"),
                published.join("excluded-neighbor/kept.flac"),
                self.root.join("z-neighbor/live.flac"),
            ],
        );
        catalog
    }

    pub fn exclude(&self, path: &Path) {
        let output = self
            .command()
            .args(["roots", "--exclude"])
            .arg(path)
            .arg("--no-scan")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        self.assert_exclusion(path);
    }

    pub fn assert_exclusion(&self, expected: &Path) {
        let catalog = self.catalog();
        assert_eq!(
            catalog.excluded.iter().map(Path::new).collect::<Vec<_>>(),
            vec![expected]
        );
        assert_eq!(
            catalog.roots.iter().map(Path::new).collect::<Vec<_>>(),
            vec![self.root.as_path()]
        );
    }

    pub fn full_scan(&self, unavailable: Option<&Path>) -> Output {
        let mut command = self.command();
        command.args([
            "scan",
            "--follow-symlinks",
            "--full",
            "--threads=1",
            "--json",
        ]);
        if let Some(path) = unavailable {
            command.arg("--replace").arg(&self.root);
            let roots = Json::Arr(vec![Json::Str(path.to_str().unwrap().into())]);
            command
                .env("AEDE_DELEGATED_CHILD", "1")
                .env("AEDE_DELEGATED_DATA_DIR", &self.data)
                .env("AEDE_UNAVAILABLE_ROOTS", roots.to_string_compact());
        }
        command.output().unwrap()
    }

    pub fn catalog(&self) -> Catalog {
        store::load(&store::catalog_path(&self.data))
            .unwrap()
            .unwrap()
    }

    pub fn detach(&self) {
        std::fs::rename(&self.target, self.dir.join("detached-target")).unwrap();
    }

    pub fn remount(&self) {
        std::fs::rename(self.dir.join("detached-target"), &self.target).unwrap();
    }

    pub fn expected_partial(&self) -> Vec<PathBuf> {
        vec![
            self.alias.join(r"kept\native.flac"),
            self.alias.join("excluded-neighbor/kept.flac"),
            self.root.join("z-neighbor/live.flac"),
            self.root.join("z-neighbor/added.flac"),
        ]
    }

    pub fn initialize_independent_stores(&self) {
        user::save(&Default::default(), &user::user_path(&self.data)).unwrap();
        conclusions::save(
            &Default::default(),
            &conclusions::conclusions_path(&self.data),
        )
        .unwrap();
        sources::save(&Default::default(), &sources::sources_path(&self.data)).unwrap();
    }

    pub fn snapshot(&self) -> Vec<(PathBuf, Option<Vec<u8>>)> {
        [
            store::catalog_path(&self.data),
            user::user_path(&self.data),
            conclusions::conclusions_path(&self.data),
            sources::sources_path(&self.data),
        ]
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).ok();
            (path, bytes)
        })
        .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn success(output: Output) -> Json {
    assert!(
        output.status.success(),
        "status: {}; stderr: {}; stdout: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    json::parse(&String::from_utf8(output.stdout).unwrap()).unwrap()
}

pub fn assert_paths(catalog: &Catalog, expected: &[PathBuf]) {
    let paths: BTreeSet<&Path> = catalog
        .files
        .iter()
        .map(|file| Path::new(&file.path))
        .collect();
    assert_eq!(paths, expected.iter().map(PathBuf::as_path).collect());
    assert_eq!(catalog.tracks.len(), expected.len());
}

pub fn assert_cached(previous: &Catalog, current: &Catalog, path: &Path) {
    let before = previous
        .files
        .iter()
        .find(|file| Path::new(&file.path) == path)
        .unwrap();
    let after = current
        .files
        .iter()
        .find(|file| Path::new(&file.path) == path)
        .unwrap();
    assert_eq!(after.path, before.path, "stored spelling must survive");
    assert_eq!(after.size, before.size);
    assert_eq!(after.mtime, before.mtime);
    assert_eq!(after.tags, before.tags);
    assert_eq!(after.has_embedded_art, before.has_embedded_art);
    assert_eq!(after.lyrics_path, before.lyrics_path);
    assert_eq!(
        current.file_mtime_subseconds.get(&after.path),
        previous.file_mtime_subseconds.get(&before.path)
    );
    assert_eq!(
        current.file_added_at.get(&after.path),
        previous.file_added_at.get(&before.path)
    );
}

pub fn assert_identity_refusal(output: &Output) {
    assert!(!output.status.success(), "unexpected publication");
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostic.contains("identity"), "{diagnostic}");
    assert!(
        diagnostic.contains("reconnect") || diagnostic.contains("select"),
        "{diagnostic}"
    );
}
