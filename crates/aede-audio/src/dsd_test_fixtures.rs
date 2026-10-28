use crate::test_support::Wave;
use std::f64::consts::PI;

// Independently encoded fixtures: first-order sigma-delta, no external codec.
pub(crate) fn signal(rate: u32, frequency: f64, frames: usize) -> Vec<u8> {
    let mut result = vec![0; frames.div_ceil(8)];
    let mut error = 0.0;
    for i in 0..frames {
        let value = 0.35 * (2.0 * PI * frequency * i as f64 / f64::from(rate)).sin();
        let bit = if value + error >= 0.0 { 1.0 } else { -1.0 };
        error += value - bit;
        if bit > 0.0 {
            result[i / 8] |= 1 << (i % 8);
        }
    }
    result
}
pub(crate) fn dsf(data: &[Vec<u8>], rate: u32, samples: u64, lsb: bool) -> Vec<u8> {
    let payload = samples.div_ceil(8).div_ceil(4096) * 4096 * data.len() as u64;
    let mut bytes = b"DSD ".to_vec();
    bytes.extend(28u64.to_le_bytes());
    bytes.extend((92 + payload).to_le_bytes());
    bytes.extend(0u64.to_le_bytes());
    bytes.extend(b"fmt ");
    bytes.extend(52u64.to_le_bytes());
    for n in [
        1,
        0,
        data.len() as u32,
        data.len() as u32,
        rate,
        if lsb { 1 } else { 8 },
    ] {
        bytes.extend(n.to_le_bytes());
    }
    bytes.extend(samples.to_le_bytes());
    bytes.extend(4096u32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((payload + 12).to_le_bytes());
    for start in (0..samples.div_ceil(8) as usize).step_by(4096) {
        for channel in data {
            for i in start..start + 4096 {
                let b = channel.get(i).copied().unwrap_or(0);
                bytes.push(if lsb { b } else { b.reverse_bits() });
            }
        }
    }
    bytes
}
pub(crate) fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut b = id.to_vec();
    b.extend((data.len() as u64).to_be_bytes());
    b.extend(data);
    if !data.len().is_multiple_of(2) {
        b.push(0);
    }
    b
}
pub(crate) fn dff(data: &[Vec<u8>], rate: u32) -> Vec<u8> {
    let mut properties = b"SND ".to_vec();
    properties.extend(chunk(b"FS  ", &rate.to_be_bytes()));
    let mut channels = (data.len() as u16).to_be_bytes().to_vec();
    if data.len() == 1 {
        channels.extend(b"C   ");
    } else {
        channels.extend(b"SLFTSRGT");
    }
    properties.extend(chunk(b"CHNL", &channels));
    properties.extend(chunk(b"CMPR", b"DSD \0"));
    let mut form = b"DSD ".to_vec();
    form.extend(chunk(b"FVER", &[1, 5, 0, 0]));
    form.extend(chunk(b"PROP", &properties));
    let mut audio = Vec::new();
    for i in 0..data[0].len() {
        for channel in data {
            audio.push(channel[i].reverse_bits());
        }
    }
    form.extend(chunk(b"DSD ", &audio));
    let mut bytes = b"FRM8".to_vec();
    bytes.extend((form.len() as u64).to_be_bytes());
    bytes.extend(form);
    bytes
}
pub(crate) fn source(bytes: &[u8]) -> Wave {
    let file = Wave::new(&[0.0], 1);
    std::fs::write(&file.0, bytes).unwrap();
    file
}
