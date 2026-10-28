//! Integer-only callback renderer. Pause/mute use framed balanced DSD silence.
use super::{
    Carrier, Encoder, Format, Payload,
    state::{Fault, Signals},
};
use crate::queue::PcmConsumer;
use std::sync::{Arc, atomic::Ordering};

pub(super) struct Renderer {
    input: PcmConsumer<Payload>,
    encoder: Encoder,
    signals: Arc<Signals>,
    channels: usize,
    preamble: usize,
}
impl Renderer {
    pub fn new(
        input: PcmConsumer<Payload>,
        format: Format,
        carrier: Carrier,
        signals: Arc<Signals>,
    ) -> Self {
        Self {
            input,
            encoder: Encoder::new(format, carrier),
            signals,
            channels: format.channels(),
            preamble: 32,
        }
    }
    pub fn frame_bytes(&self) -> usize {
        self.encoder.frame_bytes()
    }
    #[cfg(any(target_os = "macos", test))]
    pub fn render(&mut self, output: &mut [u8]) {
        if !output.len().is_multiple_of(self.frame_bytes()) {
            output.fill(0);
            self.signals.fail(Fault::Buffer);
            return;
        }
        let (active, paused, muted) = self.settings();
        let frame_bytes = self.frame_bytes();
        let mut delivered = self.signals.consumed.load(Ordering::Acquire);
        for frame in output.chunks_exact_mut(frame_bytes) {
            let before = self.input.consumed_frames();
            let payload = self.next_frame(active, paused);
            if self.signals.cancel.load(Ordering::Acquire)
                || self.signals.fault.load(Ordering::Acquire) != 0
            {
                frame.fill(0);
                continue;
            }
            let result = if muted {
                self.encoder.write_silence(frame)
            } else {
                self.encoder.write_frame(&payload[..self.channels], frame)
            };
            if result.is_err() {
                frame.fill(0);
                self.signals.fail(Fault::Buffer);
            } else {
                // Queue removal precedes the fault check. Only a successfully
                // encoded frame belongs to software output progress.
                delivered += self.input.consumed_frames() - before;
            }
        }
        self.publish(delivered);
    }

    /// Retain original words and source prefixes until a push device accepts
    /// them. Mute and marker phase belong to each actual write, not staging.
    #[cfg(any(target_os = "windows", target_os = "linux", test))]
    pub fn stage(
        &mut self,
        frames: &mut [[Payload; 2]],
        prefixes: &mut [u64],
    ) -> Result<(), String> {
        if self.channels != 2 || prefixes.len() != frames.len() + 1 {
            self.signals.fail(Fault::Buffer);
            return Err("DoP staging needs stereo frames and their source prefixes".into());
        }
        prefixes[0] = self.input.consumed_frames();
        let (active, paused, _) = self.settings();
        for (index, frame) in frames.iter_mut().enumerate() {
            *frame = self.next_frame(active, paused);
            prefixes[index + 1] = self.input.consumed_frames();
        }
        self.signals.current()
    }
    pub fn publish(&self, frames: u64) {
        self.signals.consumed.store(frames, Ordering::Release);
        if frames == self.signals.expected {
            self.signals.source_finished.store(true, Ordering::Release);
        }
    }
    fn settings(&self) -> (bool, bool, bool) {
        (
            self.signals.admitted.load(Ordering::Acquire)
                && self.signals.ready.load(Ordering::Acquire),
            self.signals.paused.load(Ordering::Acquire),
            self.signals.muted.load(Ordering::Acquire),
        )
    }
    fn next_frame(&mut self, active: bool, paused: bool) -> [Payload; 2] {
        let mut payload = [Payload(0x6969); 2];
        if self.signals.cancel.load(Ordering::Acquire)
            || self.signals.fault.load(Ordering::Acquire) != 0
        {
            return payload;
        }
        if self.preamble > 0 {
            self.preamble -= 1;
        } else if active && !paused && self.input.consumed_frames() != self.signals.expected {
            match self.input.try_read_frame(&mut payload[..self.channels]) {
                Ok(true) => {}
                Ok(false) => self.signals.fail(Fault::Underrun),
                Err(_) => self.signals.fail(Fault::Source),
            }
        }
        payload
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
