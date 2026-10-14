use super::*;
fn measure(hz: f32, gain: f32) -> Features {
    let mut extract = Extractor::new(1).unwrap();
    let samples = (0..RATE as usize)
        .map(|n| gain * (std::f32::consts::TAU * hz * n as f32 / RATE as f32).sin())
        .collect::<Vec<_>>();
    extract.push(&samples, || Ok(())).unwrap();
    extract.finish().unwrap()
}
#[test]
fn personal_exemplars_select_similar_timbre_without_using_mastering_gain() {
    let a = measure(440., 0.4);
    let b = measure(450., 0.4);
    let near = measure(445., 0.04);
    let far = measure(8000., 0.4);
    let model = AcousticModel::new(&[&a, &b]).unwrap();
    assert!(model.distance(&near).unwrap() < model.radius());
    assert!(model.distance(&far).unwrap() > model.radius());
    assert!(AcousticModel::new(&[&a]).is_none());
}
#[test]
fn unavailable_or_silent_features_cannot_be_training_examples() {
    let a = measure(440., 0.4);
    let mut invalid = a.clone();
    invalid.profile = 999;
    assert!(AcousticModel::new(&[&a, &invalid]).is_none());
    let silent = measure(440., 0.);
    assert!(AcousticModel::new(&[&a, &silent]).is_none());
}
