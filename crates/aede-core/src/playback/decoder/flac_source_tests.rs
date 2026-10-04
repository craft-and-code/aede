use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

struct TemporarySource(PathBuf);

impl TemporarySource {
    fn new(bytes: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aede_flac_source_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }

    fn open(&self) -> File {
        File::open(&self.0).unwrap()
    }
}

impl Drop for TemporarySource {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

fn metadata(kind: u8, last: bool, body: &[u8]) -> Vec<u8> {
    let mut block = vec![kind | if last { 0x80 } else { 0 }];
    block.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
    block.extend_from_slice(body);
    block
}

fn streaminfo() -> [u8; 34] {
    let mut info = [0; 34];
    for (index, byte) in info.iter_mut().enumerate() {
        *byte = index as u8;
    }
    info
}

fn native_with(blocks: &[Vec<u8>], audio: &[u8]) -> Vec<u8> {
    let mut bytes = b"fLaC".to_vec();
    for block in blocks {
        bytes.extend_from_slice(block);
    }
    bytes.extend_from_slice(audio);
    bytes
}

#[test]
fn virtual_reads_and_seeks_keep_streaminfo_and_original_audio_bytes() {
    let info = streaminfo();
    let audio = b"original encoded audio";
    let bytes = native_with(
        &[
            metadata(0, false, &info),
            metadata(6, false, b"unneeded artwork"),
            metadata(4, true, b"unneeded comments"),
        ],
        audio,
    );
    let file = TemporarySource::new(&bytes);
    let mut source = FlacAudioSource::new(file.open()).unwrap().unwrap();
    let mut expected = b"fLaC\x80\0\0\x22".to_vec();
    expected.extend_from_slice(&info);
    expected.extend_from_slice(audio);
    assert_eq!(source.byte_len(), Some(expected.len() as u64));
    assert!(source.is_seekable());
    let mut read = Vec::new();
    source.read_to_end(&mut read).unwrap();
    assert_eq!(read, expected);
    for start in [0, 3, 7, 8, 40, 41, 42, 43, expected.len() - 1] {
        source.seek(SeekFrom::Start(start as u64)).unwrap();
        let mut suffix = Vec::new();
        source.read_to_end(&mut suffix).unwrap();
        assert_eq!(suffix, expected[start..], "virtual offset {start}");
    }
    assert_eq!(
        source.seek(SeekFrom::End(-4)).unwrap(),
        expected.len() as u64 - 4
    );
    let mut tail = [0; 4];
    source.read_exact(&mut tail).unwrap();
    assert_eq!(&tail, &expected[expected.len() - 4..]);
    source.seek(SeekFrom::Start(43)).unwrap();
    assert_eq!(source.seek(SeekFrom::Current(-2)).unwrap(), 41);
    let mut across = [0; 5];
    source.read_exact(&mut across).unwrap();
    assert_eq!(&across, &expected[41..46]);
    assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
}

#[test]
fn leading_id3_is_skipped_without_exposing_it_to_the_demuxer() {
    let mut bytes = b"ID3\x04\0\0\0\0\0\x04junk".to_vec();
    bytes.extend_from_slice(&native_with(&[metadata(0, true, &streaminfo())], b"audio"));
    let file = TemporarySource::new(&bytes);
    let mut source = FlacAudioSource::new(file.open()).unwrap().unwrap();
    let mut actual = Vec::new();
    source.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, bytes[14..]);
    for bytes in [
        b"ID3\x04\0\0\0\0\0\x04junkMP3!".as_slice(),
        b"OggS not native FLAC".as_slice(),
        b"tiny".as_slice(),
        b"".as_slice(),
    ] {
        let file = TemporarySource::new(bytes);
        assert!(FlacAudioSource::new(file.open()).unwrap().is_none());
    }
}

#[test]
fn nested_metadata_lengths_are_never_parsed_or_allocated() {
    let bytes = native_with(
        &[
            metadata(0, false, &streaminfo()),
            metadata(4, false, &u32::MAX.to_le_bytes()),
            metadata(6, true, &[0xff; 64]),
        ],
        b"audio",
    );
    let file = TemporarySource::new(&bytes);
    let mut source = FlacAudioSource::new(file.open()).unwrap().unwrap();
    source.seek(SeekFrom::Start(PREFIX_BYTES as u64)).unwrap();
    let mut audio = Vec::new();
    source.read_to_end(&mut audio).unwrap();
    assert_eq!(audio, b"audio");
}

#[test]
fn large_valid_optional_blocks_are_skipped_without_an_artwork_size_cap() {
    let bytes = native_with(
        &[
            metadata(0, false, &streaminfo()),
            metadata(6, true, &vec![0xff; 8 * 1024 * 1024 + 1]),
        ],
        b"audio",
    );
    let file = TemporarySource::new(&bytes);
    let mut source = FlacAudioSource::new(file.open()).unwrap().unwrap();
    let mut actual = Vec::new();
    source.read_to_end(&mut actual).unwrap();
    assert_eq!(actual.len(), PREFIX_BYTES + 5);
    assert_eq!(&actual[PREFIX_BYTES..], b"audio");
}

#[test]
fn truncated_invalid_or_duplicate_metadata_is_refused_before_probing() {
    let valid = native_with(&[metadata(0, true, &streaminfo())], b"");
    for length in [4, 5, 7, 8, 20, 41] {
        let file = TemporarySource::new(&valid[..length]);
        assert!(
            FlacAudioSource::new(file.open()).is_err(),
            "prefix {length}"
        );
    }
    for blocks in [
        vec![metadata(1, true, &streaminfo())],
        vec![metadata(0, true, &[0; 33])],
        vec![metadata(0, true, &[0; 35])],
        vec![
            metadata(0, false, &streaminfo()),
            metadata(0, true, &streaminfo()),
        ],
        vec![metadata(0, false, &streaminfo()), metadata(127, true, b"")],
        vec![metadata(0, false, &streaminfo())],
        vec![
            metadata(0, false, &streaminfo()),
            vec![0x81, 0xff, 0xff, 0xff],
        ],
    ] {
        let file = TemporarySource::new(&native_with(&blocks, b""));
        assert!(FlacAudioSource::new(file.open()).is_err());
    }
    let file = TemporarySource::new(b"ID3\x04\0\0\x7f\x7f\x7f\x7f");
    assert!(FlacAudioSource::new(file.open()).is_err());
}

#[test]
fn virtual_seeks_reject_negative_or_overflow_positions_and_allow_eof() {
    let file = TemporarySource::new(&native_with(
        &[metadata(0, false, &streaminfo()), metadata(1, true, b"")],
        b"audio",
    ));
    let mut source = FlacAudioSource::new(file.open()).unwrap().unwrap();
    assert!(source.seek(SeekFrom::Current(-1)).is_err());
    assert_eq!(source.stream_position().unwrap(), 0);
    let end = source.byte_len().unwrap();
    assert!(source.seek(SeekFrom::End(-(end as i64) - 1)).is_err());
    assert_eq!(source.seek(SeekFrom::End(0)).unwrap(), end);
    assert_eq!(source.read(&mut [0; 8]).unwrap(), 0);
    assert_eq!(source.seek(SeekFrom::End(7)).unwrap(), end + 7);
    assert_eq!(source.read(&mut [0; 8]).unwrap(), 0);
    assert!(source.seek(SeekFrom::Start(u64::MAX)).is_err());
    assert_eq!(source.stream_position().unwrap(), end + 7);
}

#[test]
fn zero_length_optional_headers_cannot_create_unbounded_opening_work() {
    let mut bytes = native_with(&[metadata(0, false, &streaminfo())], b"");
    for _ in 1..MAX_METADATA_BLOCKS {
        bytes.extend_from_slice(&[1, 0, 0, 0]);
    }
    // The bound includes STREAMINFO. A final header at the limit is accepted.
    let final_header = bytes.len() - 4;
    bytes[final_header] |= 0x80;
    {
        let file = TemporarySource::new(&bytes);
        assert!(FlacAudioSource::new(file.open()).unwrap().is_some());
    }
    bytes[final_header] &= 0x7f;
    bytes.extend_from_slice(&[0x81, 0, 0, 0]);
    let file = TemporarySource::new(&bytes);
    let error = FlacAudioSource::new(file.open()).err().unwrap();
    assert!(error.to_string().contains("header-count limit"));
}

#[test]
fn the_virtual_length_and_reads_remain_bounded_by_the_opened_file_snapshot() {
    use std::io::Write as _;

    let file = TemporarySource::new(&native_with(&[metadata(0, true, &streaminfo())], b"audio"));
    let mut source = FlacAudioSource::new(file.open()).unwrap().unwrap();
    let captured_length = source.byte_len().unwrap();
    let mut append = std::fs::OpenOptions::new()
        .append(true)
        .open(&file.0)
        .unwrap();
    append.write_all(b"new bytes after opening").unwrap();
    let mut read = Vec::new();
    source.read_to_end(&mut read).unwrap();
    assert_eq!(source.byte_len(), Some(captured_length));
    assert_eq!(read.len() as u64, captured_length);
    assert_eq!(&read[PREFIX_BYTES..], b"audio");
}
