use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-extra/ramp.ape")
}
struct Copy(PathBuf);
impl Copy {
    fn new(bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "aede-ape-pcm16-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, bytes).unwrap();
        Self(path)
    }
    fn open(&self) -> Result<ApePcm16Decoder> {
        ApePcm16Decoder::open(File::open(&self.0).unwrap())
    }
}
impl Drop for Copy {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn exact(mut decoder: ApePcm16Decoder) {
    assert_eq!(decoder.info().frames, Some(132300));
    for start in [73727, 73728, 73729, 132299, 0, 588, 132300, 90000] {
        decoder.seek(start).unwrap();
        let mut at = start;
        while let Some(bytes) = decoder.next_chunk().unwrap() {
            assert!(bytes.len() <= 2 * 1_048_576);
            for frame in bytes.as_chunks::<4>().0 {
                let left = (at % 30001) as i16 - 15000;
                assert_eq!(
                    *frame,
                    [left.to_le_bytes(), (-left).to_le_bytes()]
                        .concat()
                        .as_slice()
                );
                at += 1;
            }
        }
        assert_eq!(at, 132300);
        assert!(decoder.next_chunk().unwrap().is_none());
    }
}
#[test]
fn original_pcm16_survives_exact_seeks_across_compressed_frames_and_eof() {
    exact(ApePcm16Decoder::open(File::open(fixture()).unwrap()).unwrap());
}
#[test]
fn bounded_prefixes_keep_relative_seek_offsets_and_original_pcm() {
    let original = fs::read(fixture()).unwrap();
    let mut tagged = b"ID3\x04\0\0\0\0\0\x20".to_vec();
    tagged.extend_from_slice(&[0; 32]);
    // Apparent APE magic in a tag payload is not the descriptor.
    tagged[10..14].copy_from_slice(b"MAC ");
    for prefix in [b"ID3\x04\0\0\0\0\0\0".to_vec(), vec![0; 65537], tagged] {
        let file = Copy::new(&[prefix, original.clone()].concat());
        exact(file.open().unwrap());
    }
}
#[test]
fn invalid_seek_retires_the_reader() {
    let mut reader = ApePcm16Decoder::open(File::open(fixture()).unwrap()).unwrap();
    assert!(reader.seek(132301).is_err());
    assert!(reader.seek(0).is_err());
    assert!(reader.next_chunk().is_err());
}
#[test]
fn admission_refuses_legacy_versions_precision_and_unbounded_prefixes() {
    let source = fs::read(fixture()).unwrap();
    let mut legacy = source.clone();
    legacy[4..6].copy_from_slice(&3970u16.to_le_bytes());
    assert!(
        Copy::new(&legacy)
            .open()
            .err()
            .unwrap()
            .to_string()
            .contains("unsupported APE version")
    );
    let mut wide = source.clone();
    wide[68..70].copy_from_slice(&24u16.to_le_bytes());
    assert!(Copy::new(&wide).open().is_err());
    assert!(
        Copy::new(&[vec![0; 1024 * 1024 + 1], source].concat())
            .open()
            .is_err()
    );
}
#[test]
fn corrupt_pcm_retires_the_reader_without_publishing_partial_bytes() {
    let mut bytes = fs::read(fixture()).unwrap();
    bytes[300] ^= 0x7f;
    let file = Copy::new(&bytes);
    let before = fs::read(&file.0).unwrap();
    let mut reader = file.open().unwrap();
    assert!(reader.next_chunk().is_err());
    assert!(reader.next_chunk().is_err());
    assert!(reader.seek(73728).is_err());
    assert_eq!(fs::read(&file.0).unwrap(), before);
}
#[cfg(unix)]
#[test]
fn decoding_uses_the_opened_handle_after_path_replacement() {
    let file = Copy::new(&fs::read(fixture()).unwrap());
    let decoder = file.open().unwrap();
    fs::remove_file(&file.0).unwrap();
    fs::write(&file.0, [0; 100]).unwrap();
    exact(decoder);
}
