//! Explicit fixture qualification tool; only its reference PCM is loaded whole.
use aede_audio::native::{ApePcm16Decoder, Decoder};
use std::{
    fs::{self, File},
    path::Path,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: verify_legacy_ape INPUT.ape REFERENCE.s16le".into());
    }
    let path = Path::new(&args[1]);
    let expected = fs::read(&args[2])?;
    if expected.is_empty() || !expected.len().is_multiple_of(4) {
        return Err("expected nonempty stereo PCM16 reference".into());
    }
    let mut d = Decoder::open(path)?.ok_or("not recognized")?;
    let info = d.info();
    let mut at = 0;
    while let Some(pcm) = d.next_chunk()? {
        for sample in pcm {
            let v = i16::from_le_bytes(
                expected
                    .get(at..at + 2)
                    .ok_or("decoded beyond reference")?
                    .try_into()?,
            ) as f32
                / 32768.0;
            if sample != v {
                return Err(format!("PCM mismatch at {at}").into());
            }
            at += 2;
        }
    }
    if at != expected.len() {
        return Err("wrong decoded length".into());
    }
    let mut d = ApePcm16Decoder::open(File::open(path)?)?;
    let total = expected.len() as u64 / 4;
    for start in [
        0,
        1,
        9215,
        9216,
        73727,
        73728,
        294911,
        294912,
        total - 1,
        total,
    ]
    .into_iter()
    .filter(|s| *s <= total)
    {
        d.seek(start)?;
        let mut at = start as usize * 4;
        while let Some(pcm) = d.next_chunk()? {
            if pcm
                != expected
                    .get(at..at + pcm.len())
                    .ok_or("seek beyond reference")?
            {
                return Err(format!("seek mismatch {start} at {at}").into());
            }
            at += pcm.len();
        }
        if at != expected.len() {
            return Err("seek length".into());
        }
    }
    println!("OK {} {:?} every sample and seek", path.display(), info);
    Ok(())
}
