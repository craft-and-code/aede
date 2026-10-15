use super::*;
#[test]
fn overlapping_audio_and_adjacent_fragments_are_counted_once() {
    let values = union(vec![
        Interval {
            start_ms: 5000,
            end_ms: 10000,
        },
        Interval {
            start_ms: 0,
            end_ms: 8000,
        },
        Interval {
            start_ms: 10000,
            end_ms: 12000,
        },
    ]);
    assert_eq!(
        values,
        vec![Interval {
            start_ms: 0,
            end_ms: 12000
        }]
    );
    assert_eq!(duration(&values, 2000, 9000), 7000);
    assert_eq!(duration(&values, 15000, 20000), 0);
}
