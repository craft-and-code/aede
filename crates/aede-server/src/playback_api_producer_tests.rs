use super::*;
use aede_dsp::Dsp;

use crate::accounts_test_support::Fixture;
use crate::playback_test_support::{additional_flac, install_wav};
use crate::test_support::test_runtime;

async fn producer_events(
    fixture: &Fixture,
    references: &[&str],
    position_ms: u64,
    interactive: bool,
) -> Vec<ProducerEvent> {
    let references: Vec<_> = references
        .iter()
        .map(|reference| EntityRef::parse_token(reference).unwrap())
        .collect();
    let (sources, catalog) = current_sources(&fixture.0, &references).await.unwrap();
    let settings = ProducerSettings {
        queue: true,
        output_rate: Some(8_000),
        tone: ToneControls::new(0.0, 0.0).unwrap(),
        normalize: Mode::Off,
        normalization_catalog: catalog,
        data_dir: fixture.0.data_dir.clone(),
        first_position_ms: position_ms,
        interactive,
    };
    let (mut receiver, worker) = spawn_producer(
        sources,
        settings,
        std::sync::Arc::new(AtomicBool::new(false)),
    );
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    worker.await.unwrap();
    events
}

#[test]
fn remote_seek_discards_only_the_first_source_prefix_and_reports_its_actual_position() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 4_000).await;
        let events = producer_events(
            &fixture,
            &[&installed.reference, &installed.reference],
            125,
            true,
        )
        .await;
        let mut audio = Vec::new();
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        for event in events {
            match event {
                ProducerEvent::Format(format) => assert_eq!(format.position_ms, Some(125)),
                ProducerEvent::Track(track) => starts.push(track),
                ProducerEvent::TrackEnd(track) => ends.push(track),
                ProducerEvent::Audio { bytes, .. } => audio.extend_from_slice(&bytes),
                ProducerEvent::Eof { frames } => assert_eq!(frames, 7_000),
                ProducerEvent::Error(failure) => panic!("unexpected failure: {}", failure.code),
            }
        }
        assert_eq!(starts.len(), 2);
        assert_eq!(starts[0].start_frame, 0);
        assert_eq!(starts[0].position_ms, Some(125));
        assert_eq!(starts[1].start_frame, 3_000);
        assert_eq!(starts[1].position_ms, Some(0));
        assert_eq!(ends.len(), 2);
        assert_eq!(ends[0].end_frame, 3_000);
        assert_eq!(ends[1].end_frame, 7_000);
        let mut source = PcmTrack::open(&installed.path).unwrap();
        let mut full = Vec::new();
        while let Some(block) = source.read_block(|_| Ok::<(), Infallible>(())).unwrap() {
            full.extend_from_slice(block.f32le);
        }
        let mut expected = full[1_000 * 4..].to_vec();
        expected.extend_from_slice(&full);
        assert_eq!(audio, expected, "skipped PCM never enters output or DSP");
    });
}

#[test]
fn remote_seek_accepts_exact_eof_but_refuses_a_position_beyond_decoded_audio() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 4_000).await;
        let events = producer_events(&fixture, &[&installed.reference], 500, true).await;
        let mut starts = 0;
        let mut ends = 0;
        let mut eof = 0;
        for event in events {
            match event {
                ProducerEvent::Format(format) => assert_eq!(format.position_ms, Some(500)),
                ProducerEvent::Track(track) => {
                    starts += 1;
                    assert_eq!(track.position_ms, Some(500));
                    assert_eq!(track.start_frame, 0);
                }
                ProducerEvent::TrackEnd(track) => {
                    ends += 1;
                    assert_eq!(track.end_frame, 0);
                }
                ProducerEvent::Eof { frames } => {
                    eof += 1;
                    assert_eq!(frames, 0);
                }
                ProducerEvent::Audio { .. } => panic!("an EOF seek must emit no audio"),
                ProducerEvent::Error(failure) => panic!("unexpected failure: {}", failure.code),
            }
        }
        assert_eq!((starts, ends, eof), (1, 1, 1));
        let events = producer_events(&fixture, &[&installed.reference], 501, true).await;
        assert_eq!(events.len(), 1, "invalid seek fails before any new format");
        assert!(matches!(
            events.first(),
            Some(ProducerEvent::Error(failure)) if failure.code == "invalid_seek"
        ));
    });
}

#[test]
fn remote_seek_does_not_bypass_decoded_flac_md5_errors_in_the_discarded_prefix() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        install_wav(&fixture, 8).await;
        let installed = additional_flac(&fixture, "incorrect-md5.flac", true).await;
        let events = producer_events(&fixture, &[&installed.reference], 60_000, true).await;
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.first(),
            Some(ProducerEvent::Error(failure)) if failure.code == "decode_failed"
        ));
    });
}

#[test]
fn finite_legacy_production_keeps_position_metadata_absent() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 8).await;
        let events = producer_events(&fixture, &[&installed.reference], 0, false).await;
        let mut audio_frames = 0;
        let mut eof = None;
        for event in events {
            match event {
                ProducerEvent::Format(format) => {
                    assert_eq!(format.position_ms, None);
                    assert!(
                        serde_json::to_value(format)
                            .unwrap()
                            .get("position_ms")
                            .is_none()
                    );
                }
                ProducerEvent::Track(track) => {
                    assert_eq!(track.position_ms, None);
                    assert!(
                        serde_json::to_value(track)
                            .unwrap()
                            .get("position_ms")
                            .is_none()
                    );
                }
                ProducerEvent::Audio { frames, .. } => audio_frames += frames,
                ProducerEvent::Eof { frames } => eof = Some(frames),
                ProducerEvent::TrackEnd(_) => {}
                ProducerEvent::Error(failure) => panic!("unexpected failure: {}", failure.code),
            }
        }
        assert_eq!(audio_frames, 8);
        assert_eq!(eof, Some(8));
    });
}

#[test]
fn peak_limited_normalization_keeps_the_tone_headroom_reserve() {
    let tone = ToneControls::new(6.0, 0.0).unwrap();
    let format = PcmFormat::new(48_000, 1).unwrap();
    for source_peak in [None, Some(1.0)] {
        let selected = GainPlan {
            gain_db: 10.0,
            source_peak,
            label: "test normalization",
            peak_label: "test peak",
        };
        let gain = output_gain(Some(selected), tone).ok().unwrap();
        assert_eq!(
            gain, -6.0,
            "the normalization cap must not consume the EQ reserve"
        );
        let mut dsp = Dsp::new(format);
        dsp.set_gain_db(gain, 0).unwrap();
        dsp.set_tone(tone).unwrap();
        let mut low_frequency = vec![0.95; 48_000];
        let stats = dsp.process_for_output(&mut low_frequency).unwrap();
        assert_eq!(
            stats.overfull_samples, 0,
            "reserved bass headroom avoids clipping"
        );
    }
    assert_eq!(output_gain(None, tone).ok(), Some(-6.0));
}
