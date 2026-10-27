use super::*;
use symphonia::core::formats::Packet;

#[test]
fn native_codec_panics_become_errors_and_retire_before_another_publication() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-alac/stereo24.m4a");
    let mut format = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            MediaSourceStream::new(Box::new(File::open(path).unwrap()), Default::default()),
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .unwrap()
        .format;
    let params = format.default_track().unwrap().codec_params.clone();
    let packet = format.next_packet().unwrap();
    let mut codec = make_decoder(&params, &DecoderOptions::default()).unwrap();
    assert!(codec.decode(&packet).unwrap().frames() > 0);
    let mut data = packet.data.to_vec();
    // An impossible decoded extent triggers a bounds check in all build profiles.
    data[2] ^= 16;
    let corrupt = Packet::new_from_boxed_slice(
        packet.track_id(),
        packet.ts() + packet.dur(),
        packet.dur(),
        data.into_boxed_slice(),
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        codec.decode(&corrupt).map(|b| b.frames())
    }));
    assert!(
        result.is_ok(),
        "a malformed ALAC packet must not unwind into its caller"
    );
    assert!(result.unwrap().is_err());
    assert_eq!(codec.last_decoded().frames(), 0);
    assert!(codec.decode(&packet).is_err());
    codec.reset();
    assert!(
        codec.decode(&packet).is_err(),
        "panic retirement requires a new codec"
    );
    assert_eq!(codec.finalize().verify_ok, Some(false));
}

#[test]
fn alac_media_coverage_survives_reset_but_cannot_certify_an_absent_tail() {
    fn source() -> Box<dyn FormatReader> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-alac/stereo24-clock48k.m4a");
        symphonia::default::get_probe()
            .format(
                &Hint::new(),
                MediaSourceStream::new(Box::new(File::open(path).unwrap()), Default::default()),
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .unwrap()
            .format
    }
    let mut format = source();
    let params = format.default_track().unwrap().codec_params.clone();
    let first = format.next_packet().unwrap();
    let mut retired = make_decoder(&params, &DecoderOptions::default()).unwrap();
    assert!(retired.decode(&first).is_ok());
    assert_eq!(retired.finalize().verify_ok, Some(false));
    assert_eq!(retired.last_decoded().frames(), 0);
    retired.reset();
    assert!(retired.decode(&first).is_err());

    let mut codec = make_decoder(&params, &DecoderOptions::default()).unwrap();
    assert!(codec.decode(&first).is_ok());
    codec.reset();
    let mut format = source();
    let mut frames = 0;
    while let Ok(packet) = format.next_packet() {
        frames += codec.decode(&packet).unwrap().frames();
    }
    assert_eq!(frames, 48013);
    assert_eq!(codec.finalize().verify_ok, Some(true));
}

#[test]
fn alac_packet_gaps_overlaps_and_timestamp_overflow_retire_the_codec() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/native-alac/stereo24-clock48k.m4a");
    let mut format = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            MediaSourceStream::new(Box::new(File::open(path).unwrap()), Default::default()),
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .unwrap()
        .format;
    let params = format.default_track().unwrap().codec_params.clone();
    let first = format.next_packet().unwrap();
    for timestamp in [0, first.dur() + 1, u64::MAX] {
        let mut codec = make_decoder(&params, &DecoderOptions::default()).unwrap();
        assert!(codec.decode(&first).is_ok());
        let displaced = Packet::new_from_boxed_slice(
            first.track_id(),
            timestamp,
            first.dur(),
            first.data.clone(),
        );
        assert!(codec.decode(&displaced).is_err());
        assert_eq!(codec.last_decoded().frames(), 0);
        codec.reset();
        assert!(codec.decode(&first).is_err());
        assert_eq!(codec.finalize().verify_ok, Some(false));
    }
}
