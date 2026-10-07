use super::*;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(if name.starts_with("playback") {
            "../aede-core/tests/playback_fixtures/flac"
        } else {
            "../aede-core/tests/fixtures"
        })
        .join(name)
}

fn reference(name: &str) -> (IntegerPcmFormat, Vec<i32>) {
    let mut decoder = IntegerFileDecoder::open(&fixture(name)).unwrap();
    let format = decoder.format();
    let mut block = vec![0; 37 * usize::from(format.channels())];
    let mut result = Vec::new();
    loop {
        let frames = decoder.read_frames(&mut block).unwrap();
        if frames == 0 {
            break;
        }
        result.extend_from_slice(&block[..frames * usize::from(format.channels())]);
    }
    (format, result)
}

#[test]
fn exact_source_session_preserves_real_pcm_and_observes_only_a_separate_view() {
    for name in [
        "track.wav",
        "track.flac",
        "playback-real24.flac",
        "playback-stereo.flac",
    ] {
        let (format, expected) = reference(name);
        let mut track = SourceTrack::open(&fixture(name), true).unwrap();
        let route = ExactOutputFormat::new(
            format.sample_rate(),
            format.layout(),
            ExactSampleRepresentation::Signed32Le,
            Some(24),
        )
        .unwrap();
        let mut processing = Processing::exact(format, route).unwrap();
        processing.begin_track(7, 0.0).unwrap();
        let mut actual = Vec::new();
        let mut observer_calls = 0;
        while let Some(raw) = track
            .read_block_observed(|_, _| observer_calls += 1)
            .unwrap()
        {
            assert!(raw.frames() <= BLOCK_FRAMES);
            let block = processing.push_source(raw).unwrap();
            let integers = block.integers.unwrap();
            actual.extend_from_slice(integers);
            assert_eq!(block.bytes.len(), integers.len() * 4);
            for ((integer, observed), bytes) in integers
                .iter()
                .zip(block.samples)
                .zip(block.bytes.as_chunks::<4>().0)
            {
                assert_eq!(
                    *observed,
                    *integer as f32 / (1_u32 << (format.bits_per_sample() - 1)) as f32
                );
                assert_eq!(
                    i32::from_le_bytes(*bytes),
                    integer << (32 - format.bits_per_sample())
                );
            }
            assert_eq!(block.spans.len(), 1);
            assert_eq!(block.spans[0].token, 7);
            assert_eq!(block.spans[0].stats.overfull_samples, 0);
        }
        assert_eq!(observer_calls, 0);
        assert_eq!(actual, expected, "{name}");
        let eof = processing.end_track().unwrap();
        assert!(eof.samples.is_empty());
        assert!(eof.spans[0].complete);
    }
}

#[test]
fn exact_seeking_discards_only_the_requested_source_prefix() {
    let name = "playback-stereo.flac";
    let (format, expected) = reference(name);
    let mut track = SourceTrack::open(&fixture(name), true).unwrap();
    let seek = track
        .seek_from_start(Duration::from_millis(10), || false)
        .unwrap();
    let mut actual = Vec::new();
    while let Some(SourceBlock::Exact { samples, .. }) = track
        .read_block_observed(|_, _| panic!("strict observer"))
        .unwrap()
    {
        actual.extend_from_slice(samples);
    }
    assert_eq!(seek.frames, u64::from(format.sample_rate()) / 100);
    assert_eq!(
        actual,
        expected[seek.frames as usize * usize::from(format.channels())..]
    );
}

#[test]
fn session_reuse_includes_source_depth_and_rejects_gain_or_wrong_source_policy() {
    let track = SourceTrack::open(&fixture("playback-real24.flac"), true).unwrap();
    let format = track.integer_format().unwrap();
    let route = ExactOutputFormat::new(
        format.sample_rate(),
        format.layout(),
        ExactSampleRepresentation::Signed32Le,
        Some(24),
    )
    .unwrap();
    let mut processing = Processing::exact(format, route).unwrap();
    assert!(processing.compatible(&track));
    assert!(
        !processing
            .compatible(&SourceTrack::open(&fixture("playback-real24.flac"), false).unwrap())
    );
    assert!(processing.begin_track(1, 1.0).is_err());
    processing.begin_track(1, 0.0).unwrap();
    assert!(
        processing
            .push_source(SourceBlock::Processed {
                samples: &[0.0],
                frames: 1
            })
            .is_err()
    );
}
