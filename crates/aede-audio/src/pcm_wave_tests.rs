use super::*;
use std::io::Cursor;
fn header(rate: u32, channels: u16) -> Vec<u8> {
    let mut bytes = b"RIFF\xff\xff\xff\xffWAVEfmt ".to_vec();
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(3u16.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * u32::from(channels) * 4).to_le_bytes());
    bytes.extend((channels * 4).to_le_bytes());
    bytes.extend(32u16.to_le_bytes());
    bytes.extend(b"data\xff\xff\xff\xff");
    bytes
}
#[test]
fn self_describing_pcm_keeps_its_actual_rate_and_channels() {
    for (rate, channels) in [(44100, 1), (96000, 2), (192000, 6)] {
        let mut bytes = header(rate, channels);
        bytes.extend(0.25f32.to_le_bytes());
        let mut cursor = Cursor::new(bytes);
        assert_eq!(read_format(&mut cursor).unwrap(), (rate, channels));
        assert_eq!(read::<4>(&mut cursor).unwrap(), 0.25f32.to_le_bytes());
    }
}
#[test]
fn malformed_truncated_or_over_budget_headers_are_errors() {
    let good = header(48000, 2);
    for length in 0..good.len() {
        assert!(read_format(&mut Cursor::new(&good[..length])).is_err());
    }
    for (at, value) in [(20, 1u8), (22, 0), (32, 1), (34, 16)] {
        let mut bytes = good.clone();
        bytes[at] = value;
        assert!(read_format(&mut Cursor::new(bytes)).is_err());
    }
    let mut oversized = b"RIFF\xff\xff\xff\xffWAVEJUNK".to_vec();
    oversized.extend(u32::MAX.to_le_bytes());
    assert!(read_format(&mut Cursor::new(oversized)).is_err());
}
