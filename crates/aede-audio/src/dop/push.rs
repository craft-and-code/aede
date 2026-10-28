//! Bounded integer writes for polled exclusive devices. Unaccepted frames retain
//! their marker phase and source prefixes across partial/nonblocking writes.
use super::{
    Carrier, Encoder, Format, Payload,
    driver::{Driver, ServiceError},
    render::Renderer,
    state::Signals,
};
use crate::queue::PcmConsumer;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

pub(super) trait Sink {
    fn verify(&self) -> Result<(), String>;
    fn space(&self) -> Result<usize, String>;
    fn write(&mut self, bytes: &[u8]) -> Result<usize, String>;
    fn start(&mut self) -> Result<(), String>;
    fn close(&mut self) -> Result<(), String>;
    fn tail(&self) -> Duration;
}

pub(super) struct Push<S: Sink> {
    sink: S,
    renderer: Renderer,
    encoder: Encoder,
    signals: Arc<Signals>,
    bytes: Vec<u8>,
    prefixes: Vec<u64>,
    frames: Vec<[Payload; 2]>,
    frame_bytes: usize,
    capacity: usize,
    count: usize,
    at: usize,
    prefilled: usize,
    started: bool,
    closed: bool,
}
impl<S: Sink> Push<S> {
    fn current(&self) -> Result<(), ServiceError> {
        ServiceError::current(&self.signals)
    }

    pub fn new(
        sink: S,
        format: Format,
        carrier: Carrier,
        input: PcmConsumer<Payload>,
        signals: Arc<Signals>,
        capacity: usize,
    ) -> Result<Self, String> {
        if format.channels() != 2 || !(1..=16384).contains(&capacity) {
            return Err("DoP device buffer exceeds the bounded stereo profile".into());
        }
        let renderer = Renderer::new(input, format, carrier, signals.clone());
        let frame_bytes = renderer.frame_bytes();
        Ok(Self {
            sink,
            renderer,
            encoder: Encoder::new(format, carrier),
            signals,
            bytes: vec![0; capacity * frame_bytes],
            prefixes: vec![0; capacity + 1],
            frames: vec![[Payload(0x6969); 2]; capacity],
            frame_bytes,
            capacity,
            count: 0,
            at: 0,
            prefilled: 0,
            started: false,
            closed: false,
        })
    }
}
impl<S: Sink> Driver for Push<S> {
    fn prepared(&self) -> bool {
        self.started
    }
    fn service(&mut self) -> Result<(), ServiceError> {
        self.current()?;
        if self.closed {
            return Err("DoP native output is closed".into());
        }
        let available = self.sink.space()?;
        self.current()?;
        if available > self.capacity {
            return Err("DoP native free space exceeds its buffer".into());
        }
        if available == 0 {
            return Ok(());
        }
        let paused = self.signals.paused.load(Ordering::Acquire);
        let muted = self.signals.muted.load(Ordering::Acquire);
        if !paused && self.at == self.count {
            self.count = available;
            self.at = 0;
            let stage = self.renderer.stage(
                &mut self.frames[..self.count],
                &mut self.prefixes[..=self.count],
            );
            self.current()?;
            stage?;
        }
        let offered = if paused {
            available
        } else {
            available.min(self.count - self.at)
        };
        let mut candidate = self.encoder.clone();
        for index in 0..offered {
            let bytes = &mut self.bytes[index * self.frame_bytes..(index + 1) * self.frame_bytes];
            if paused || muted {
                candidate.write_silence(bytes)
            } else {
                candidate.write_frame(&self.frames[self.at + index], bytes)
            }
            .map_err(|e| e.to_string())?;
        }
        // Native route queries and staging can outlive cancellation. Pending
        // words still need admission immediately before starting another write.
        self.current()?;
        let accepted = self.sink.write(&self.bytes[..offered * self.frame_bytes])?;
        if accepted > offered {
            return Err("DoP native output accepted more frames than offered".into());
        }
        self.encoder.advance(accepted);
        if !paused {
            self.at += accepted;
            self.renderer.publish(self.prefixes[self.at]);
        }
        if !self.started {
            self.prefilled += accepted;
            if self.prefilled >= self.capacity {
                self.current()?;
                self.sink.start()?;
                self.started = true;
            }
        }
        Ok(())
    }
    fn verify(&self) -> Result<(), String> {
        self.sink.verify()
    }
    fn tail(&self) -> Duration {
        self.sink.tail()
    }
    fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.signals.cancel.store(true, Ordering::Release);
        self.sink.close()
    }
}
impl<S: Sink> Drop for Push<S> {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("DoP native output cleanup failed: {error}");
        }
    }
}

#[cfg(test)]
#[path = "push_tests.rs"]
mod tests;
