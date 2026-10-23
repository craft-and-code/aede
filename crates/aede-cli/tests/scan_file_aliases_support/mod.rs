use aede_core::json::{self, Json};
use aede_core::model::{AudioFile, Catalog};
use aede_core::{conclusions, sources, store, user};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub struct Fixture {
    pub dir: PathBuf,
    pub root: PathBuf,
    pub target: PathBuf,
    pub kept: PathBuf,
    pub removed: PathBuf,
    pub kept_alias: PathBuf,
    pub removed_alias: PathBuf,
    data: PathBuf,
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
            "aede_file_aliases_{}_{nonce}_{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let root = dir.join("parent");
        let target = root.join("z-target");
        let kept = target.join("skip-neighbor").join(r"kept\native.flac");
        let removed = target.join("skip/removed.flac");
        let kept_alias = root.join(r"a-kept\native.flac");
        let removed_alias = root.join("b-removed.flac");
        let data = dir.join("data");
        let fixture = Self {
            dir,
            root,
            target,
            kept,
            removed,
            kept_alias,
            removed_alias,
            data,
        };
        for relative in [
            r"parent/z-target/skip-neighbor/kept\native.flac",
            "parent/z-target/skip/removed.flac",
            "parent/z-neighbor/live.flac",
        ] {
            fixture.track(relative);
        }
        std::os::unix::fs::symlink(
            Path::new("z-target")
                .join("skip-neighbor")
                .join(r"kept\native.flac"),
            &fixture.kept_alias,
        )
        .unwrap();
        std::os::unix::fs::symlink("z-target/skip/removed.flac", &fixture.removed_alias).unwrap();
        fixture
    }

    pub fn track(&self, relative: &str) -> PathBuf {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(audio_fixture("track.flac"), &path).unwrap();
        path
    }

    fn command(&self) -> Command {
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

    pub fn seed(&self, follow: bool) -> Catalog {
        let mut command = self.command();
        command
            .arg("scan")
            .arg(&self.root)
            .args(["--threads=1", "--json"]);
        if follow {
            command.arg("--follow-symlinks");
        }
        success(command.output().unwrap());
        let catalog = self.catalog();
        let mut expected = vec![
            self.kept.clone(),
            self.removed.clone(),
            self.root.join("z-neighbor/live.flac"),
        ];
        if follow {
            expected.extend([self.kept_alias.clone(), self.removed_alias.clone()]);
        }
        assert_paths(&catalog, &expected);
        self.assert_policy(&[]);
        catalog
    }

    pub fn full_scan(&self, follow: bool, unavailable: Option<&Path>) -> Output {
        let mut command = self.command();
        command.args(["scan", "--full", "--threads=1", "--json"]);
        if follow {
            command.arg("--follow-symlinks");
        }
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

    pub fn exclude(&self, excluded: &Path) {
        let output = self
            .command()
            .args(["roots", "--exclude"])
            .arg(excluded)
            .arg("--no-scan")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        self.assert_policy(&[excluded.to_path_buf()]);
    }

    pub fn assert_policy(&self, excluded: &[PathBuf]) {
        let catalog = self.catalog();
        assert_eq!(
            catalog.roots.iter().map(Path::new).collect::<Vec<_>>(),
            vec![self.root.as_path()]
        );
        assert_eq!(
            catalog.excluded.iter().map(Path::new).collect::<Vec<_>>(),
            excluded.iter().map(PathBuf::as_path).collect::<Vec<_>>()
        );
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

    pub fn replace_kept(&self) {
        std::fs::copy(audio_fixture("compilation.flac"), &self.kept).unwrap();
    }

    pub fn add_neighbor(&self) {
        self.track("parent/z-neighbor/added.flac");
    }

    pub fn expected(&self, follow: bool, partial: bool, whole: bool) -> Vec<PathBuf> {
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

    pub fn initialize_independent_stores(&self) {
        user::save(&Default::default(), &user::user_path(&self.data)).unwrap();
        conclusions::save(
            &Default::default(),
            &conclusions::conclusions_path(&self.data),
        )
        .unwrap();
        sources::save(&Default::default(), &sources::sources_path(&self.data)).unwrap();
    }

    pub fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
        [
            store::catalog_path(&self.data),
            user::user_path(&self.data),
            conclusions::conclusions_path(&self.data),
            sources::sources_path(&self.data),
        ]
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
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

fn audio_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/fixtures")
        .join(name)
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

pub fn count(report: &Json, key: &str) -> u64 {
    report.get("counts").unwrap().field_u64(key).unwrap()
}

pub fn assert_paths(catalog: &Catalog, expected: &[PathBuf]) {
    let paths: BTreeSet<&Path> = catalog
        .files
        .iter()
        .map(|file| Path::new(&file.path))
        .collect();
    assert_eq!(paths, expected.iter().map(PathBuf::as_path).collect());
    assert_eq!(catalog.files.len(), expected.len());
    assert_eq!(catalog.tracks.len(), expected.len());
}

pub fn file<'a>(catalog: &'a Catalog, path: &Path) -> &'a AudioFile {
    catalog
        .files
        .iter()
        .find(|file| Path::new(&file.path) == path)
        .unwrap_or_else(|| panic!("missing catalog file {path:?}"))
}

pub fn assert_cached(previous: &Catalog, current: &Catalog, path: &Path) {
    fn row(catalog: &Catalog, path: &Path) -> Json {
        let serialized = store::to_json(catalog);
        let mut row = serialized
            .get("file")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .find(|row| row.get("path").and_then(Json::as_str) == path.to_str())
            .unwrap_or_else(|| panic!("missing persisted file {path:?}"))
            .clone();
        // File IDs are positions in the rebuilt graph, not file identity.
        row.set("id", Json::Null);
        row
    }
    assert_eq!(row(current, path), row(previous, path));
    assert_eq!(
        file(current, path).integrity,
        file(previous, path).integrity
    );
    assert_eq!(
        file(current, path).fingerprint,
        file(previous, path).fingerprint
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
