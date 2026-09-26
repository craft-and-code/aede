//! One-file terminal playback through the decoder, DSP and local audio output.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use aede_core::playback::decoder::FileDecoder;
use aede_dsp::{Dsp, PcmFormat};

use super::Res;
use crate::args::Args;

const BUFFER_FRAMES: usize = 4096;

pub fn play(args: &Args) -> Res {
    if args.has("data") {
        return Err("aede play reads a file directly; --data applies to catalog commands".into());
    }
    let [raw] = args.positionals.as_slice() else {
        return Err("give exactly one local audio file: aede play <audio-file>".into());
    };
    let path = Path::new(raw);
    let mut decoder = FileDecoder::open(path)?;
    let format = PcmFormat::new(decoder.sample_rate(), decoder.channels())?;
    let mut dsp = Dsp::new(format);

    let mut player = Command::new("ffplay")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nodisp",
            "-autoexit",
            "-nostats",
        ])
        .args([
            "-f",
            "f32le",
            "-sample_rate",
            &format.sample_rate().to_string(),
        ])
        .args([
            "-ch_layout",
            &format!("{}c", format.channels()),
            "-i",
            "pipe:0",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|error| {
            format!("cannot start ffplay: {error}; install ffplay for local audio output")
        })?;
    let Some(mut input) = player.stdin.take() else {
        let _ = player.kill();
        let _ = player.wait();
        return Err("ffplay did not open its PCM input".into());
    };

    println!("Playing {}", path.display());
    let streamed = stream_pcm(&mut decoder, &mut dsp, &mut input);
    drop(input);
    if let Err(error) = streamed {
        if let Some(status) = player.try_wait()?
            && !status.success()
        {
            return Err(
                format!("ffplay stopped with {status}; check the audio output device").into(),
            );
        }
        let _ = player.kill();
        let _ = player.wait();
        return Err(error);
    }
    let status = player.wait()?;
    if !status.success() {
        return Err(format!("ffplay stopped with {status}; check the audio output device").into());
    }
    Ok(())
}

fn stream_pcm(decoder: &mut FileDecoder, dsp: &mut Dsp, output: &mut impl Write) -> Res {
    let channels = usize::from(decoder.channels());
    let mut samples = vec![0.0; BUFFER_FRAMES * channels];
    let mut bytes = vec![0; samples.len() * 4];
    loop {
        let frames = decoder.read_frames(&mut samples)?;
        if frames == 0 {
            return Ok(());
        }
        let count = frames * channels;
        dsp.process(&mut samples[..count])?;
        for (sample, encoded) in samples[..count]
            .iter()
            .zip(bytes.as_chunks_mut::<4>().0.iter_mut())
        {
            encoded.copy_from_slice(&sample.to_le_bytes());
        }
        output
            .write_all(&bytes[..count * 4])
            .map_err(|error| format!("audio output closed: {error}"))?;
    }
}

#[cfg(test)]
#[path = "play_tests.rs"]
mod tests;
