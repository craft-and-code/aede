use super::super::super::bits::BitReader;
use super::super::{read_frame, read_stream_info};
use super::PrecisionConfirmation;

fn padded_frame() -> ([u8; 34], Vec<u8>, u32) {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audit-padded24.flac");
    let mut file = std::fs::File::open(&path).expect("real FLAC fixture");
    let (info, audio_start) = read_stream_info(&mut file).expect("fixture STREAMINFO");
    let encoded = std::fs::read(&path).expect("fixture bytes");
    let audio = &encoded[audio_start as usize..];
    let frame = read_frame(&mut BitReader::new(audio), &info).expect("fixture first frame");
    (
        info.raw.expect("complete STREAMINFO"),
        audio[frame.span.start..frame.span.end].to_vec(),
        frame.block_size,
    )
}

#[test]
fn unavailable_confirmation_stays_unavailable_for_the_file() {
    let (info, frame, block_size) = padded_frame();
    assert_eq!(
        PrecisionConfirmation::new(Some(info)).confirm(&frame, block_size),
        Some(8)
    );
    assert_eq!(
        PrecisionConfirmation::new(None).confirm(&frame, block_size),
        None
    );

    let mut confirmation = PrecisionConfirmation::new(Some(info));
    // Keep a valid frame declaration but truncate before its sample data, so
    // the existing codec cannot confirm it. A later valid frame cannot undo
    // that missing evidence for the file.
    assert_eq!(confirmation.confirm(&frame[..4], block_size), None);
    assert_eq!(confirmation.confirm(&frame, block_size), None);
}

#[test]
fn inconsistent_or_unsupported_frames_do_not_confirm_padding() {
    let (info, frame, block_size) = padded_frame();
    let max_block = u32::from(u16::from_be_bytes([info[2], info[3]]));
    for invalid_block in [0, max_block + 1, 65_536] {
        assert_eq!(
            PrecisionConfirmation::new(Some(info)).confirm(&frame, invalid_block),
            None
        );
    }

    let mut zero_block_info = info;
    zero_block_info[2..4].fill(0);
    assert_eq!(
        PrecisionConfirmation::new(Some(zero_block_info)).confirm(&frame, block_size),
        None
    );

    let mut different_bits = frame.clone();
    different_bits[3] = (different_bits[3] & !0x0e) | 0x02; // eight bits, versus 24
    let mut different_channels = frame.clone();
    different_channels[3] &= 0x0f; // mono, versus stereo
    for inconsistent in [different_bits, different_channels] {
        assert_eq!(
            PrecisionConfirmation::new(Some(info)).confirm(&inconsistent, block_size),
            None
        );
    }

    // The existing codec cannot hold a 33-bit side channel. Refuse before
    // asking it to decode that otherwise matching 32-bit stereo declaration.
    let mut info_32 = info;
    let mut packed = u64::from_be_bytes(info_32[10..18].try_into().expect("packed STREAMINFO"));
    packed = (packed & !(31 << 36)) | (31 << 36);
    info_32[10..18].copy_from_slice(&packed.to_be_bytes());
    let mut correlated_32 = frame;
    correlated_32[3] = (correlated_32[3] & 1) | 0xae;
    assert_eq!(
        PrecisionConfirmation::new(Some(info_32)).confirm(&correlated_32, block_size),
        None
    );
}
