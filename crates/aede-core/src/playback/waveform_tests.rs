use super::*;
#[test]
fn bounded_envelope_retains_impulses_and_silent_regions_with_exact_clock() {
    let mut envelope = Envelope::new();
    for i in 0..1_000_001 {
        envelope.push(if i == 400_000 { 0.75 } else { 0. });
    }
    let result = envelope.finish(48000);
    assert!(result.peaks.len() <= 2048);
    assert_eq!(result.duration_ms, 20833);
    assert_eq!(result.peaks.iter().filter(|peak| **peak > 0.).count(), 1);
    assert_eq!(
        result.peaks[(400_000 / result.bucket_frames) as usize],
        0.75
    );
    assert_eq!(result.peaks.last(), Some(&0.));
}
