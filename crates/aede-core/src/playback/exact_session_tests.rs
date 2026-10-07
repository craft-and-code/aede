use std::collections::BTreeMap;
use std::path::PathBuf;

use aede_dsp::{ChannelLayout, OutputMeter, PcmFormat};
use md5::{Digest, Md5};

use super::{ExactPcmSession, ExactSessionBlock, ExactSessionError};
use crate::playback::decoder::{FlacMd5Status, IntegerFileDecoder};
use crate::playback::exact_output::{
    ExactOutputError, ExactOutputFormat, ExactSampleRepresentation as Representation,
    MAX_EXACT_BLOCK_FRAMES,
};
use crate::playback::format::IntegerPcmFormat;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(if name.starts_with("playback") {
            "tests/playback_fixtures/flac"
        } else {
            "tests/fixtures"
        })
        .join(name)
}

fn input(bits: u32, layout: ChannelLayout) -> IntegerPcmFormat {
    IntegerPcmFormat::new(48000, layout, bits).expect("source format")
}

fn output(input: IntegerPcmFormat, representation: Representation) -> ExactOutputFormat {
    ExactOutputFormat::new(
        input.sample_rate(),
        input.layout(),
        representation,
        Some(32),
    )
    .expect("output format")
}

#[derive(Default)]
struct Received {
    samples: Vec<i32>,
    bytes: Vec<u8>,
    frames: BTreeMap<usize, usize>,
    completed: Vec<usize>,
}

impl Received {
    fn collect(&mut self, block: ExactSessionBlock<'_>, channels: usize, width: usize) {
        assert_eq!(block.encoded.len(), block.samples.len() * width);
        let mut offset = 0;
        for span in block.spans {
            assert_eq!(span.samples.start, offset);
            assert!(span.samples.end >= offset);
            assert_eq!(span.samples.len() % channels, 0);
            *self.frames.entry(span.token).or_default() += span.samples.len() / channels;
            assert_eq!(span.stats.overfull_samples, 0);
            if span.complete {
                self.completed.push(span.token);
            }
            offset = span.samples.end;
        }
        assert_eq!(offset, block.samples.len());
        self.samples.extend_from_slice(block.samples);
        self.bytes.extend_from_slice(block.encoded);
    }
}

fn render(tracks: &[(usize, &[i32])], chunk_frames: usize) -> Received {
    let format = input(24, ChannelLayout::STEREO);
    let mut session = ExactPcmSession::new(format, output(format, Representation::Signed32Le))
        .expect("exact session");
    let mut received = Received::default();
    for &(token, samples) in tracks {
        session.begin_track(token).expect("begin");
        for chunk in samples.chunks(chunk_frames * 2) {
            received.collect(session.push_source(chunk).expect("source"), 2, 4);
        }
        received.collect(session.end_track().expect("end"), 2, 4);
    }
    received.collect(session.finish().expect("finish"), 2, 4);
    let repeated = session.finish().expect("finish again");
    assert!(repeated.samples.is_empty());
    assert!(repeated.encoded.is_empty());
    assert!(repeated.spans.is_empty());
    received
}

#[test]
fn successive_repeated_occurrences_preserve_low_bits_order_and_unique_completions() {
    let first = [-8388608, 8388607, -1, 1, 256, -257, 0, 17];
    let next = [12345, -54321, 7, -9];
    let tracks = [
        (4, first.as_slice()),
        (5, next.as_slice()),
        (6, first.as_slice()),
    ];
    for chunks in [1, 3, MAX_EXACT_BLOCK_FRAMES] {
        let received = render(&tracks, chunks);
        assert_eq!(received.samples, [first.as_slice(), &next, &first].concat());
        assert_eq!(received.completed, [4, 5, 6]);
        assert_eq!(received.frames, BTreeMap::from([(4, 4), (5, 2), (6, 4)]));
        assert_eq!(
            received.bytes[..16],
            [0, 0, 0, 128, 0, 255, 255, 127, 0, 255, 255, 255, 0, 1, 0, 0]
        );
        if chunks != 1 {
            assert_eq!(received.bytes, render(&tracks, 1).bytes);
        }
    }
}

#[test]
fn empty_tracks_and_empty_blocks_emit_only_one_completion_per_occurrence() {
    let format = input(16, ChannelLayout::MONO);
    let mut session =
        ExactPcmSession::new(format, output(format, Representation::Float32Le)).expect("session");
    let mut received = Received::default();
    for token in [0, 1, 2] {
        session.begin_track(token).expect("begin");
        let empty = session.push_source(&[]).expect("empty source");
        assert!(empty.spans.is_empty());
        received.collect(empty, 1, 4);
        received.collect(session.end_track().expect("empty end"), 1, 4);
    }
    received.collect(session.finish().expect("finish"), 1, 4);
    assert_eq!(received.completed, [0, 1, 2]);
    assert!(received.samples.is_empty());
    assert!(received.bytes.is_empty());
    assert_eq!(session.source_frames(), 0);
}

#[test]
fn invalid_source_blocks_leave_the_previous_output_and_frame_position_unchanged() {
    let format = input(24, ChannelLayout::STEREO);
    let mut session =
        ExactPcmSession::new(format, output(format, Representation::Signed32Le)).expect("session");
    session.begin_track(19).expect("begin");
    session
        .push_source(&[-8388608, 8388607])
        .expect("valid source");
    let saved_samples = session.samples.clone();
    let saved_bytes = session.bytes.clone();
    let saved_spans = session.spans.clone();
    let oversized = vec![0; (MAX_EXACT_BLOCK_FRAMES + 1) * 2];
    for bad in [
        &[1][..],
        &[3, 8388608],
        &[-8388609, 7],
        oversized.as_slice(),
    ] {
        assert!(matches!(
            session.push_source(bad),
            Err(ExactSessionError::Output(_))
        ));
        assert_eq!(session.source_frames(), 1);
        assert_eq!(session.samples, saved_samples);
        assert_eq!(session.bytes, saved_bytes);
        assert_eq!(session.spans, saved_spans);
    }
    assert_eq!(
        session.push_source(&[1, -1]).expect("resume").samples,
        [1, -1]
    );
    assert_eq!(session.source_frames(), 2);
    assert!(session.end_track().expect("end").spans[0].complete);
}

#[test]
fn lifecycle_refusals_do_not_seal_or_advance_an_active_occurrence() {
    let format = input(16, ChannelLayout::MONO);
    let sink = output(format, Representation::Signed32Le);
    let mut session = ExactPcmSession::new(format, sink).expect("session");
    assert!(matches!(
        session.push_source(&[1]),
        Err(ExactSessionError::NoTrack)
    ));
    assert!(matches!(
        session.end_track(),
        Err(ExactSessionError::NoTrack)
    ));
    session.begin_track(7).expect("begin");
    session.push_source(&[32767, -32768]).expect("source");
    assert!(matches!(
        session.begin_track(8),
        Err(ExactSessionError::TrackActive)
    ));
    assert!(matches!(
        session.finish(),
        Err(ExactSessionError::TrackActive)
    ));
    assert_eq!(session.source_frames(), 2);
    assert_eq!(session.spans[0].token, 7);
    assert!(!session.spans[0].complete);
    assert_eq!(session.end_track().expect("end").spans[0].token, 7);
    session.finish().expect("finish");
    assert!(matches!(
        session.begin_track(9),
        Err(ExactSessionError::Finished)
    ));
    assert!(matches!(
        session.push_source(&[1]),
        Err(ExactSessionError::Finished)
    ));
    assert!(matches!(
        session.end_track(),
        Err(ExactSessionError::Finished)
    ));
    assert!(!session.compatible(format, sink));
}

#[test]
fn cumulative_frame_overflow_refuses_audio_without_emitting_an_extra_frame() {
    let format = input(16, ChannelLayout::MONO);
    let mut session =
        ExactPcmSession::new(format, output(format, Representation::Signed32Le)).expect("session");
    session.begin_track(0).expect("begin");
    session.push_source(&[1]).expect("source");
    session.source_frames = u64::MAX;
    let saved_bytes = session.bytes.clone();
    assert!(matches!(
        session.push_source(&[2]),
        Err(ExactSessionError::FrameCountOverflow)
    ));
    assert_eq!(session.source_frames(), u64::MAX);
    assert_eq!(session.samples, [1]);
    assert_eq!(session.bytes, saved_bytes);
    assert!(!session.spans[0].complete);
}

#[test]
fn compatibility_includes_source_depth_rate_layout_and_output_representation() {
    let format = input(24, ChannelLayout::STEREO);
    let sink = output(format, Representation::Signed32Le);
    let session = ExactPcmSession::new(format, sink).expect("session");
    assert_eq!(session.input_format(), format);
    assert_eq!(session.output_format(), sink);
    assert!(session.compatible(format, sink));
    for changed in [
        input(16, ChannelLayout::STEREO),
        input(24, ChannelLayout::MONO),
        IntegerPcmFormat::new(44100, ChannelLayout::STEREO, 24).expect("different rate"),
    ] {
        assert!(!session.compatible(changed, sink));
    }
    assert!(!session.compatible(format, output(format, Representation::Float32Le)));
    let unknown = ExactOutputFormat::new(
        48000,
        ChannelLayout::STEREO,
        Representation::Float32Le,
        None,
    )
    .expect("unknown precision descriptor");
    assert!(matches!(
        ExactPcmSession::new(format, unknown),
        Err(ExactSessionError::Output(
            ExactOutputError::UnknownEffectivePrecision
        ))
    ));
}

#[test]
fn observed_peaks_work_with_the_existing_meter_without_modifying_source_pcm() {
    for bits in [16, 24] {
        let format = input(bits, ChannelLayout::STEREO);
        let mut session = ExactPcmSession::new(format, output(format, Representation::Float32Le))
            .expect("session");
        session.begin_track(0).expect("begin");
        let full = 1_i32 << (bits - 1);
        let source = [-full, full - 1, 1, -1, 13, -17];
        let block = session.push_source(&source).expect("source");
        assert_eq!(block.samples, source);
        assert_eq!(block.spans[0].stats.sample_peak, 1.0);
        assert_eq!(block.spans[0].stats.overfull_samples, 0);
        let observed: Vec<_> = block
            .encoded
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&v| f32::from_le_bytes(v))
            .collect();
        let mut meter = OutputMeter::new(
            PcmFormat::with_layout(48000, ChannelLayout::STEREO).expect("meter format"),
        )
        .expect("meter");
        meter
            .observe(&observed, block.spans[0].stats)
            .expect("same observation contract");
        assert_eq!(block.samples, source);
    }
}

#[test]
fn real_sources_match_independent_pcm_digests_through_decode_and_exact_session() {
    let fixtures = [
        ("track.flac", "8f523032fd7d2d351b19bf2c32666373"),
        ("playback-stereo.flac", "7e2d2157e4982d903552e068d16a097d"),
        ("playback-real24.flac", "094731fdef7e6d68c1ee38926308ddba"),
        ("playback-padded24.flac", "50da7e50ca791bc6be7e1ca80ef96570"),
        ("track.wav", "4e741314017bcfd9a7012a9c128a703a"),
    ];
    for (name, digest) in fixtures {
        let path = fixture(name);
        let mut decoder = IntegerFileDecoder::open(&path).expect("fixture");
        let format = decoder.format();
        let representation = if format.bits_per_sample() == 16 {
            Representation::Signed16Le
        } else {
            Representation::PackedSigned24Le
        };
        let sink = ExactOutputFormat::new(
            format.sample_rate(),
            format.layout(),
            representation,
            Some(format.bits_per_sample()),
        )
        .expect("same-depth output");
        let mut session = ExactPcmSession::new(format, sink).expect("session");
        session.begin_track(23).expect("begin");
        let channels = usize::from(format.channels());
        let mut source = vec![0; 37 * channels];
        let mut received = Received::default();
        loop {
            let frames = decoder.read_frames(&mut source).expect("decode");
            if frames == 0 {
                break;
            }
            let block = session
                .push_source(&source[..frames * channels])
                .expect("preserve");
            assert_eq!(block.samples, &source[..frames * channels]);
            received.collect(block, channels, representation.bytes_per_sample());
        }
        if name.ends_with(".flac") {
            assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Verified));
        }
        received.collect(
            session.end_track().expect("verified EOF"),
            channels,
            representation.bytes_per_sample(),
        );
        received.collect(
            session.finish().expect("finish"),
            channels,
            representation.bytes_per_sample(),
        );
        assert_eq!(
            format!("{:x}", Md5::digest(&received.bytes)),
            digest,
            "{name}"
        );
        assert_eq!(received.completed, [23]);
        assert_eq!(received.frames[&23] as u64, session.source_frames());
    }
}

#[test]
fn a_fresh_session_after_seeking_carries_only_the_requested_integer_suffix() {
    let path = fixture("playback-real24.flac");
    let mut original = IntegerFileDecoder::open(&path).expect("fixture");
    let format = original.format();
    let mut all = Vec::new();
    let channels = usize::from(format.channels());
    let mut buffer = vec![0; 101 * channels];
    loop {
        let frames = original.read_frames(&mut buffer).expect("reference decode");
        if frames == 0 {
            break;
        }
        all.extend_from_slice(&buffer[..frames * channels]);
    }
    let mut decoder = IntegerFileDecoder::open(&path).expect("reopened source");
    let skipped = decoder.skip_frames(4097, || false).expect("seek");
    assert_eq!(skipped.frames, 4097);
    assert!(!skipped.reached_eof);
    let mut session = ExactPcmSession::new(format, output(format, Representation::Signed32Le))
        .expect("fresh session");
    session.begin_track(1).expect("resumed occurrence");
    let mut received = Received::default();
    loop {
        let frames = decoder.read_frames(&mut buffer).expect("suffix decode");
        if frames == 0 {
            break;
        }
        received.collect(
            session
                .push_source(&buffer[..frames * channels])
                .expect("suffix"),
            channels,
            4,
        );
    }
    assert_eq!(received.samples, all[4097 * channels..]);
    assert_eq!(
        session.source_frames() as usize,
        all.len() / channels - 4097
    );
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Verified));
    assert!(session.end_track().expect("end").spans[0].complete);
}
