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
