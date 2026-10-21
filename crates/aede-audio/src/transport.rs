//! Bounded transport transitions keep device output open and discarded PCM unacknowledged.
use crate::ring::Consumer;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
#[derive(Default)]
pub struct Reset {
    pub requested: AtomicU64,
    pub applied: AtomicU64,
    pub consumed: AtomicU64,
}
pub struct Transport {
    pub reset: Arc<Reset>,
    length: u32,
    remaining: u32,
    rising: u32,
    last: [f32; 2],
    quiet: bool,
    played: bool,
    reserve: usize,
    buffering: bool,
    rebuffering: bool,
}
impl Transport {
    pub fn new(rate: u32, reset: Arc<Reset>) -> Self {
        Self {
            reset,
            length: (rate / 200).max(1),
            remaining: 0,
            rising: 0,
            last: [0.; 2],
            quiet: false,
            played: false,
            reserve: (rate / 10).max(1) as usize,
            buffering: true,
            rebuffering: false,
        }
    }
    /// Keep 100 ms of scheduling reserve in the existing half-second ring.
    /// A completed short source drains immediately; held PCM is never acknowledged.
    pub fn ready(&mut self, available: usize, finished: bool) -> bool {
        if self.buffering && available < self.reserve && !finished {
            return false;
        }
        self.buffering = false;
        self.rebuffering = false;
        true
    }
    pub fn rebuffering(&self) -> bool {
        self.rebuffering
    }
    pub fn starved(&mut self) {
        self.buffering = true;
        self.rebuffering = true;
    }
    pub fn sample(&mut self, sample: Option<[f32; 2]>, paused: bool) -> [f32; 2] {
        if let Some(sample) = sample {
            if self.quiet && self.played {
                self.rising = self.length;
            }
            self.quiet = false;
            self.played = true;
            let gain = if self.rising > 0 {
                self.rising -= 1;
                1. - self.rising as f32 / self.length as f32
            } else {
                1.
            };
            self.last = sample.map(|value| value * gain);
            return self.last;
        }
        if !self.quiet {
            self.remaining = if paused { self.length } else { 0 };
            self.quiet = true;
        }
        if self.remaining > 0 {
            self.remaining -= 1;
            self.last
                .map(|value| value * self.remaining as f32 / self.length as f32)
        } else {
            [0.; 2]
        }
    }
    pub fn flush(&mut self, ring: &mut Consumer<[f32; 2]>, consumed: &AtomicU64) -> bool {
        let requested = self.reset.requested.load(Ordering::Acquire);
        if requested == self.reset.applied.load(Ordering::Relaxed)
            || !self.quiet
            || self.remaining > 0
        {
            return false;
        }
        while ring.pop().is_ok() {}
        self.reset
            .consumed
            .store(consumed.swap(0, Ordering::Relaxed), Ordering::Relaxed);
        self.last = [0.; 2];
        self.buffering = true;
        self.rebuffering = false;
        self.reset.applied.store(requested, Ordering::Release);
        true
    }
}
