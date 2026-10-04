use super::{SPECTRUM_BANDS, Spectrum};
use crate::PcmFormat;

fn tone(hz: f32, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|index| (std::f32::consts::TAU * hz * index as f32 / 48_000.0).sin() * 0.5)
        .collect()
}

#[test]
fn silence_produces_twenty_four_empty_bands() {
    assert_eq!(SPECTRUM_BANDS, 24);
    let mut spectrum = Spectrum::new(PcmFormat::new(48_000, 1).unwrap());
    let levels = spectrum
        .push(&vec![0.0; 2048])
        .unwrap()
        .expect("one window");
    assert_eq!(levels, [0.0; SPECTRUM_BANDS]);
}

#[test]
fn bass_and_treble_move_different_bars() {
    let mut bass = Spectrum::new(PcmFormat::new(48_000, 1).unwrap());
    let mut treble = Spectrum::new(PcmFormat::new(48_000, 1).unwrap());
    let low = bass.push(&tone(95.0, 2048)).unwrap().expect("bass window");
    let high = treble
        .push(&tone(4000.0, 2048))
        .unwrap()
        .expect("treble window");
    let low_peak = low
        .iter()
        .position(|v| *v == low.iter().copied().fold(0.0, f32::max))
        .unwrap();
    let high_peak = high
        .iter()
        .position(|v| *v == high.iter().copied().fold(0.0, f32::max))
        .unwrap();
    assert!(low_peak < 4, "bass peak at {low_peak}");
    assert!(high_peak > 5, "treble peak at {high_peak}");
    assert!(low.iter().all(|v| (0.0..=1.0).contains(v)));
    assert!(high.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn analysis_keeps_its_window_across_pcm_blocks() {
    let mut spectrum = Spectrum::new(PcmFormat::new(48_000, 2).unwrap());
    let stereo = tone(400.0, 2048)
        .into_iter()
        .flat_map(|sample| [sample, -sample])
        .collect::<Vec<_>>();
    assert!(spectrum.push(&stereo[..1000]).unwrap().is_none());
    let levels = spectrum
        .push(&stereo[1000..])
        .unwrap()
        .expect("completed window");
    assert!(levels.iter().any(|level| *level > 0.0));
}

#[test]
fn rejected_frames_do_not_replace_or_advance_a_partial_analysis_window() {
    let format = PcmFormat::new(48_000, 2).expect("stereo");
    let samples = tone(400.0, 2_048)
        .into_iter()
        .flat_map(|sample| [sample, -sample])
        .collect::<Vec<_>>();
    let mut spectrum = Spectrum::new(format);
    let mut reference = Spectrum::new(format);
    assert!(spectrum.push(&samples[..1_000]).expect("prefix").is_none());
    assert_eq!(
        spectrum.push(&[0.25]),
        Err(crate::DspError::IncompleteFrame)
    );
    assert_eq!(
        spectrum.push(&[0.25, f32::INFINITY]),
        Err(crate::DspError::NonFiniteSample)
    );
    assert_eq!(
        spectrum.push(&samples[1_000..]).expect("remaining signal"),
        reference.push(&samples).expect("reference signal")
    );
}
