use super::*;
use crate::accounts_test_support::Fixture;
use aede_core::store;
const GROUP: &str = "12345678-1234-1234-1234-123456789abc";
struct Fake {
    calls: usize,
    url: &'static str,
    bytes: &'static [u8],
}
impl Archive for Fake {
    fn index(&mut self, _: &str) -> Result<Option<Json>, String> {
        self.calls += 1;
        Ok(Some(
            aede_core::json::parse(&format!(
                r#"{{"images":[{{"front":true,"approved":true,"image":"{}"}}]}}"#,
                self.url
            ))
            .unwrap(),
        ))
    }
    fn image(&mut self, _: &str) -> Result<Vec<u8>, String> {
        self.calls += 1;
        Ok(self.bytes.to_vec())
    }
}
#[test]
fn missing_covers_cache_validated_images_and_refuse_invalid_targets_without_touching_music() {
    let fixture = Fixture::new();
    let catalog = std::fs::read(store::catalog_path(&fixture.0.data_dir)).unwrap();
    let mut archive = Fake {
        calls: 0,
        url: "https://archive.org/download/cover.png",
        bytes: include_bytes!("../../aede-core/tests/fixtures/images/rgba.png"),
    };
    assert!(
        obtain(&fixture.0.data_dir, GROUP, &mut archive)
            .unwrap()
            .is_some()
    );
    assert!(
        obtain(&fixture.0.data_dir, GROUP, &mut archive)
            .unwrap()
            .is_some()
    );
    assert_eq!(archive.calls, 2);
    assert_eq!(
        std::fs::read(store::catalog_path(&fixture.0.data_dir)).unwrap(),
        catalog
    );
    assert!(cached(&fixture.0.data_dir, "../private").is_err());
    let other = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    archive.url = "https://localhost/private";
    assert!(obtain(&fixture.0.data_dir, other, &mut archive).is_err());
    assert_eq!(archive.calls, 3);
    archive.url = "https://archive.org/download/cover.png";
    archive.bytes = b"broken PNG";
    assert!(obtain(&fixture.0.data_dir, other, &mut archive).is_err());
    assert!(cached(&fixture.0.data_dir, other).unwrap().is_none());
}
#[cfg(unix)]
#[test]
fn missing_cover_cache_rejects_symlinked_directories_and_files() {
    let fixture = Fixture::new();
    let root = fixture.0.data_dir.join("missing-coverartarchive");
    std::os::unix::fs::symlink(std::env::temp_dir(), &root).unwrap();
    assert!(cached(&fixture.0.data_dir, GROUP).is_err());
    std::fs::remove_file(&root).unwrap();
    std::fs::create_dir_all(root.join(GROUP)).unwrap();
    std::os::unix::fs::symlink(
        store::catalog_path(&fixture.0.data_dir),
        root.join(GROUP).join("cover.png"),
    )
    .unwrap();
    assert!(cached(&fixture.0.data_dir, GROUP).is_err());
}
