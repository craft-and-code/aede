use super::*;

#[test]
fn processed_pcm_reports_distinct_pre_guard_sample_and_true_peaks() {
    let format = PcmFormat::new(48_000, 2).expect("format");
    let mut meter = OutputMeter::new(format).expect("meter");
    let mut samples = (0..4_800)
        .flat_map(|frame| {
            let value = (2.0 * std::f32::consts::PI * frame as f32 / 48.0).sin() * 0.5;
            [value, value]
        })
        .collect::<Vec<_>>();
    for block in samples.chunks_mut(960) {
        let sample_peak = block
            .iter()
            .fold(0.0_f32, |peak, value| peak.max(value.abs()));
        meter
            .observe(
                block,
                ProcessStats {
                    sample_peak,
                    overfull_samples: 0,
                },
            )
            .expect("observe");
    }
    let snapshot = meter.snapshot().expect("snapshot");
    assert_eq!(snapshot.frames, 4_800);
    assert!((snapshot.output_sample_peak - 0.5).abs() < 0.001);
    assert!(
        snapshot
            .output_true_peak
            .is_some_and(|peak| peak > 0.49 && peak < 0.55)
    );
    assert_eq!(snapshot.guarded_samples, 0);
}

#[test]
fn guard_intervention_and_high_rate_true_peak_limit_are_explicit() {
    let format = PcmFormat::new(192_000, 1).expect("format");
    let mut meter = OutputMeter::new(format).expect("meter");
    meter
        .observe(
            &[1.0, -0.5],
            ProcessStats {
                sample_peak: 1.4,
                overfull_samples: 1,
            },
        )
        .expect("observe");
    let snapshot = meter.snapshot().expect("snapshot");
    assert_eq!(snapshot.pre_guard_sample_peak, 1.4);
    assert_eq!(snapshot.output_sample_peak, 1.0);
    assert_eq!(snapshot.output_true_peak, None);
    assert_eq!(snapshot.guarded_samples, 1);
    assert_eq!(meter.sample_peak_snapshot(), snapshot);
}

#[test]
fn invalid_meter_block_leaves_previous_measurements_intact() {
    let format = PcmFormat::new(44_100, 2).expect("format");
    let mut meter = OutputMeter::new(format).expect("meter");
    assert!(matches!(
        meter.observe(
            &[f32::NAN, 0.0],
            ProcessStats {
                sample_peak: 0.0,
                overfull_samples: 0,
            }
        ),
        Err(OutputMeterError::InvalidBuffer)
    ));
    assert_eq!(meter.snapshot().expect("snapshot").frames, 0);
}

#[test]
fn low_rate_output_meter_is_refused_before_observation_or_snapshot() {
    for rate in [1, 15, 16, 17] {
        for channels in [1, 2] {
            let format = PcmFormat::new(rate, channels).expect("positive PCM format");
            assert!(
                matches!(
                    OutputMeter::new(format),
                    Err(OutputMeterError::Meter(ebur128::Error::NoMem))
                ),
                "rate {rate}, channels {channels}: construction must refuse an unsafe meter"
            );
        }
    }
}

#[test]
fn low_rate_sample_peak_fallback_snapshots_single_and_seven_frame_blocks_safely() {
    for rate in [16, 17] {
        for channels in [1, 2] {
            for frames in [1, 7] {
                let format = PcmFormat::new(rate, channels).expect("positive PCM format");
                let mut meter = OutputMeter::sample_peak_only(format);
                let samples = vec![-0.5; frames * usize::from(channels)];
                meter
                    .observe(
                        &samples,
                        ProcessStats {
                            sample_peak: 0.75,
                            overfull_samples: 0,
                        },
                    )
                    .expect("guarded fallback frames");
                let expected = OutputSnapshot {
                    frames: frames as u64,
                    pre_guard_sample_peak: 0.75,
                    output_sample_peak: 0.5,
                    output_true_peak: None,
                    guarded_samples: 0,
                };
                assert_eq!(meter.snapshot().expect("fallback snapshot"), expected);
                assert_eq!(meter.snapshot().expect("repeated snapshot"), expected);
                assert_eq!(meter.sample_peak_snapshot(), expected);
                meter
                    .observe(
                        &vec![1.0; usize::from(channels)],
                        ProcessStats {
                            sample_peak: 1.5,
                            overfull_samples: usize::from(channels),
                        },
                    )
                    .expect("continued guarded fallback capture");
                assert_eq!(
                    meter.snapshot().expect("continued fallback snapshot"),
                    OutputSnapshot {
                        frames: frames as u64 + 1,
                        pre_guard_sample_peak: 1.5,
                        output_sample_peak: 1.0,
                        output_true_peak: None,
                        guarded_samples: u64::from(channels),
                    }
                );
            }
        }
    }
}

#[test]
fn low_rate_sample_peak_fallback_refuses_invalid_buffers_without_changing_diagnostics() {
    for rate in [16, 17] {
        for channels in [1, 2] {
            let format = PcmFormat::new(rate, channels).expect("positive PCM format");
            let mut meter = OutputMeter::sample_peak_only(format);
            let samples = vec![0.5; usize::from(channels)];
            let stats = ProcessStats {
                sample_peak: 0.5,
                overfull_samples: 0,
            };
            meter.observe(&samples, stats).expect("valid prefix");
            let previous = meter.snapshot().expect("prefix snapshot");
            let mut invalid = vec![
                (Vec::new(), stats),
                (vec![f32::NAN; usize::from(channels)], stats),
                (vec![f32::INFINITY; usize::from(channels)], stats),
                (vec![1.25; usize::from(channels)], stats),
                (
                    samples.clone(),
                    ProcessStats {
                        sample_peak: 0.25,
                        overfull_samples: 0,
                    },
                ),
                (
                    samples.clone(),
                    ProcessStats {
                        sample_peak: f32::NAN,
                        overfull_samples: 0,
                    },
                ),
                (
                    samples.clone(),
                    ProcessStats {
                        sample_peak: 0.5,
                        overfull_samples: samples.len() + 1,
                    },
                ),
            ];
            if channels == 2 {
                invalid.push((vec![0.0], stats));
            }
            for (block, invalid_stats) in invalid {
                assert!(matches!(
                    meter.observe(&block, invalid_stats),
                    Err(OutputMeterError::InvalidBuffer)
                ));
                assert_eq!(meter.snapshot().expect("unchanged snapshot"), previous);
            }
        }
    }
}

#[test]
fn supported_rates_snapshot_single_and_seven_frame_output_blocks_safely() {
    for rate in [18, 8_000, 44_100, 48_000, 192_000, 384_000] {
        for channels in [1, 2] {
            for frames in [1, 7] {
                let format = PcmFormat::new(rate, channels).expect("supported PCM format");
                let mut meter = OutputMeter::new(format).expect("supported output meter");
                meter
                    .observe(
                        &vec![0.0; frames * usize::from(channels)],
                        ProcessStats {
                            sample_peak: 0.0,
                            overfull_samples: 0,
                        },
                    )
                    .expect("supported silent frames");
                let expected = OutputSnapshot {
                    frames: frames as u64,
                    pre_guard_sample_peak: 0.0,
                    output_sample_peak: 0.0,
                    output_true_peak: (rate < 192_000).then_some(0.0),
                    guarded_samples: 0,
                };
                assert_eq!(meter.snapshot().expect("supported snapshot"), expected);
                assert_eq!(meter.snapshot().expect("repeated snapshot"), expected);
            }
        }
    }
}
