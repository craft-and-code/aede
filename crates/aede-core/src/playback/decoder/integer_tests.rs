use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use aede_dsp::ChannelLayout;
use md5::{Digest, Md5};

use super::super::{Error, FlacMd5Status};
use super::IntegerFileDecoder;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(if name.starts_with("playback") {
            "tests/playback_fixtures/flac"
        } else {
            "tests/fixtures"
        })
        .join(name)
}

struct TemporarySource(PathBuf);

impl TemporarySource {
    fn new(bytes: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aede_integer_decode_{}_{}.audio",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for TemporarySource {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

fn read_to_end(decoder: &mut IntegerFileDecoder, buffer_frames: usize) -> Result<Vec<i32>, Error> {
    let channels = usize::from(decoder.format().channels());
    let mut samples = Vec::new();
    let mut buffer = vec![0; buffer_frames * channels];
    loop {
        let frames = decoder.read_frames(&mut buffer)?;
        if frames == 0 {
            return Ok(samples);
        }
        samples.extend_from_slice(&buffer[..frames * channels]);
    }
}

fn pcm_md5(samples: &[i32], bits: u32) -> String {
    let mut digest = Md5::new();
    for sample in samples {
        digest.update(&sample.to_le_bytes()[..(bits / 8) as usize]);
    }
    format!("{:x}", digest.finalize())
}

#[cfg(unix)]
#[test]
fn integer_admission_refuses_a_fifo_without_waiting_for_a_writer() {
    let source = TemporarySource::new(&[]);
    std::fs::remove_file(&source.0).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&source.0)
            .status()
            .unwrap()
            .success()
    );
    let opening = source.0.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = sender.send(IntegerFileDecoder::open(&opening).map(|_| ()));
    });
    let result = receiver.recv_timeout(std::time::Duration::from_secs(2));
    if result.is_err() {
        // Release the old blocking open before reporting this regression.
        let writer = std::fs::OpenOptions::new()
            .write(true)
            .open(&source.0)
            .unwrap();
        let _ = receiver.recv_timeout(std::time::Duration::from_secs(1));
        drop(writer);
    }
    worker.join().unwrap();
    assert!(
        matches!(result, Ok(Err(_))),
        "a FIFO needs no media writer to be refused"
    );
}

#[test]
fn native_flac_preserves_independently_verified_integer_pcm_and_format() {
    // Complete PCM digests and prefixes come from signed little-endian output
    // of reference flac 1.5.0, independently of this playback decoder.
    for (name, rate, channels, bits, frames, digest, prefix) in [
        (
            "track.flac",
            44_100,
            1,
            16,
            44_100,
            "8f523032fd7d2d351b19bf2c32666373",
            &[0, 51, 102, 153, 203, 252, 301, 348, 394, 438, 480, 521][..],
        ),
        (
            "playback-stereo.flac",
            44_100,
            2,
            16,
            18_432,
            "7e2d2157e4982d903552e068d16a097d",
            &[
                -2461, -992, 1201, -409, 1096, -2393, 2446, 38, 565, 10, -16, 2470,
            ][..],
        ),
        (
            "playback-real24.flac",
            48_000,
            2,
            24,
            18_432,
            "094731fdef7e6d68c1ee38926308ddba",
            &[
                -630004, 600928, 307378, 177023, 280518, 883048, 626076, 1267308, 144732, 74040,
                -4105, 467887,
            ][..],
        ),
        (
            "playback-padded24.flac",
            48_000,
            2,
            24,
            18_432,
            "50da7e50ca791bc6be7e1ca80ef96570",
            &[
                -630016, 600832, 307456, 176896, 280576, 882944, 626176, 1267200, 144640, 73984,
                -4096, 467968,
            ][..],
        ),
        (
            "playback-dualmono.flac",
            44_100,
            2,
            16,
            18_432,
            "80055b149f8a74aef381a1164895c5e2",
            &[
                -2734, -2734, -4382, -4382, -36, -36, -2315, -2315, -3654, -3654, 294, 294,
            ][..],
        ),
    ] {
        let mut decoder = IntegerFileDecoder::open(&fixture(name)).unwrap();
        let format = decoder.format();
        assert_eq!(format.sample_rate(), rate, "{name}");
        assert_eq!(format.channels(), channels, "{name}");
        assert_eq!(format.bits_per_sample(), bits, "{name}");
        assert_eq!(
            format.layout(),
            if channels == 1 {
                ChannelLayout::MONO
            } else {
                ChannelLayout::STEREO
            },
            "{name}"
        );
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Pending));
        let samples = read_to_end(&mut decoder, 127).unwrap();
        assert_eq!(samples.len(), frames * usize::from(channels), "{name}");
        assert_eq!(&samples[..prefix.len()], prefix, "{name}");
        assert_eq!(pcm_md5(&samples, bits), digest, "{name}");
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Verified));
        assert!(read_to_end(&mut decoder, 1).unwrap().is_empty());
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Verified));
    }
}

#[test]
fn public_integer_wav_reads_keep_reference_samples_format_and_seek_suffix() {
    let mut decoder = IntegerFileDecoder::open(&fixture("track.wav")).unwrap();
    assert_eq!(decoder.format().sample_rate(), 44_100);
    assert_eq!(decoder.format().bits_per_sample(), 16);
    assert_eq!(decoder.format().layout(), ChannelLayout::MONO);
    assert_eq!(decoder.flac_md5_status(), None);
    let samples = read_to_end(&mut decoder, 37).unwrap();
    assert_eq!(samples.len(), 11_025);
    // Independently decoded with FFmpeg to signed s16le; no float playback
    // buffer is used as an oracle for integer source preservation.
    assert_eq!(pcm_md5(&samples, 16), "4e741314017bcfd9a7012a9c128a703a");
    assert_eq!(
        &samples[..12],
        &[0, 51, 102, 153, 203, 252, 301, 348, 394, 438, 480, 521]
    );
    let mut suffix = IntegerFileDecoder::open(&fixture("track.wav")).unwrap();
    let skipped = suffix.skip_frames(4097, || false).unwrap();
    assert_eq!(skipped.frames, 4097);
    assert!(!skipped.reached_eof);
    assert_eq!(read_to_end(&mut suffix, 503).unwrap(), samples[4097..]);
    let mut empty = [99; 1];
    assert_eq!(suffix.read_frames(&mut empty).unwrap(), 0);
    assert_eq!(empty, [99]);
}

#[test]
fn real_and_padded_twenty_four_bit_sources_keep_their_low_bits_and_declared_depth() {
    for (name, low_bits_present) in [
        ("playback-real24.flac", true),
        ("playback-padded24.flac", false),
    ] {
        let mut decoder = IntegerFileDecoder::open(&fixture(name)).unwrap();
        assert_eq!(decoder.format().bits_per_sample(), 24);
        let samples = read_to_end(&mut decoder, 113).unwrap();
        assert_eq!(
            samples.iter().any(|sample| sample & 255 != 0),
            low_bits_present,
            "{name}"
        );
        assert!(
            samples
                .iter()
                .all(|sample| (-8_388_608..=8_388_607).contains(sample))
        );
    }
}

#[test]
fn small_and_large_reads_keep_the_same_integer_sample_order() {
    for name in [
        "track.flac",
        "track.wav",
        "playback-stereo.flac",
        "playback-real24.flac",
    ] {
        let mut small = IntegerFileDecoder::open(&fixture(name)).unwrap();
        let mut large = IntegerFileDecoder::open(&fixture(name)).unwrap();
        assert_eq!(
            read_to_end(&mut small, 1).unwrap(),
            read_to_end(&mut large, 4096).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn invalid_buffers_leave_their_contents_and_source_position_unchanged() {
    let mut decoder = IntegerFileDecoder::open(&fixture("playback-stereo.flac")).unwrap();
    assert!(matches!(
        decoder.read_frames(&mut []),
        Err(Error::InvalidBuffer)
    ));
    let mut incomplete = [71, 72, 73];
    assert!(matches!(
        decoder.read_frames(&mut incomplete),
        Err(Error::InvalidBuffer)
    ));
    assert_eq!(incomplete, [71, 72, 73]);
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Pending));
    let mut first = [0; 2];
    assert_eq!(decoder.read_frames(&mut first).unwrap(), 1);
    assert_eq!(first, [-2461, -992]);
}

#[test]
fn short_integer_reads_leave_the_unused_output_tail_unchanged() {
    let mut decoder = IntegerFileDecoder::open(&fixture("track.flac")).unwrap();
    let mut output = vec![123_456; 100_000];
    let frames = decoder.read_frames(&mut output).unwrap();
    assert!(frames > 0 && frames < output.len());
    assert_eq!(&output[..4], &[0, 51, 102, 153]);
    assert!(output[frames..].iter().all(|&sample| sample == 123_456));
}

#[test]
fn reading_only_a_prefix_does_not_verify_the_complete_integer_source() {
    let mut decoder = IntegerFileDecoder::open(&fixture("track.flac")).unwrap();
    let mut first = [0];
    assert_eq!(decoder.read_frames(&mut first).unwrap(), 1);
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Pending));
}

#[test]
fn absent_flac_signatures_stay_absent_after_complete_integer_decoding() {
    for unknown_length in [false, true] {
        let mut bytes = std::fs::read(fixture("track.flac")).unwrap();
        bytes[26..42].fill(0);
        if unknown_length {
            bytes[21] &= 0xf0;
            bytes[22..26].fill(0);
        }
        let source = TemporarySource::new(&bytes);
        let mut decoder = IntegerFileDecoder::open(&source.0).unwrap();
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::NoSignature));
        assert_eq!(read_to_end(&mut decoder, 127).unwrap().len(), 44_100);
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::NoSignature));
    }
    let mut wav = IntegerFileDecoder::open(&fixture("track.wav")).unwrap();
    assert_eq!(wav.flac_md5_status(), None);
    assert_eq!(read_to_end(&mut wav, 127).unwrap().len(), 11_025);
    assert_eq!(wav.flac_md5_status(), None);
}

#[test]
fn unknown_flac_length_still_allows_verification_at_complete_eof() {
    let mut bytes = std::fs::read(fixture("track.flac")).unwrap();
    bytes[21] &= 0xf0;
    bytes[22..26].fill(0);
    let source = TemporarySource::new(&bytes);
    let mut decoder = IntegerFileDecoder::open(&source.0).unwrap();
    assert_eq!(read_to_end(&mut decoder, 127).unwrap().len(), 44_100);
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Verified));
}

#[test]
fn an_integer_flac_md5_mismatch_is_a_sticky_error() {
    let mut bytes = std::fs::read(fixture("playback-real24.flac")).unwrap();
    bytes[26] ^= 1;
    let source = TemporarySource::new(&bytes);
    let mut decoder = IntegerFileDecoder::open(&source.0).unwrap();
    assert!(matches!(
        read_to_end(&mut decoder, 127),
        Err(Error::FlacMd5Mismatch)
    ));
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Mismatch));
    assert!(matches!(
        read_to_end(&mut decoder, 127),
        Err(Error::FlacMd5Mismatch)
    ));
}

#[test]
fn missing_signatures_do_not_hide_truncated_or_misdeclared_integer_audio() {
    for truncated in [false, true] {
        let mut bytes = std::fs::read(fixture("track.flac")).unwrap();
        bytes[26..42].fill(0);
        if truncated {
            bytes.truncate(bytes.len() - 10);
        } else {
            let packed = u64::from_be_bytes(bytes[18..26].try_into().unwrap()) + 1;
            bytes[18..26].copy_from_slice(&packed.to_be_bytes());
        }
        let source = TemporarySource::new(&bytes);
        let mut decoder = IntegerFileDecoder::open(&source.0).unwrap();
        assert!(read_to_end(&mut decoder, 127).is_err());
        assert!(read_to_end(&mut decoder, 127).is_err());
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::NoSignature));
    }
}

#[test]
fn decoding_uses_source_bytes_instead_of_a_filename_extension() {
    let bytes = std::fs::read(fixture("track.flac")).unwrap();
    let source = TemporarySource::new(&bytes);
    let mut decoder = IntegerFileDecoder::open(&source.0).unwrap();
    let samples = read_to_end(&mut decoder, 127).unwrap();
    assert_eq!(pcm_md5(&samples, 16), "8f523032fd7d2d351b19bf2c32666373");
    assert_eq!(std::fs::read(&source.0).unwrap(), bytes);
}

#[test]
fn native_integer_flac_retains_bounded_metadata_and_leading_id3_handling() {
    let path = fixture("track.flac");
    let original = std::fs::read(&path).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    let (_, audio_start) = crate::audit::flac::read_stream_info(&mut file).unwrap();
    for leading_id3 in [false, true] {
        let mut bytes = original[..42].to_vec();
        bytes[4] &= 0x7f;
        bytes.extend_from_slice(&[0x84, 0, 0, 4, 0xff, 0xff, 0xff, 0xff]);
        bytes.extend_from_slice(&original[audio_start as usize..]);
        if leading_id3 {
            bytes.splice(0..0, *b"ID3\x04\0\0\0\0\0\x04junk");
        }
        let source = TemporarySource::new(&bytes);
        let mut decoder = IntegerFileDecoder::open(&source.0).unwrap();
        let samples = read_to_end(&mut decoder, 127).unwrap();
        assert_eq!(pcm_md5(&samples, 16), "8f523032fd7d2d351b19bf2c32666373");
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Verified));
        assert_eq!(std::fs::read(&source.0).unwrap(), bytes);
    }
}

#[test]
fn integer_admission_refuses_sources_outside_the_initial_profile() {
    for name in [
        "playback-ogg-flac.oga",
        "playback32-mono.flac",
        "playback32-stereo.flac",
        "playback32-mid-side.flac",
        "track.mp3",
        "track.ogg",
        "track.opus",
        "track.aac",
        "track.m4a",
        "track.aiff",
    ] {
        assert!(IntegerFileDecoder::open(&fixture(name)).is_err(), "{name}");
    }
    let surround =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/channel_fixtures/surround-5_1.wav");
    assert!(IntegerFileDecoder::open(&surround).is_err());
}

#[test]
fn progressive_integer_seek_preserves_the_exact_suffix_and_verifies_at_eof() {
    for name in ["track.flac", "track.wav", "playback-real24.flac"] {
        let mut whole = IntegerFileDecoder::open(&fixture(name)).unwrap();
        let channels = usize::from(whole.format().channels());
        let reference = read_to_end(&mut whole, 127).unwrap();
        let total_frames = reference.len() / channels;
        for requested in [0, 1, 4097, total_frames, total_frames + 17] {
            let mut decoder = IntegerFileDecoder::open(&fixture(name)).unwrap();
            let seek = decoder.skip_frames(requested as u64, || false).unwrap();
            let skipped = requested.min(total_frames);
            assert_eq!(seek.frames, skipped as u64, "{name}");
            assert_eq!(seek.reached_eof, requested > total_frames, "{name}");
            assert_eq!(
                read_to_end(&mut decoder, 113).unwrap(),
                reference[skipped * channels..],
                "{name}: seek to {requested}"
            );
            assert_eq!(
                decoder.flac_md5_status(),
                if name.ends_with(".flac") {
                    Some(FlacMd5Status::Verified)
                } else {
                    None
                }
            );
        }
    }
}

#[test]
fn integer_seek_counts_frames_and_can_follow_an_existing_read() {
    let mut decoder = IntegerFileDecoder::open(&fixture("playback-stereo.flac")).unwrap();
    let mut frame = [0; 2];
    assert_eq!(decoder.read_frames(&mut frame).unwrap(), 1);
    assert_eq!(frame, [-2461, -992]);
    let seek = decoder.skip_frames(7, || false).unwrap();
    assert_eq!(seek.frames, 7);
    assert!(!seek.reached_eof);
    assert_eq!(decoder.read_frames(&mut frame).unwrap(), 1);
    assert_eq!(frame, [-3396, -4429]);
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Pending));
}

#[test]
fn cancelled_integer_seeks_do_not_verify_a_complete_source() {
    for requested in [0, u64::MAX] {
        let mut decoder = IntegerFileDecoder::open(&fixture("track.flac")).unwrap();
        assert!(matches!(
            decoder.skip_frames(requested, || true),
            Err(Error::SeekCancelled)
        ));
        assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Pending));
    }
    let mut decoder = IntegerFileDecoder::open(&fixture("track.flac")).unwrap();
    let mut checks = 0;
    assert!(matches!(
        decoder.skip_frames(u64::MAX, || {
            checks += 1;
            checks > 2
        }),
        Err(Error::SeekCancelled)
    ));
    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Pending));
}
