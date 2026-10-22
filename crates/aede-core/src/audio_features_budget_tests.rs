use super::*;
use std::cell::Cell;
use std::rc::Rc;

struct TimedSource {
    clock: Rc<Cell<Duration>>,
    reads: usize,
    pcm_time: Duration,
    eof_time: Duration,
    samples: Vec<f32>,
}
impl Source for TimedSource {
    fn read_samples(&mut self) -> Result<Option<&[f32]>, String> {
        self.reads += 1;
        let elapsed = if self.reads == 1 {
            self.pcm_time
        } else {
            self.eof_time
        };
        self.clock.set(self.clock.get() + elapsed);
        Ok((self.reads == 1).then_some(self.samples.as_slice()))
    }
}
fn measure(pcm: Duration, eof: Duration, waiting: Duration) -> Result<Features, String> {
    let clock = Rc::new(Cell::new(Duration::ZERO));
    let mut source = TimedSource {
        clock: clock.clone(),
        reads: 0,
        pcm_time: pcm,
        eof_time: eof,
        samples: vec![0.; 4096],
    };
    extract_source(
        &mut source,
        1,
        || {
            clock.set(clock.get() + waiting);
            Ok(())
        },
        || clock.get(),
    )
}

#[test]
fn eof_read_cannot_publish_features_after_exceeding_the_active_budget() {
    let result = measure(Duration::ZERO, Duration::from_secs(181), Duration::ZERO);
    assert_eq!(
        result.unwrap_err(),
        "acoustic active computation budget exceeded"
    );
}

#[test]
fn eof_read_counts_toward_the_total_active_budget() {
    let result = measure(
        Duration::from_secs(90),
        Duration::from_secs(91),
        Duration::ZERO,
    );
    assert_eq!(
        result.unwrap_err(),
        "acoustic active computation budget exceeded"
    );
}

#[test]
fn eof_at_the_budget_boundary_and_long_checkpoint_waits_remain_valid() {
    let features = measure(
        Duration::from_secs(90),
        Duration::from_secs(90),
        Duration::from_secs(1000),
    )
    .unwrap();
    assert!(features.valid());
    assert_eq!(features.frames, 4096);
}

#[test]
fn pcm_read_still_refuses_an_exceeded_active_budget() {
    let result = measure(Duration::from_secs(181), Duration::ZERO, Duration::ZERO);
    assert_eq!(
        result.unwrap_err(),
        "acoustic active computation budget exceeded"
    );
}
