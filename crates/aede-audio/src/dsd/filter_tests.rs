use super::*;

// Evaluate the transfer function, independently of bit packing and streaming.
// Bounds are our engineering contract, not Sony/Philips certification limits.
fn response_db(taps: &[f64], frequency: f64, rate: f64) -> f64 {
    let (mut real, mut imaginary) = (0.0, 0.0);
    for (i, tap) in taps.iter().enumerate() {
        let phase = 2.0 * PI * frequency * i as f64 / rate;
        real += tap * phase.cos();
        imaginary += tap * phase.sin();
    }
    20.0 * real.hypot(imaginary).max(1e-16).log10()
}

#[test]
fn measurement_filter_keeps_analysis_band_and_rejects_decimation_images() {
    let taps = coefficients(TAPS, 0.045);
    for rate in [2_822_400.0, 5_644_800.0] {
        for frequency in (0..=75_000).step_by(50) {
            let db = response_db(&taps, f64::from(frequency), rate);
            assert!(db.abs() < 0.01, "{rate} Hz, {frequency} Hz: {db} dB");
        }
        let nyquist = rate / 16.0;
        for i in 0..=4096 {
            let frequency = nyquist + (rate / 2.0 - nyquist) * f64::from(i) / 4096.0;
            let db = response_db(&taps, frequency, rate);
            assert!(db < -75.0, "{rate} Hz, {frequency} Hz: {db} dB");
        }
    }
}

#[test]
fn listening_filter_preserves_audio_band_and_rejects_ultrasound() {
    for rate in [352_800, 705_600] {
        let filter = ListeningFilter::new(rate, 1);
        for frequency in (0..=20_000).step_by(50) {
            let db = response_db(&filter.taps, f64::from(frequency), f64::from(rate));
            assert!(db.abs() < 0.01, "{rate} Hz, {frequency} Hz: {db} dB");
        }
        for i in 0..=4096 {
            let frequency = 44_100.0 + (f64::from(rate) / 2.0 - 44_100.0) * f64::from(i) / 4096.0;
            let db = response_db(&filter.taps, frequency, f64::from(rate));
            assert!(db < -90.0, "{rate} Hz, {frequency} Hz: {db} dB");
        }
    }
}
