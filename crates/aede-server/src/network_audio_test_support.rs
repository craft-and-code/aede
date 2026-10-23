//! Test-only admission for existing network supervision fixtures.

pub(super) fn selected(path: std::path::PathBuf) -> aede_devices::SelectedSource {
    let metadata = std::fs::metadata(&path).unwrap();
    let expected = aede_core::user::PlaybackSource::from_metadata(&metadata).unwrap();
    aede_devices::SelectedSource::from_expected(path, &expected).unwrap()
}
