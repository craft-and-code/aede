use super::*;

/// Builds an MPEG-1 Layer III header, 44.1 kHz, stereo, 128 kbit/s.
fn header_128() -> [u8; 4] {
    // 11111111 11111011 10010000 00000000
    [0xFF, 0xFB, 0x90, 0x00]
}

#[test]
fn mpeg1_layer3_frame_header() {
    let h = parse_frame_header(&header_128()).expect("valid header");
    assert_eq!(h.mpeg_version, MpegVersion::V1);
    assert_eq!(h.layer, 3);
    assert_eq!(h.sample_rate, 44_100);
    assert_eq!(h.bitrate_kbps, 128);
    assert_eq!(h.channels, 2);
    assert_eq!(h.samples_per_frame, 1152);
    // 1152/8 * 128000 / 44100 = 417 bytes
    assert_eq!(h.frame_len, 417);
}

#[test]
fn reserved_indices_rejected() {
    // Reserved version (0b01): 0xEB carries the version bits 01.
    assert!(parse_frame_header(&[0xFF, 0xEB, 0x90, 0x00]).is_none());
    // Reserved layer (0b00)
    assert!(parse_frame_header(&[0xFF, 0xF9, 0x90, 0x00]).is_none());
    // "free" bitrate (index 0)
    assert!(parse_frame_header(&[0xFF, 0xFB, 0x00, 0x00]).is_none());
    // Reserved sample rate (index 3)
    assert!(parse_frame_header(&[0xFF, 0xFB, 0x9C, 0x00]).is_none());
}

#[test]
fn xing_gives_the_frame_count() {
    let header = parse_frame_header(&header_128()).unwrap();
    let mut frame = header_128().to_vec();
    frame.extend(vec![0u8; 32]); // MPEG-1 stereo side info
    frame.extend_from_slice(b"Xing");
    frame.extend_from_slice(&0x0003u32.to_be_bytes()); // frames + bytes
    frame.extend_from_slice(&1000u32.to_be_bytes());
    frame.extend_from_slice(&500_000u32.to_be_bytes());
    let info = read_vbr_header(&frame, &header).expect("Xing detected");
    assert_eq!(info.frames, 1000);
    assert_eq!(info.bytes, 500_000);
}

#[test]
fn frame_detection_ignores_stray_bytes() {
    let header = header_128();
    let mut data = vec![0x00, 0xFF, 0x12, 0x34]; // false start
    let frame_len = parse_frame_header(&header).unwrap().frame_len;
    data.extend_from_slice(&header);
    data.extend(vec![0u8; frame_len - 4]);
    data.extend_from_slice(&header); // second frame, for confirmation
    data.extend(vec![0u8; frame_len - 4]);
    let (offset, parsed) = find_first_frame(&data).expect("frame found");
    assert_eq!(offset, 4);
    assert_eq!(parsed.sample_rate, 44_100);
}
