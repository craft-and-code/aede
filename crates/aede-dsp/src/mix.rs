//! Convex two-voice mixing and bounded overlap; queue ownership stays with the caller.
pub fn blend(a: [f32; 2], b: [f32; 2], frame: u64, total: u64) -> [f32; 2] {
    let gain = if total <= 1 {
        1.
    } else {
        (frame.min(total - 1) as f64 / (total - 1) as f64) as f32
    };
    std::array::from_fn(|channel| a[channel] * (1. - gain) + b[channel] * gain)
}
pub fn overlap(
    ms: u64,
    rate: u32,
    outgoing: Option<u64>,
    incoming: Option<u64>,
    position: u64,
) -> u64 {
    let (Some(outgoing), Some(incoming)) = (outgoing, incoming) else {
        return 0;
    };
    ms.min(outgoing / 2)
        .min(incoming / 2)
        .min(outgoing.saturating_sub(position))
        .saturating_mul(u64::from(rate))
        / 1000
}
