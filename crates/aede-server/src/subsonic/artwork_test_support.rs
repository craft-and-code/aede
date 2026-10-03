//! Shared validated image bytes and a real isolated catalogued sidecar.

use crate::accounts_test_support::Fixture;
use crate::subsonic::{artwork, authentication, parameters::Parameters, test_support};

pub(in crate::subsonic) const PNG: &[u8] =
    include_bytes!("../../../aede-core/tests/fixtures/images/rgba.png");
pub(in crate::subsonic) const JPEG: &[u8] =
    include_bytes!("../../../aede-core/tests/fixtures/images/baseline.jpg");
pub(in crate::subsonic) const LARGE_PNG: &[u8] =
    include_bytes!("../../../aede-core/tests/fixtures/images/interlaced.png");

pub(in crate::subsonic) async fn install(
    fixture: &Fixture,
    bytes: &[u8],
) -> (String, std::path::PathBuf) {
    let folder = fixture.0.data_dir.join("album");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("cover.png");
    std::fs::write(&path, bytes).unwrap();
    let mut guard = fixture.0.catalog.write().await;
    let catalog = guard.as_mut().unwrap();
    let release = &mut catalog.releases[0];
    release.folder = folder.to_str().unwrap().into();
    release.key = "cover-fixture".into();
    release.cover_path = Some(path.to_str().unwrap().into());
    let id = artwork::cover_id(catalog, &catalog.releases[0])
        .unwrap()
        .unwrap();
    (id, path)
}

pub(super) async fn identity(fixture: &Fixture, username: &str) -> authentication::Identity {
    let token = test_support::key(fixture, username);
    authentication::authenticate(&fixture.0, &Parameters::from_pairs(&[("apiKey", &token)]))
        .await
        .unwrap()
}

pub(super) fn parameters(id: &str, size: Option<u32>) -> Parameters {
    match size {
        None => Parameters::from_pairs(&[("id", id)]),
        Some(size) => Parameters::from_pairs(&[("id", id), ("size", &size.to_string())]),
    }
}
