use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

struct TemporaryWav(PathBuf);

impl TemporaryWav {
    fn new(bytes: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aede_integer_wav_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(bytes).unwrap();
        Self(path)
    }

    fn open(&self) -> File {
        File::open(&self.0).unwrap()
    }
}

impl Drop for TemporaryWav {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

fn format(bits: u16, channels: u16) -> Vec<u8> {
    let mut bytes = Vec::new();
    let block_align = channels * (bits / 8);
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&96_000_u32.to_le_bytes());
    bytes.extend_from_slice(&(96_000 * u32::from(block_align)).to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes
}

fn extensible_format(bits: u16, channels: u16, mask: u32) -> Vec<u8> {
    let mut bytes = format(bits, channels);
    bytes[..2].copy_from_slice(&0xfffe_u16.to_le_bytes());
    bytes.extend_from_slice(&22_u16.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(&mask.to_le_bytes());
    bytes.extend_from_slice(&PCM_SUBTYPE);
    bytes
}

fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut bytes = kind.to_vec();
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(body);
    if !body.len().is_multiple_of(2) {
        bytes.push(0);
    }
    bytes
}

fn wav(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
    for chunk in chunks {
        bytes.extend_from_slice(chunk);
    }
    let size = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    bytes
}

fn pcm(bits: u16, samples: &[i32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes()[..usize::from(bits / 8)].to_vec())
        .collect()
}

fn with_samples(bits: u16, channels: u16, samples: &[i32]) -> Vec<u8> {
    wav(&[
        chunk(b"fmt ", &format(bits, channels)),
        chunk(b"data", &pcm(bits, samples)),
    ])
}

fn decode_all(stream: &mut IntegerWavStream) -> Vec<i32> {
    let mut samples = Vec::new();
    while let Some(chunk) = stream.next_chunk().unwrap() {
        samples.extend_from_slice(&chunk);
    }
    assert!(stream.next_chunk().unwrap().is_none());
    samples
}

#[test]
fn signed_endpoints_low_bits_and_channel_order_survive_without_scaling() {
    for (bits, values) in [
        (16, vec![-32_768, 32_767, -1, 1, 0, -2]),
        (24, vec![-8_388_608, 8_388_607, -1, 1, 0, -2]),
    ] {
        for channels in [1, 2] {
            let bytes = with_samples(bits, channels, &values);
            let fixture = TemporaryWav::new(&bytes);
            let mut stream = IntegerWavStream::open(fixture.open()).unwrap();
            assert_eq!(stream.sample_rate(), 96_000);
            assert_eq!(stream.channels(), channels);
            assert_eq!(stream.bits_per_sample(), u32::from(bits));
            assert_eq!(stream.channel_layout(), canonical_layout(channels));
            assert_eq!(decode_all(&mut stream), values);
            assert_eq!(std::fs::read(&fixture.0).unwrap(), bytes);
        }
    }
}

#[test]
fn every_signed_sixteen_bit_value_is_preserved_across_progressive_chunks() {
    let reference: Vec<i32> = (i32::from(i16::MIN)..=i32::from(i16::MAX)).collect();
    let fixture = TemporaryWav::new(&with_samples(16, 2, &reference));
    let mut stream = IntegerWavStream::open(fixture.open()).unwrap();
    let mut actual = Vec::new();
    while let Some(chunk) = stream.next_chunk().unwrap() {
        assert!(chunk.len() <= 4096 * 2);
        assert!(chunk.len().is_multiple_of(2));
        actual.extend_from_slice(&chunk);
    }
    assert_eq!(actual, reference);
}

#[test]
fn twenty_four_bit_byte_boundaries_preserve_frames_across_progressive_chunks() {
    let pattern = [
        -8_388_608, 8_388_607, -65_537, 65_536, -257, 256, -256, 255, -1, 1, 0, -2,
    ];
    let reference: Vec<i32> = pattern.into_iter().cycle().take(4096 * 6 + 2).collect();
    let fixture = TemporaryWav::new(&with_samples(24, 2, &reference));
    let mut stream = IntegerWavStream::open(fixture.open()).unwrap();
    let mut actual = Vec::new();
    while let Some(chunk) = stream.next_chunk().unwrap() {
        assert!(chunk.len() <= 4096 * 2);
        assert!(chunk.len().is_multiple_of(2));
        actual.extend_from_slice(&chunk);
    }
    assert_eq!(actual, reference);
}

#[test]
fn checked_in_real_pcm_wav_matches_its_independent_signed_integer_reference() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.wav");
    let bytes = std::fs::read(&path).unwrap();
    // The fixture has one 16-bit mono data chunk after its INFO metadata.
    assert_eq!(&bytes[168..176], b"data\x22\x56\0\0");
    let reference: Vec<i32> = bytes[176..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|sample| i32::from(i16::from_le_bytes([sample[0], sample[1]])))
        .collect();
    let mut stream = IntegerWavStream::open(File::open(&path).unwrap()).unwrap();
    assert_eq!(stream.sample_rate(), 44_100);
    assert_eq!(stream.bits_per_sample(), 16);
    assert_eq!(stream.channels(), 1);
    assert_eq!(reference.len(), 11_025);
    assert_eq!(decode_all(&mut stream), reference);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn extensible_pcm_accepts_only_canonical_mono_and_explicit_left_right_stereo() {
    for bits in [16, 24] {
        for (channels, mask) in [(1, 0), (1, 4), (2, 3)] {
            let reference = [-1, 1, 0, 3];
            let fixture = TemporaryWav::new(&wav(&[
                chunk(b"fmt ", &extensible_format(bits, channels, mask)),
                chunk(b"data", &pcm(bits, &reference)),
            ]));
            let mut stream = IntegerWavStream::open(fixture.open()).unwrap();
            assert_eq!(stream.channel_layout(), canonical_layout(channels));
            assert_eq!(decode_all(&mut stream), reference);
        }
        for (channels, mask) in [(1, 1), (1, 2), (1, 8), (1, 3), (2, 0), (2, 5), (2, 4)] {
            let fixture = TemporaryWav::new(&wav(&[
                chunk(b"fmt ", &extensible_format(bits, channels, mask)),
                chunk(b"data", &pcm(bits, &[0, 0])),
            ]));
            assert!(matches!(
                IntegerWavStream::open(fixture.open()),
                Err(Error::ChannelLayout(_))
            ));
        }
    }
}

#[test]
fn metadata_and_odd_chunk_padding_do_not_change_source_samples() {
    let reference = [-8_388_608, -1, 8_388_607];
    let bytes = wav(&[
        chunk(b"JUNK", b"odd"),
        chunk(b"data", &pcm(24, &reference)),
        chunk(b"LIST", &[0xff; 1025]),
        chunk(b"fmt ", &format(24, 1)),
        chunk(b"JUNK", b"end"),
    ]);
    let fixture = TemporaryWav::new(&bytes);
    let mut stream = IntegerWavStream::open(fixture.open()).unwrap();
    assert_eq!(decode_all(&mut stream), reference);
    assert_eq!(std::fs::read(&fixture.0).unwrap(), bytes);
}

#[test]
fn empty_data_and_zero_extension_pcm_headers_have_stable_eof() {
    let mut header = format(16, 1);
    header.extend_from_slice(&0_u16.to_le_bytes());
    let fixture = TemporaryWav::new(&wav(&[chunk(b"fmt ", &header), chunk(b"data", &[])]));
    let mut stream = IntegerWavStream::open(fixture.open()).unwrap();
    assert!(stream.next_chunk().unwrap().is_none());
    assert!(stream.next_chunk().unwrap().is_none());
}

#[test]
fn unsupported_containers_codecs_depths_and_channel_counts_are_refused() {
    let valid = with_samples(16, 1, &[0]);
    for magic in [b"RF64", b"RIFX", b"OggS"] {
        let mut bytes = valid.clone();
        bytes[..4].copy_from_slice(magic);
        let fixture = TemporaryWav::new(&bytes);
        assert!(IntegerWavStream::open(fixture.open()).is_err());
    }
    for (tag, bits, channels) in [
        (3_u16, 16, 1),
        (2, 16, 1),
        (1, 8, 1),
        (1, 32, 1),
        (1, 16, 6),
    ] {
        let mut header = format(bits, channels);
        header[..2].copy_from_slice(&tag.to_le_bytes());
        let fixture = TemporaryWav::new(&wav(&[chunk(b"fmt ", &header), chunk(b"data", &[0; 12])]));
        assert!(IntegerWavStream::open(fixture.open()).is_err());
    }
}

#[test]
fn invalid_rate_alignment_or_extensible_declarations_are_refused() {
    let base = format(16, 2);
    let mut invalid = Vec::new();
    for (offset, value) in [
        (4, vec![0; 4]),
        (8, 1_u32.to_le_bytes().to_vec()),
        (12, vec![2, 0]),
    ] {
        let mut bytes = base.clone();
        bytes[offset..offset + value.len()].copy_from_slice(&value);
        invalid.push(bytes);
    }
    let mut overflowing_rate = base;
    overflowing_rate[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    invalid.push(overflowing_rate);
    for (offset, value) in [
        (16, 21_u16.to_le_bytes().to_vec()),
        (18, 20_u16.to_le_bytes().to_vec()),
        (24, 3_u32.to_le_bytes().to_vec()),
    ] {
        let mut bytes = extensible_format(24, 2, 3);
        bytes[offset..offset + value.len()].copy_from_slice(&value);
        invalid.push(bytes);
    }
    let mut padded = extensible_format(32, 2, 3);
    padded[18..20].copy_from_slice(&24_u16.to_le_bytes());
    invalid.push(padded);
    let mut misleading_extension = format(16, 1);
    misleading_extension.extend_from_slice(&1_u16.to_le_bytes());
    invalid.push(misleading_extension);
    for header in invalid {
        let fixture = TemporaryWav::new(&wav(&[chunk(b"fmt ", &header), chunk(b"data", &[0; 12])]));
        assert!(IntegerWavStream::open(fixture.open()).is_err());
    }
}

#[test]
fn truncated_riff_chunks_missing_padding_and_incomplete_frames_are_refused() {
    let valid = with_samples(16, 2, &[0, 1]);
    for length in 0..valid.len() {
        let fixture = TemporaryWav::new(&valid[..length]);
        assert!(
            IntegerWavStream::open(fixture.open()).is_err(),
            "length {length}"
        );
    }
    for data in [vec![0], vec![0; 3], vec![0; 5]] {
        let fixture = TemporaryWav::new(&wav(&[
            chunk(b"fmt ", &format(16, 2)),
            chunk(b"data", &data),
        ]));
        assert!(matches!(
            IntegerWavStream::open(fixture.open()),
            Err(Error::IncompleteFrame)
        ));
    }
    let mut no_padding = with_samples(24, 1, &[0]);
    no_padding.pop();
    let riff_size = no_padding.len() as u32 - 8;
    no_padding[4..8].copy_from_slice(&riff_size.to_le_bytes());
    let fixture = TemporaryWav::new(&no_padding);
    assert!(IntegerWavStream::open(fixture.open()).is_err());
    let mut invalid_chunk = valid.clone();
    invalid_chunk[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    let fixture = TemporaryWav::new(&invalid_chunk);
    assert!(IntegerWavStream::open(fixture.open()).is_err());
    let mut suffix = valid;
    suffix.push(0);
    let fixture = TemporaryWav::new(&suffix);
    assert!(IntegerWavStream::open(fixture.open()).is_err());
}

#[test]
fn missing_duplicate_and_truncated_trailing_chunks_are_refused() {
    let header = chunk(b"fmt ", &format(16, 1));
    let audio = chunk(b"data", &[0, 0]);
    for chunks in [
        vec![header.clone()],
        vec![audio.clone()],
        vec![header.clone(), audio.clone(), header.clone()],
        vec![header.clone(), audio.clone(), audio.clone()],
        vec![header.clone(), audio.clone(), chunk(b"LIST", b"wavl")],
        vec![header, audio, b"LIST\0\0".to_vec()],
    ] {
        let fixture = TemporaryWav::new(&wav(&chunks));
        assert!(IntegerWavStream::open(fixture.open()).is_err());
    }
}

#[test]
fn excessive_empty_metadata_chunks_are_refused_without_decoding_audio() {
    let mut chunks = vec![chunk(b"fmt ", &format(16, 1)), chunk(b"data", &[0, 0])];
    chunks.extend((0..MAX_CHUNKS).map(|_| chunk(b"JUNK", &[])));
    let fixture = TemporaryWav::new(&wav(&chunks));
    assert!(IntegerWavStream::open(fixture.open()).is_err());
}

#[test]
fn source_truncation_after_admission_is_a_sticky_terminal_read_failure() {
    let values = vec![1; 4096 * 2 + 2];
    let bytes = with_samples(24, 2, &values);
    let fixture = TemporaryWav::new(&bytes);
    let mut stream = IntegerWavStream::open(fixture.open()).unwrap();
    assert_eq!(stream.next_chunk().unwrap().unwrap(), values[..4096 * 2]);
    let mut writer = OpenOptions::new().write(true).open(&fixture.0).unwrap();
    writer.set_len(44 + 4096 * 2 * 3).unwrap();
    let error = stream.next_chunk().unwrap_err().to_string();
    writer.seek(SeekFrom::Start(0)).unwrap();
    writer.write_all(&bytes).unwrap();
    assert_eq!(stream.next_chunk().unwrap_err().to_string(), error);
}
