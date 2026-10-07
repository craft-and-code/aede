use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::super::{Error, FileDecoder, FlacMd5Status};
use flaccompagnon_core::decode::PcmStreamDecoder;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(if name.starts_with("playback") {
            "tests/playback_fixtures/flac"
        } else {
            "tests/fixtures"
        })
        .join(name)
}

struct TemporaryFlac(PathBuf);

impl TemporaryFlac {
    fn new(bytes: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aede_flac_validation_{}_{}.flac",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

fn refresh_first_ogg_page_crc(bytes: &mut [u8]) {
    let segments = usize::from(bytes[26]);
    let page_len = 27
        + segments
        + bytes[27..27 + segments]
            .iter()
            .map(|&len| usize::from(len))
            .sum::<usize>();
    bytes[22..26].fill(0);
    let crc = crate::audit::crc::crc32_ogg(&bytes[..page_len]);
    bytes[22..26].copy_from_slice(&crc.to_le_bytes());
}

impl Drop for TemporaryFlac {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

fn read_to_end(decoder: &mut FileDecoder) -> Result<Vec<f32>, Error> {
    let mut samples = Vec::new();
    let mut buffer = vec![0.0; 127 * usize::from(decoder.channels())];
    loop {
        let frames = decoder.read_frames(&mut buffer)?;
        if frames == 0 {
            return Ok(samples);
        }
        samples.extend_from_slice(&buffer[..frames * usize::from(decoder.channels())]);
    }
}

#[test]
fn signed_source_samples_verify_before_float_conversion_without_changing_pcm() {
    for name in [
        "track.flac",
        "playback-ogg-flac.oga",
        "playback-stereo.flac",
        "playback-real24.flac",
        "playback-padded24.flac",
        "playback32-mono.flac",
        "playback32-stereo.flac",
    ] {
        let mut decoder = FileDecoder::open(&fixture(name)).unwrap();
        assert_eq!(
            decoder.flac_md5_status(),
            Some(FlacMd5Status::Pending),
            "{name}"
        );
        let samples = read_to_end(&mut decoder).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            decoder.flac_md5_status(),
            Some(FlacMd5Status::Verified),
            "{name}"
        );
        assert!(read_to_end(&mut decoder).unwrap().is_empty());
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Verified));
        let mut original = PcmStreamDecoder::open(&fixture(name)).unwrap();
        let mut reference = Vec::new();
        while let Some(chunk) = original.next_chunk().unwrap() {
            reference.extend(chunk);
        }
        assert_eq!(
            samples, reference,
            "{name}: enabling MD5 must not change playback samples"
        );
    }
}

#[test]
fn native_playback_verifies_audio_without_parsing_unneeded_nested_metadata() {
    for leading_id3 in [false, true] {
        let path = fixture("track.flac");
        let original = std::fs::read(&path).unwrap();
        let mut source = std::fs::File::open(&path).unwrap();
        let (_, audio_start) = crate::audit::flac::read_stream_info(&mut source).unwrap();
        let mut bytes = original[..42].to_vec();
        bytes[4] &= 0x7f;
        // A bounded outer comment block with an impossible vendor length must
        // never reach the demuxer's allocating textual metadata parser.
        bytes.extend_from_slice(&[0x84, 0, 0, 4, 0xff, 0xff, 0xff, 0xff]);
        bytes.extend_from_slice(&original[audio_start as usize..]);
        if leading_id3 {
            bytes.splice(0..0, *b"ID3\x04\0\0\0\0\0\x04junk");
        }
        let file = TemporaryFlac::new(&bytes);
        let mut decoder = FileDecoder::open(&file.0).unwrap();
        assert_eq!(read_to_end(&mut decoder).unwrap().len(), 44_100);
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Verified));
        assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
    }
}

#[test]
fn a_recognized_native_flac_probe_error_is_terminal_even_after_leading_id3() {
    for leading_id3 in [false, true] {
        let mut bytes = std::fs::read(fixture("track.flac")).unwrap();
        // A complete native STREAMINFO with a forbidden minimum block size
        // must be rejected instead of retried by an unverified decode path.
        bytes[8..10].fill(0);
        if leading_id3 {
            bytes.splice(0..0, *b"ID3\x04\0\0\0\0\0\x04junk");
        }
        let source = TemporaryFlac::new(&bytes);
        assert!(
            super::FlacStream::open(&source.0).is_err(),
            "recognized FLAC must keep its probe failure; leading ID3: {leading_id3}"
        );
        assert!(FileDecoder::open(&source.0).is_err());
        assert_eq!(std::fs::read(&source.0).unwrap(), bytes);
    }
}

#[test]
fn source_md5_verification_is_independent_of_playback_processing_and_resampling() {
    use crate::playback::stream::{PcmTrack, StreamError};

    for changed_output in [false, true] {
        for wrong_digest in [false, true] {
            let mut bytes = std::fs::read(fixture("playback-stereo.flac")).unwrap();
            if wrong_digest {
                bytes[26] ^= 1;
            }
            let source = TemporaryFlac::new(&bytes);
            let mut track = PcmTrack::open(&source.0).unwrap();
            if changed_output {
                track.set_output_rate(48_000).unwrap();
            }
            let mut delivered_frames = 0;
            loop {
                let result = track.read_block(|samples| {
                    if changed_output {
                        samples.fill(0.0);
                    }
                    Ok::<(), std::convert::Infallible>(())
                });
                match result {
                    Ok(Some(block)) => {
                        delivered_frames += block.frames;
                        if changed_output {
                            assert!(block.samples.iter().all(|sample| *sample == 0.0));
                        }
                    }
                    Ok(None) => {
                        assert!(!wrong_digest, "invalid source cannot complete normally");
                        assert_eq!(track.flac_md5_status(), Some(FlacMd5Status::Verified));
                        break;
                    }
                    Err(StreamError::Decode(Error::FlacMd5Mismatch)) => {
                        assert!(wrong_digest);
                        assert_eq!(track.flac_md5_status(), Some(FlacMd5Status::Mismatch));
                        assert!(matches!(
                            track.read_block(|_| Ok::<(), std::convert::Infallible>(())),
                            Err(StreamError::Decode(Error::FlacMd5Mismatch))
                        ));
                        break;
                    }
                    Err(error) => panic!("unexpected playback error: {error}"),
                }
            }
            assert!(delivered_frames > 0);
            assert_eq!(std::fs::read(&source.0).unwrap(), bytes);
        }
    }
}

#[test]
fn reading_a_prefix_or_cancelling_a_seek_does_not_verify_the_complete_source() {
    let mut decoder = FileDecoder::open(&fixture("track.flac")).unwrap();
    let mut buffer = [0.0];
    assert_eq!(decoder.read_frames(&mut buffer).unwrap(), 1);
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Pending));
    assert!(matches!(
        decoder.skip_frames(u64::MAX, || true),
        Err(Error::SeekCancelled)
    ));
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Pending));
    let mut decoder = FileDecoder::open(&fixture("track.flac")).unwrap();
    assert_eq!(
        decoder.skip_frames(12_345, || false).unwrap().frames,
        12_345
    );
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Pending));
    assert_eq!(read_to_end(&mut decoder).unwrap().len(), 44_100 - 12_345);
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Verified));
}

#[test]
fn missing_md5_or_unknown_length_never_becomes_a_verified_signature() {
    for unknown_length in [false, true] {
        let mut bytes = std::fs::read(fixture("track.flac")).unwrap();
        bytes[26..42].fill(0);
        if unknown_length {
            bytes[21] &= 0xf0;
            bytes[22..26].fill(0);
        }
        let file = TemporaryFlac::new(&bytes);
        let mut decoder = FileDecoder::open(&file.0).unwrap();
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::NoSignature));
        assert_eq!(read_to_end(&mut decoder).unwrap().len(), 44_100);
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::NoSignature));
    }
    let decoder = FileDecoder::open(&fixture("track.wav")).unwrap();
    assert_eq!(decoder.flac_md5_status(), None);
}

#[test]
fn missing_signature_does_not_hide_truncation_or_incorrect_declared_length() {
    for truncated in [false, true] {
        let mut bytes = std::fs::read(fixture("track.flac")).unwrap();
        bytes[26..42].fill(0);
        if truncated {
            bytes.truncate(bytes.len() - 10);
        } else {
            let mut packed = u64::from_be_bytes(bytes[18..26].try_into().unwrap());
            packed += 1;
            bytes[18..26].copy_from_slice(&packed.to_be_bytes());
        }
        let file = TemporaryFlac::new(&bytes);
        let mut decoder = FileDecoder::open(&file.0).unwrap();
        assert!(read_to_end(&mut decoder).is_err());
        assert!(
            read_to_end(&mut decoder).is_err(),
            "failed decoder must not subsequently return clean EOF"
        );
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::NoSignature));
    }
}

#[test]
fn an_unknown_length_with_a_signature_can_still_verify_the_complete_audio() {
    let mut bytes = std::fs::read(fixture("track.flac")).unwrap();
    bytes[21] &= 0xf0;
    bytes[22..26].fill(0);
    let file = TemporaryFlac::new(&bytes);
    let mut decoder = FileDecoder::open(&file.0).unwrap();
    assert_eq!(read_to_end(&mut decoder).unwrap().len(), 44_100);
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Verified));
}

#[test]
fn unsupported_thirty_three_bit_side_channels_are_refused_without_panicking() {
    let mut decoder = FileDecoder::open(&fixture("playback32-mid-side.flac")).unwrap();
    let error = read_to_end(&mut decoder).unwrap_err();
    assert!(
        error.to_string().contains("32-bit correlated FLAC stereo"),
        "{error}"
    );
    assert!(read_to_end(&mut decoder).is_err());
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Pending));
}

#[test]
fn ogg_flac_checks_its_audio_digest_and_distinguishes_an_absent_signature() {
    for missing in [false, true] {
        let mut bytes = std::fs::read(fixture("playback-ogg-flac.oga")).unwrap();
        let streaminfo = bytes.windows(4).position(|bytes| bytes == b"fLaC").unwrap();
        if missing {
            bytes[streaminfo + 26..streaminfo + 42].fill(0);
        } else {
            bytes[streaminfo + 26] ^= 1;
        }
        refresh_first_ogg_page_crc(&mut bytes);
        let file = TemporaryFlac::new(&bytes);
        let mut decoder = FileDecoder::open(&file.0).unwrap();
        if missing {
            assert_eq!(read_to_end(&mut decoder).unwrap().len(), 44_100);
            assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::NoSignature));
        } else {
            assert!(matches!(
                read_to_end(&mut decoder),
                Err(Error::FlacMd5Mismatch)
            ));
            assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Mismatch));
            assert!(matches!(
                read_to_end(&mut decoder),
                Err(Error::FlacMd5Mismatch)
            ));
        }
    }
}

#[test]
fn ogg_flac_frame_format_must_match_streaminfo_even_without_an_audio_signature() {
    for (shift, mask, value) in [(44, (1u64 << 20) - 1, 22_050), (41, 7, 1), (36, 31, 23)] {
        let mut bytes = std::fs::read(fixture("playback-ogg-flac.oga")).unwrap();
        let start = bytes.windows(4).position(|bytes| bytes == b"fLaC").unwrap();
        let packed = u64::from_be_bytes(bytes[start + 18..start + 26].try_into().unwrap());
        let packed = (packed & !(mask << shift)) | (value << shift);
        bytes[start + 18..start + 26].copy_from_slice(&packed.to_be_bytes());
        bytes[start + 26..start + 42].fill(0);
        refresh_first_ogg_page_crc(&mut bytes);
        let file = TemporaryFlac::new(&bytes);
        let mut decoder = FileDecoder::open(&file.0).unwrap();
        assert!(
            matches!(read_to_end(&mut decoder), Err(Error::InvalidFormat)),
            "header field at bit {shift}"
        );
        assert!(read_to_end(&mut decoder).is_err());
    }
}

#[test]
fn a_frame_gap_without_a_signature_or_declared_length_is_not_clean_eof() {
    let path = fixture("track.flac");
    let mut source = std::fs::File::open(&path).unwrap();
    let (_, audio_start) = crate::audit::flac::read_stream_info(&mut source).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    let mut stream = super::FlacStream::open(&path).unwrap().unwrap();
    let packet = stream.format.next_packet().unwrap();
    let first_frame_bytes = packet.data.len();
    assert_eq!(
        &bytes[audio_start as usize..audio_start as usize + first_frame_bytes],
        &*packet.data
    );
    bytes.drain(audio_start as usize..audio_start as usize + first_frame_bytes);
    bytes[26..42].fill(0);
    bytes[21] &= 0xf0;
    bytes[22..26].fill(0);
    let file = TemporaryFlac::new(&bytes);
    let mut decoder = FileDecoder::open(&file.0).unwrap();
    let error = read_to_end(&mut decoder).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("missing, repeated or out of order"),
        "{error}"
    );
    assert!(read_to_end(&mut decoder).is_err());
}
