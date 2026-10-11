use super::*;

fn sine(rate: u32, amplitude: f32) -> Vec<f32> {
    (0..rate)
        .map(|frame| {
            (std::f32::consts::TAU * 1000.0 * frame as f32 / rate as f32).sin() * amplitude
        })
        .collect()
}

#[test]
fn track_measurement_is_independent_of_pcm_block_size() {
    let format = PcmFormat::new(48_000, 1).unwrap();
    let samples = sine(48_000, 0.2);
    let mut whole = LoudnessProgramme::new();
    whole.push(format, &samples).unwrap();
    let mut chunked = LoudnessProgramme::new();
    for block in samples.chunks(480) {
        chunked.push(format, block).unwrap();
    }
    let left = whole.measurement().unwrap().unwrap();
    let right = chunked.measurement().unwrap().unwrap();
    assert!((left.integrated_lufs - right.integrated_lufs).abs() < 0.0001);
    assert_eq!(left.true_peak, right.true_peak);
}

#[test]
fn mixed_rate_programme_combines_gated_energy_instead_of_track_lufs() {
    let loud_format = PcmFormat::new(48_000, 1).unwrap();
    let quiet_format = PcmFormat::new(44_100, 1).unwrap();
    let loud = sine(48_000, 0.5);
    let quiet = sine(44_100, 0.05);
    let mut first = LoudnessProgramme::new();
    first.push(loud_format, &loud).unwrap();
    let first_lufs = first.measurement().unwrap().unwrap().integrated_lufs;
    let mut second = LoudnessProgramme::new();
    second.push(quiet_format, &quiet).unwrap();
    let second_lufs = second.measurement().unwrap().unwrap().integrated_lufs;
    let mut programme = LoudnessProgramme::new();
    programme.push(loud_format, &loud).unwrap();
    programme.push(quiet_format, &quiet).unwrap();
    let actual = programme.measurement().unwrap().unwrap().integrated_lufs;
    assert!((actual - (first_lufs + second_lufs) / 2.0).abs() > 2.0);
    assert!(actual <= first_lufs + 0.2);
}

#[test]
fn invalid_pcm_is_refused_without_changing_the_measurement() {
    let format = PcmFormat::new(48_000, 1).unwrap();
    let mut meter = LoudnessProgramme::new();
    let samples = sine(48_000, 0.2);
    meter.push(format, &samples).unwrap();
    let previous = meter.measurement().unwrap();
    assert!(matches!(
        meter.push(format, &[f32::NAN]),
        Err(LoudnessError::InvalidBuffer)
    ));
    assert!(matches!(
        meter.push(PcmFormat::new(48_000, 3).unwrap(), &[0.0; 3]),
        Err(LoudnessError::UnsupportedLayout)
    ));
    assert_eq!(meter.measurement().unwrap(), previous);
}

#[test]
fn high_rate_programme_retains_lufs_without_claiming_true_peak() {
    for rate in [192_000, 384_000] {
        let mut meter = LoudnessProgramme::new();
        meter
            .push(PcmFormat::new(rate, 1).unwrap(), &sine(rate, 0.2))
            .unwrap();
        let measured = meter.measurement().unwrap().unwrap();
        assert!(measured.integrated_lufs.is_finite());
        assert_eq!(measured.true_peak, None);
    }
}

#[test]
fn programme_with_high_rate_section_does_not_claim_true_peak() {
    let mut meter = LoudnessProgramme::new();
    meter
        .push(PcmFormat::new(48_000, 1).unwrap(), &sine(48_000, 0.2))
        .unwrap();
    assert!(meter.measurement().unwrap().unwrap().true_peak.is_some());
    meter
        .push(PcmFormat::new(192_000, 1).unwrap(), &sine(192_000, 0.2))
        .unwrap();
    let measured = meter.measurement().unwrap().unwrap();
    assert!(measured.integrated_lufs.is_finite());
    assert_eq!(measured.true_peak, None);
}

#[test]
fn format_changes_release_completed_pcm_windows_without_changing_gating() {
    let mut programme = LoudnessProgramme::new();
    let mut reference = Vec::new();
    // Returning to a previous format must still discard unfinished gates at
    // each intervening boundary, including a section shorter than 400 ms.
    for (rate, channels, milliseconds, amplitude) in [
        (48_000, 1, 1_139, 0.2),
        (44_100, 2, 137, 0.5),
        (48_000, 1, 919, 0.02),
        (96_000, 2, 433, 0.1),
        (44_100, 1, 1_701, 0.3),
    ] {
        let format = PcmFormat::new(rate, channels).unwrap();
        let frames = rate as usize * milliseconds / 1000;
        let mono = sine(rate, amplitude);
        let samples = mono
            .iter()
            .cycle()
            .take(frames)
            .flat_map(|&sample| std::iter::repeat_n(sample, usize::from(channels)))
            .collect::<Vec<_>>();
        let mut section =
            EbuR128::new(u32::from(channels), rate, Mode::I | Mode::TRUE_PEAK).unwrap();
        let mut tail = PeakTail::new(format);
        for block in samples.chunks(usize::from(channels) * 733) {
            section.add_frames_f32(block).unwrap();
            tail.push(block);
            programme.push(format, block).unwrap();
        }
        reference.push((section, tail));
    }
    let expected_lufs =
        EbuR128::loudness_global_multiple(reference.iter().map(|(meter, _)| meter)).unwrap() as f32;
    let expected_peak = reference
        .iter()
        .map(|(meter, tail)| tail.peak(meter).unwrap().unwrap())
        .fold(0.0_f32, f32::max);
    let actual = programme.measurement().unwrap().unwrap();
    assert_eq!(actual.integrated_lufs, expected_lufs);
    assert_eq!(actual.true_peak, Some(expected_peak));
    assert_eq!(programme.measurement().unwrap(), Some(actual));
    assert_eq!(
        programme.sections.len(),
        1,
        "completed formats must release their full PCM windows"
    );
}

#[test]
fn high_rate_section_keeps_true_peak_unknown_after_returning_to_low_rate() {
    let mut programme = LoudnessProgramme::new();
    for rate in [48_000, 192_000, 44_100] {
        programme
            .push(PcmFormat::new(rate, 1).unwrap(), &sine(rate, 0.2))
            .unwrap();
    }
    assert_eq!(programme.measurement().unwrap().unwrap().true_peak, None);
}
