//! Original vectors exercising public reference-library format contracts.
use super::tests::{Scratch, fixture};
use super::*;

#[test]
fn wavpack_float_exponents_are_adapted_to_normalized_pcm() {
    let path = fixture("float32-norm142.wv");
    let before = fs::read(&path).unwrap();
    let mut decoder = Decoder::open(&path).unwrap().unwrap();
    assert_eq!(decoder.info().frames, Some(4));
    assert_eq!(decoder.info().bits, None);
    assert_eq!(
        decoder.next_chunk().unwrap().unwrap(),
        [0.5, -0.5, 0.25, -0.25]
    );
    assert!(decoder.next_chunk().unwrap().is_none());
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn speex_explicit_final_terminator_keeps_all_complete_coded_frames() {
    let path = fixture("speex-final-partial.spx");
    let before = fs::read(&path).unwrap();
    let mut decoder = Decoder::open(&path).unwrap().unwrap();
    assert_eq!(decoder.info().sample_rate, 8000);
    assert_eq!(decoder.info().channels, 1);
    assert_eq!(decoder.next_chunk().unwrap().unwrap().len(), 320);
    let final_pcm = decoder.next_chunk().unwrap().unwrap();
    assert_eq!(final_pcm.len(), 160);
    assert!(final_pcm.iter().all(|sample| sample.is_finite()));
    assert!(final_pcm.iter().any(|sample| sample.abs() > 0.01));
    assert!(decoder.next_chunk().unwrap().is_none());
    assert_eq!(fs::read(path).unwrap(), before);
}

// These generated pages each contain one complete packet and one lacing value.
// Recompute the transport CRC so negative cases exercise codec admission.
fn final_speex_page(bytes: &[u8]) -> usize {
    let mut at = 0;
    loop {
        assert_eq!(&bytes[at..at + 4], b"OggS");
        assert_eq!(bytes[at + 26], 1);
        let end = at + 28 + usize::from(bytes[at + 27]);
        if end == bytes.len() {
            return at;
        }
        at = end;
    }
}

fn repair_ogg_crc(page: &mut [u8]) {
    page[22..26].fill(0);
    let mut crc = 0u32;
    for &byte in page.iter() {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = (crc << 1) ^ (0x04c11db7 & 0u32.wrapping_sub(crc >> 31));
        }
    }
    page[22..26].copy_from_slice(&crc.to_le_bytes());
}

#[test]
fn speex_short_packets_require_eos_and_an_explicit_nonempty_terminator() {
    let dir = Scratch::new("speex-final-admission");
    for variant in [
        "nonterminal",
        "missing-marker",
        "underflow",
        "empty",
        "granule-after-coded-audio",
    ] {
        let mut bytes = fs::read(fixture("speex-final-partial.spx")).unwrap();
        let at = final_speex_page(&bytes);
        let payload = at + 28;
        match variant {
            "nonterminal" => bytes[at + 5] &= !4,
            // One complete 300-bit speech frame leaves only four bits here.
            // libspeex also returns -1 for this truncated, markerless packet.
            "missing-marker" => {
                bytes.pop();
                bytes[at + 27] -= 1;
            }
            "underflow" => {
                bytes.truncate(payload + 5);
                bytes[at + 27] = 5;
            }
            "empty" => {
                bytes.truncate(payload + 1);
                bytes[payload] = 0x7f; // Five-bit terminator 0b01111, then padding.
                bytes[at + 27] = 1;
            }
            "granule-after-coded-audio" => {
                // Three complete speech frames contain 480 coded frames;
                // the nominal four-slot capacity must not admit frame 481.
                bytes[at + 6..at + 14].copy_from_slice(&481u64.to_le_bytes());
            }
            _ => unreachable!(),
        }
        repair_ogg_crc(&mut bytes[at..]);
        let path = dir.write(&format!("{variant}.spx"), &bytes);
        let mut decoder = Decoder::open(&path).unwrap().unwrap();
        assert_eq!(decoder.next_chunk().unwrap().unwrap().len(), 320);
        assert!(decoder.next_chunk().is_err(), "{variant}");
        assert!(decoder.next_chunk().is_err(), "{variant}: reader retired");
    }
}
