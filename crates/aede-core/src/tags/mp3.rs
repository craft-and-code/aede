//! MPEG audio (MP3): frame header, Xing/Info/VBRI VBR headers, ID3 tags.
//!
//! References: ISO/IEC 11172-3 for the frame header, the Xing/LAME
//! specification for variable bitrate.

use std::fs::File;

use super::bytes::{Cursor, read_at_most};
use super::{RawTags, TagError};

/// Reads the tags and the audio properties of an open MPEG audio file.
///
/// `file_size` bounds the search for an ID3v1 block at the end and feeds the
/// bitrate estimate when no Xing or VBRI header is present. A file carrying an
/// ID3v2 tag in front of a FLAC stream is detected here and handed over to
/// [`super::flac::read`], so the returned container may not be `mp3`; in front
/// of a raw AAC stream it yields [`TagError::UnrecognizedFormat`], which sends
/// the file to the fallback reader.
pub fn read(file: &mut File, file_size: u64) -> Result<RawTags, TagError> {
    let audio_start = super::id3::skip_id3v2(file)?;

    // A file may carry an ID3v2 in front of a stream that is not MP3: the most
    // common case is FLAC tagged by an old tool.
    let probe = read_at_most(file, audio_start, 4)?;
    if probe.len() == 4 && &probe[..] == b"fLaC" {
        return super::flac::read(file, file_size);
    }
    // Raw AAC is tagged the same way, and its ADTS sync word differs from an
    // MPEG one only by a layer field left at zero. Scanning on would end up
    // reading an audio payload as if it were a frame header.
    if probe.len() >= 2 && probe[0] == 0xFF && probe[1] & 0xE0 == 0xE0 && probe[1] & 0x06 == 0 {
        return Err(TagError::UnrecognizedFormat);
    }

    let mut tags = RawTags::default();
    tags.properties.container = "mp3".into();
    tags.properties.codec = "mp3".into();
    tags.properties.lossless = false;

    super::id3::read_id3v2(file, &mut tags)?;

    let end = detect_id3v1_start(file, file_size)?;
    analyze_stream(file, audio_start, end, &mut tags)?;

    super::id3::read_id3v1(file, file_size, &mut tags)?;
    Ok(tags)
}

/// End of the audio payload: before a possible 128-byte ID3v1.
fn detect_id3v1_start(file: &mut File, file_size: u64) -> Result<u64, TagError> {
    if file_size < 128 {
        return Ok(file_size);
    }
    let tail = read_at_most(file, file_size - 128, 3)?;
    if tail.len() == 3 && &tail[..] == b"TAG" {
        Ok(file_size - 128)
    } else {
        Ok(file_size)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FrameHeader {
    mpeg_version: MpegVersion,
    layer: u8,
    bitrate_kbps: u32,
    sample_rate: u32,
    channels: u16,
    padding: bool,
    frame_len: usize,
    samples_per_frame: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MpegVersion {
    V1,
    V2,
    V25,
}

fn analyze_stream(
    file: &mut File,
    audio_start: u64,
    audio_end: u64,
    tags: &mut RawTags,
) -> Result<(), TagError> {
    // 64 KiB is far more than enough to find the first valid frame, even with
    // stray bytes at the start.
    let window = read_at_most(file, audio_start, 64 * 1024)?;
    let Some((offset, header)) = find_first_frame(&window) else {
        return Ok(());
    };

    tags.properties.sample_rate = Some(header.sample_rate);
    tags.properties.channels = Some(header.channels);

    let frame = &window[offset..];
    let audio_len = audio_end.saturating_sub(audio_start + offset as u64);

    if let Some(vbr) = read_vbr_header(frame, &header) {
        if vbr.frames > 0 {
            let samples = vbr.frames as u64 * header.samples_per_frame as u64;
            let duration_ms = samples * 1000 / header.sample_rate as u64;
            tags.properties.duration_ms = Some(duration_ms);
            let stream_bytes = if vbr.bytes > 0 {
                vbr.bytes as u64
            } else {
                audio_len
            };
            tags.properties.bitrate_kbps = (stream_bytes * 8)
                .checked_div(duration_ms)
                .map(|kbps| kbps as u32);
        }
        if let Some((delay, padding)) = vbr.gapless {
            // Kept for the future playback engine: without these two values,
            // there is no clean gapless playback in MP3.
            tags.insert("encoder_delay", delay.to_string());
            tags.insert("encoder_padding", padding.to_string());
        }
    } else if header.bitrate_kbps > 0 {
        // Constant bitrate: the duration follows from the size.
        tags.properties.bitrate_kbps = Some(header.bitrate_kbps);
        tags.properties.duration_ms = Some(audio_len * 8 / header.bitrate_kbps as u64);
    }
    Ok(())
}

/// Looks for a valid frame, confirmed by the presence of a second coherent
/// frame right after it. This double check rules out false positives.
fn find_first_frame(data: &[u8]) -> Option<(usize, FrameHeader)> {
    let mut i = 0usize;
    while i + 4 <= data.len() {
        if data[i] == 0xFF
            && (data[i + 1] & 0xE0) == 0xE0
            && let Some(header) = parse_frame_header(&data[i..])
        {
            let next = i + header.frame_len;
            let confirmed = next + 4 > data.len()
                || (data[next] == 0xFF
                    && (data[next + 1] & 0xE0) == 0xE0
                    && parse_frame_header(&data[next..])
                        .map(|h| h.sample_rate == header.sample_rate)
                        .unwrap_or(false));
            if confirmed {
                return Some((i, header));
            }
        }
        i += 1;
    }
    None
}

fn parse_frame_header(data: &[u8]) -> Option<FrameHeader> {
    if data.len() < 4 {
        return None;
    }
    let (b1, b2, b3) = (data[1], data[2], data[3]);

    let mpeg_version = match (b1 >> 3) & 0x03 {
        0b00 => MpegVersion::V25,
        0b10 => MpegVersion::V2,
        0b11 => MpegVersion::V1,
        _ => return None, // 0b01 is reserved
    };
    let layer = match (b1 >> 1) & 0x03 {
        0b01 => 3,
        0b10 => 2,
        0b11 => 1,
        _ => return None,
    };

    let bitrate_index = (b2 >> 4) & 0x0F;
    if bitrate_index == 0 || bitrate_index == 0x0F {
        return None; // "free" and "bad": unusable
    }
    let sample_index = (b2 >> 2) & 0x03;
    if sample_index == 0x03 {
        return None;
    }
    let padding = (b2 >> 1) & 0x01 == 1;
    let channel_mode = (b3 >> 6) & 0x03;
    let channels: u16 = if channel_mode == 0b11 { 1 } else { 2 };

    let sample_rate = match (mpeg_version, sample_index) {
        (MpegVersion::V1, 0) => 44_100,
        (MpegVersion::V1, 1) => 48_000,
        (MpegVersion::V1, 2) => 32_000,
        (MpegVersion::V2, 0) => 22_050,
        (MpegVersion::V2, 1) => 24_000,
        (MpegVersion::V2, 2) => 16_000,
        (MpegVersion::V25, 0) => 11_025,
        (MpegVersion::V25, 1) => 12_000,
        (MpegVersion::V25, 2) => 8_000,
        _ => return None,
    };

    let bitrate_kbps = bitrate_table(mpeg_version, layer, bitrate_index)?;

    let samples_per_frame: u32 = match (layer, mpeg_version) {
        (1, _) => 384,
        (2, _) => 1152,
        (3, MpegVersion::V1) => 1152,
        (3, _) => 576,
        _ => return None,
    };

    let pad = if padding { 1usize } else { 0 };
    let frame_len = if layer == 1 {
        (12 * bitrate_kbps as usize * 1000 / sample_rate as usize + pad) * 4
    } else {
        (samples_per_frame as usize / 8) * bitrate_kbps as usize * 1000 / sample_rate as usize + pad
    };
    if frame_len < 24 {
        return None;
    }

    Some(FrameHeader {
        mpeg_version,
        layer,
        bitrate_kbps,
        sample_rate,
        channels,
        padding,
        frame_len,
        samples_per_frame,
    })
}

fn bitrate_table(version: MpegVersion, layer: u8, index: u8) -> Option<u32> {
    const V1L1: [u32; 15] = [
        0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
    ];
    const V1L2: [u32; 15] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
    ];
    const V1L3: [u32; 15] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const V2L1: [u32; 15] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256,
    ];
    const V2L23: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];

    let table = match (version, layer) {
        (MpegVersion::V1, 1) => &V1L1,
        (MpegVersion::V1, 2) => &V1L2,
        (MpegVersion::V1, 3) => &V1L3,
        (_, 1) => &V2L1,
        (_, _) => &V2L23,
    };
    table.get(index as usize).copied().filter(|&b| b > 0)
}

#[derive(Debug, Default)]
struct VbrInfo {
    frames: u32,
    bytes: u32,
    /// Encoder delay and trailing padding, both required for gapless playback.
    gapless: Option<(u32, u32)>,
}

fn read_vbr_header(frame: &[u8], header: &FrameHeader) -> Option<VbrInfo> {
    // The Xing header sits after the side info, whose size depends on the
    // version and the channel mode.
    let side_info = match (header.mpeg_version, header.channels) {
        (MpegVersion::V1, 1) => 17,
        (MpegVersion::V1, _) => 32,
        (_, 1) => 9,
        (_, _) => 17,
    };

    if let Some(info) = read_xing(frame, 4 + side_info) {
        return Some(info);
    }
    read_vbri(frame, 4 + 32)
}

fn read_xing(frame: &[u8], offset: usize) -> Option<VbrInfo> {
    if frame.len() < offset + 8 {
        return None;
    }
    let magic = &frame[offset..offset + 4];
    if magic != b"Xing" && magic != b"Info" {
        return None;
    }
    let mut c = Cursor::new(&frame[offset + 4..]);
    let flags = c.u32_be()?;
    let mut info = VbrInfo::default();
    if flags & 0x0001 != 0 {
        info.frames = c.u32_be()?;
    }
    if flags & 0x0002 != 0 {
        info.bytes = c.u32_be()?;
    }
    if flags & 0x0004 != 0 {
        c.skip(100); // seek table
    }
    if flags & 0x0008 != 0 {
        c.skip(4); // quality indicator
    }

    // LAME tag: the version string takes 9 bytes, and the delay/padding pair
    // sits 21 bytes further on.
    let lame_start = offset + 4 + c.position();
    if frame.len() >= lame_start + 24 {
        let tag = &frame[lame_start..lame_start + 4];
        if tag == b"LAME" || tag == b"Lavc" || tag == b"Lavf" {
            let d = &frame[lame_start + 21..lame_start + 24];
            let delay = ((d[0] as u32) << 4) | ((d[1] as u32) >> 4);
            let padding = (((d[1] as u32) & 0x0F) << 8) | d[2] as u32;
            info.gapless = Some((delay, padding));
        }
    }
    Some(info)
}

fn read_vbri(frame: &[u8], offset: usize) -> Option<VbrInfo> {
    if frame.len() < offset + 26 || &frame[offset..offset + 4] != b"VBRI" {
        return None;
    }
    let mut c = Cursor::new(&frame[offset + 4..]);
    c.skip(2 + 2 + 2); // version, delay, quality
    let bytes = c.u32_be()?;
    let frames = c.u32_be()?;
    Some(VbrInfo {
        frames,
        bytes,
        gapless: None,
    })
}

#[cfg(test)]
mod tests {
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
}
