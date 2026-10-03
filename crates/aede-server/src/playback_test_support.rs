//! Test-only WAV catalog fixtures shared by playback transport tests.

use std::path::PathBuf;

use aede_core::clock;
use aede_core::model::EntityKind;
use aede_core::store;
use aede_core::tags::AudioProperties;
use aede_core::user::EntityRef;

use crate::accounts_test_support::Fixture;

pub(super) struct InstalledTrack {
    pub(super) reference: String,
    pub(super) path: PathBuf,
}

pub(super) async fn install_wav(fixture: &Fixture, frames: u32) -> InstalledTrack {
    let path = fixture.0.data_dir.join("remote-playback.wav");
    std::fs::write(&path, wav_bytes(frames)).expect("write WAV fixture");
    let metadata = std::fs::metadata(&path).expect("WAV metadata");
    let path_text = path.to_string_lossy().into_owned();
    let mut guard = fixture.0.catalog.write().await;
    let catalog = guard.as_mut().expect("fixture catalog");
    let file = catalog.files.first_mut().expect("sample file");
    file.path = path_text.clone();
    file.size = metadata.len();
    file.mtime = clock::mtime_seconds(&metadata);
    file.properties = AudioProperties {
        codec: "pcm".into(),
        container: "wav".into(),
        sample_rate: Some(8_000),
        bit_depth: Some(16),
        channels: Some(1),
        duration_ms: Some(u64::from(frames) * 1_000 / 8_000),
        bitrate_kbps: Some(128),
        lossless: true,
    };
    catalog
        .file_mtime_subseconds
        .insert(path_text, clock::mtime_subseconds(&metadata));
    let reference = EntityRef::of(catalog, EntityKind::Track, 0)
        .expect("fixture track reference")
        .to_token();
    store::save_catalog_only(catalog, &store::catalog_path(&fixture.0.data_dir))
        .expect("save fixture catalog");
    InstalledTrack { reference, path }
}

pub(super) fn wav_bytes(frames: u32) -> Vec<u8> {
    let bytes_per_frame = 2_u32;
    let data_size = frames * bytes_per_frame;
    let mut bytes = Vec::with_capacity(44 + data_size as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&8_000_u32.to_le_bytes());
    bytes.extend_from_slice(&(8_000_u32 * bytes_per_frame).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    for index in 0..frames {
        let sample = if index.is_multiple_of(2) {
            8_000_i16
        } else {
            -8_000_i16
        };
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}
