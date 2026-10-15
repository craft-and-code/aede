use super::*;
#[test]
fn real_flac_uses_existing_decoder_resampler_without_rewriting_audio() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/playback_fixtures/flac/playback-stereo.flac");
    let before = std::fs::read(&path).unwrap();
    let mut checkpoints = 0;
    let value = extract(&path, || {
        checkpoints += 1;
        Ok(())
    })
    .unwrap();
    assert!(value.valid());
    assert!(checkpoints > 1);
    assert_eq!(before, std::fs::read(path).unwrap());
}
#[test]
fn interrupted_extraction_never_finishes_a_partial_source() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/playback_fixtures/flac/playback-stereo.flac");
    assert!(extract(&path, || Err("cancelled".into())).is_err());
}
