use super::*;
fn tone(hz: f32, seconds: usize, gain: f32, stereo: bool) -> Vec<f32> {
    (0..RATE as usize * seconds)
        .flat_map(|n| {
            let sample = gain * (std::f32::consts::TAU * hz * n as f32 / RATE as f32).sin();
            if stereo {
                vec![sample, -sample]
            } else {
                vec![sample]
            }
        })
        .collect()
}
fn analyze(samples: &[f32], channels: usize, chunk: usize) -> Features {
    let mut read = Extractor::new(channels).unwrap();
    for packet in samples.chunks(chunk * channels) {
        read.push(packet, || Ok(())).unwrap();
    }
    read.finish().unwrap()
}
#[test]
fn streaming_packets_preserve_features_and_antiphase_stereo_is_not_silence() {
    let mono = tone(440., 2, 0.5, false);
    let small = analyze(&mono, 1, 37);
    let whole = analyze(&mono, 1, 4096);
    assert_eq!(small, whole);
    let stereo = analyze(&tone(440., 2, 0.5, true), 2, 137);
    assert!((stereo.centroid_hz.mean - 440.).abs() < 12.);
    assert!((small.centroid_hz.mean - stereo.centroid_hz.mean).abs() < 0.01);
    assert!(stereo.chroma[9] > 0.7);
    assert!(stereo.tempo_bpm.is_none());
}
#[test]
fn silence_unknown_tempo_and_nonfinite_audio_are_not_fabricated_measurements() {
    let silent = analyze(&vec![0.; RATE as usize], 1, 128);
    assert!(silent.tempo_bpm.is_none());
    assert!(!silent.usable());
    assert!(Extractor::new(0).is_err());
    assert!(Extractor::new(3).is_err());
    let mut read = Extractor::new(2).unwrap();
    assert!(read.push(&[0.], || Ok(())).is_err());
    assert!(read.push(&[f32::NAN, 0.], || Ok(())).is_err());
}
#[test]
fn rhythmic_pulses_produce_a_bounded_tempo_estimate_without_gain_bias() {
    let pulse = |gain| {
        (0..RATE as usize * 18)
            .map(|n| {
                let local = n % (RATE as usize / 2);
                let envelope = if local < RATE as usize / 15 {
                    1. - local as f32 / (RATE as f32 / 15.)
                } else {
                    0.
                };
                gain * envelope * (std::f32::consts::TAU * 440. * n as f32 / RATE as f32).sin()
            })
            .collect::<Vec<_>>()
    };
    let a = analyze(&pulse(0.5), 1, 4096);
    let b = analyze(&pulse(0.05), 1, 4096);
    assert!(
        (a.tempo_bpm.unwrap() - 120.).abs() < 5.,
        "{:?}",
        a.tempo_bpm
    );
    assert!((a.tempo_bpm.unwrap() - b.tempo_bpm.unwrap()).abs() < 1.);
}
#[test]
fn extraction_honours_cancellation_before_more_spectral_work() {
    let mut read = Extractor::new(1).unwrap();
    let mut calls = 0;
    let result = read.push(&tone(440., 1, 0.1, false), || {
        calls += 1;
        Err("cancelled".into())
    });
    assert!(result.is_err());
    assert_eq!(calls, 1);
}

#[test]
fn measured_selections_preserve_missing_rhythm_and_use_explicit_boundaries() {
    let mut features = analyze(&tone(440., 2, 0.5, false), 1, 4096);
    let recipes = audio_recipes();
    assert!(recipes[0].measure(&features).is_none());
    assert!(recipes[2].measure(&features).is_some());
    assert!(recipes[3].measure(&features).is_none());
    features.tempo_bpm = Some(90.);
    features.periodicity = Some(0.8);
    assert_eq!(recipes[0].measure(&features), Some((90., "bpm")));
    features.tempo_bpm = Some(130.);
    assert!(recipes[0].measure(&features).is_none());
    assert_eq!(recipes[1].measure(&features), Some((130., "bpm")));
    features.rms_dbfs = None;
    assert!(
        recipes
            .iter()
            .all(|recipe| recipe.measure(&features).is_none())
    );
}
