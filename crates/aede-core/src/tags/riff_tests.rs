use super::*;

fn extensible_format(subtype: u32) -> Vec<u8> {
    let mut format = Vec::new();
    format.extend(0xfffeu16.to_le_bytes());
    format.extend(1u16.to_le_bytes());
    format.extend(44_100u32.to_le_bytes());
    format.extend(176_400u32.to_le_bytes());
    format.extend(4u16.to_le_bytes());
    format.extend(32u16.to_le_bytes());
    format.extend(22u16.to_le_bytes());
    format.extend(32u16.to_le_bytes());
    format.extend(4u32.to_le_bytes());
    format.extend(subtype.to_le_bytes());
    format.extend([0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]);
    format
}

fn read_container(
    bytes: &[u8],
    reader: fn(&mut File, u64) -> Result<RawTags, TagError>,
) -> Result<RawTags, TagError> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "aede_wav_format_{}_{nonce}_{}.wav",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, bytes).unwrap();
    let mut file = File::open(&path).unwrap();
    let result = reader(&mut file, bytes.len() as u64);
    drop(file);
    std::fs::remove_file(path).unwrap();
    result
}

fn read_format(format: &[u8]) -> Result<RawTags, TagError> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((20u32 + format.len() as u32).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend((format.len() as u32).to_le_bytes());
    bytes.extend(format);
    if format.len() % 2 == 1 {
        bytes.push(0);
    }
    bytes.extend(b"data");
    bytes.extend(0u32.to_le_bytes());
    read_container(&bytes, read_wav)
}

#[test]
fn aifc_floating_pcm_is_uncompressed_audio_not_a_lossy_codec() {
    for (bits, codec) in [(32u16, b"fl32"), (64u16, b"fl64")] {
        let mut common = Vec::new();
        common.extend(1u16.to_be_bytes());
        common.extend(1u32.to_be_bytes());
        common.extend(bits.to_be_bytes());
        common.extend([0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]); // 44100 as extended80
        common.extend(codec);
        common.push(0); // empty Pascal compression description
        let mut body = b"AIFCFVER".to_vec();
        body.extend(4u32.to_be_bytes());
        body.extend(0xa280_5140u32.to_be_bytes());
        body.extend(b"COMM");
        body.extend((common.len() as u32).to_be_bytes());
        body.extend(common);
        body.push(0); // even chunk alignment
        body.extend(b"SSND");
        let sound_size = 8 + usize::from(bits) / 8;
        body.extend((sound_size as u32).to_be_bytes());
        body.extend(vec![0; sound_size]); // offset, block size and one zero-valued sample
        let mut bytes = b"FORM".to_vec();
        bytes.extend((body.len() as u32).to_be_bytes());
        bytes.extend(body);
        let tags = read_container(&bytes, read_aiff).unwrap();
        assert_eq!(tags.properties.codec, std::str::from_utf8(codec).unwrap());
        assert_eq!(tags.properties.bit_depth, Some(bits));
        assert_eq!(tags.properties.sample_rate, Some(44_100));
        assert!(
            tags.properties.lossless,
            "AIFC {bits}-bit floating PCM must be preserved"
        );
    }
}

#[test]
fn extensible_wav_distinguishes_integer_float_and_compressed_subtypes() {
    for (subtype, expected, lossless) in [
        (1, "pcm", true),
        (3, "pcm_float", true),
        (6, "wav_0x6", false),
    ] {
        let tags = read_format(&extensible_format(subtype)).unwrap();
        assert_eq!(tags.properties.codec, expected);
        assert_eq!(tags.properties.lossless, lossless);
        assert_eq!(tags.properties.bit_depth, Some(32));
    }
}

#[test]
fn extensible_wav_does_not_guess_from_the_start_of_a_vendor_guid() {
    let mut format = extensible_format(1);
    format[39] ^= 1;
    let tags = read_format(&format).unwrap();
    assert_eq!(tags.properties.codec, "wav_0xfffe");
    assert!(!tags.properties.lossless);
}

#[test]
fn truncated_or_undersized_wav_format_is_refused() {
    let format = extensible_format(3);
    for length in [0, 2, 15, 16, 18, 24, 39] {
        assert!(
            read_format(&format[..length]).is_err(),
            "fmt length {length}"
        );
    }
    let mut format = format;
    format[16..18].copy_from_slice(&20u16.to_le_bytes());
    assert!(read_format(&format).is_err());
}

#[test]
fn wav_info_list() {
    let mut data = Vec::new();
    for (id, value) in [(b"INAM", "So What"), (b"IART", "Miles Davis")] {
        data.extend_from_slice(id);
        data.extend_from_slice(&(value.len() as u32).to_le_bytes());
        data.extend_from_slice(value.as_bytes());
        if value.len() % 2 == 1 {
            data.push(0);
        }
    }
    let mut tags = RawTags::default();
    read_info_list(&data, &mut tags);
    assert_eq!(tags.first("title"), Some("So What"));
    assert_eq!(tags.first("artist"), Some("Miles Davis"));
}

#[test]
fn truncated_info_list_does_not_panic() {
    let mut data = b"INAM".to_vec();
    data.extend_from_slice(&999u32.to_le_bytes()); // dishonest size
    data.extend_from_slice(b"short");
    let mut tags = RawTags::default();
    read_info_list(&data, &mut tags);
    assert!(tags.is_empty());
}
