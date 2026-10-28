//! Read/pack off the callback, with bounded backpressure and no PCM processing.
use super::{
    Payload, Reader,
    state::{Fault, Signals},
};
use crate::queue::PcmProducer;
use std::{
    io,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

pub(super) fn run(mut reader: Reader, mut output: PcmProducer<Payload>, signals: Arc<Signals>) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        produce(&mut reader, &mut output, &signals)
    }));
    match result {
        Ok(Err(error)) if !signals.cancel.load(Ordering::Acquire) => {
            signals.fail_message(Fault::Source, error)
        }
        Err(_) => signals.fail_message(Fault::Worker, "DoP source worker panicked".into()),
        _ => {}
    }
    output.close_input();
    signals.decoder_done.store(true, Ordering::Release);
}

fn produce(
    reader: &mut Reader,
    output: &mut PcmProducer<Payload>,
    signals: &Signals,
) -> Result<(), String> {
    let channels = reader.format().channels();
    let mut frames = 0u64;
    loop {
        signals.current()?;
        let Some(block) = reader
            .next_chunk(|| signals.current().map_err(crate::dsd::DsdError))
            .map_err(|e| e.to_string())?
        else {
            break;
        };
        let payload = stereo(block.payload, channels);
        frames = frames
            .checked_add((payload.len() / 2) as u64)
            .filter(|&n| n <= signals.expected)
            .ok_or("DoP source exceeded its declared frame count")?;
        let mut at = 0;
        while at < payload.len() {
            signals.current()?;
            match output.write_samples(&payload[at..]) {
                Ok(count) if count > 0 => {
                    at += count;
                    publish_ready(output, signals);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(error) => return Err(error.to_string()),
                Ok(_) => return Err("DoP queue accepted no complete source frame".into()),
            }
        }
    }
    if frames != signals.expected {
        return Err("DoP source ended before its declared length".into());
    }
    signals.ready.store(true, Ordering::Release);
    Ok(())
}

fn publish_ready(output: &PcmProducer<Payload>, signals: &Signals) {
    // The output remains on framed silence during startup. Admit programme
    // only after the bounded source queue is full; a first read burst can be
    // smaller than one hardware buffer. Short files are admitted at valid EOF.
    let queue = output.snapshot();
    if queue.queued_frames == queue.capacity_frames {
        signals.ready.store(true, Ordering::Release);
    }
}

fn stereo(payload: Vec<Payload>, channels: usize) -> Vec<Payload> {
    if channels == 2 {
        payload
    } else {
        // A physical stereo DAC receives two identical copies of mono DSD.
        // Source facts remain mono; no arithmetic, interpolation or mix occurs.
        payload.into_iter().flat_map(|word| [word, word]).collect()
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
