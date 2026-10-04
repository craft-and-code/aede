use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    dir: PathBuf,
    audio: PathBuf,
    sidecar: PathBuf,
    metadata: Metadata,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aede_current_lyrics_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let audio = dir.join("01.wav");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.wav"),
            &audio,
        )
        .unwrap();
        let sidecar = super::super::sidecar_of(&audio);
        let metadata = fs::metadata(&audio).unwrap();
        Self {
            dir,
            audio,
            sidecar,
            metadata,
        }
    }

    fn source<'a>(&'a self, tag: Option<&'a str>, sidecar: bool) -> CurrentTrack<'a> {
        CurrentTrack {
            path: &self.audio,
            size: self.metadata.len(),
            mtime: crate::clock::mtime_seconds(&self.metadata),
            mtime_subseconds: crate::clock::mtime_subseconds(&self.metadata),
            tag,
            sidecar: sidecar.then_some(self.sidecar.as_path()),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn complete_current_tag_precedes_sidecar_and_preserves_timed_blank_lines() {
    let fixture = Fixture::new();
    fs::write(&fixture.sidecar, "sidecar words").unwrap();
    let words = read_current(fixture.source(
        Some("[offset:250]\n[00:01.00][00:03.00]chorus\n[00:05.00]\n"),
        true,
    ))
    .unwrap()
    .unwrap();
    assert_eq!(words.source, Source::Tag);
    assert_eq!(words.origin, fixture.audio.to_string_lossy());
    assert_eq!(words.lines.len(), 3);
    assert_eq!(words.lines[0].at_ms, Some(1_250));
    assert_eq!(words.lines[1].at_ms, Some(3_250));
    assert_eq!(words.lines[2].at_ms, Some(5_250));
    assert_eq!(words.lines[2].text, "");
}

#[test]
fn complete_sidecar_reads_lossy_text_and_plain_tag_fallback_without_truncation() {
    let fixture = Fixture::new();
    fs::write(&fixture.sidecar, b"[00:01]A\xffB\n[00:02]second").unwrap();
    let words = read_current(fixture.source(Some("[ar:metadata only]"), true))
        .unwrap()
        .unwrap();
    assert_eq!(words.source, Source::Sidecar);
    assert_eq!(words.lines.len(), 2);
    assert_eq!(words.lines[0].text, "A\u{fffd}B");
    assert_eq!(words.lines[1].text, "second");
}

#[test]
fn complete_absent_or_empty_lyrics_are_distinct_from_unavailable_sources() {
    let fixture = Fixture::new();
    assert_eq!(read_current(fixture.source(None, false)).unwrap(), None);
    assert_eq!(
        read_current(fixture.source(Some("[ar:metadata only]"), false)).unwrap(),
        None
    );
    assert_eq!(
        read_current(fixture.source(None, true)),
        Err(ReadError::SidecarUnavailable)
    );
    fs::remove_file(&fixture.audio).unwrap();
    assert_eq!(
        read_current(fixture.source(None, false)),
        Err(ReadError::SourceUnavailable)
    );
}

#[test]
fn complete_lyrics_refuse_oversized_sources_and_expansion_instead_of_returning_a_prefix() {
    let fixture = Fixture::new();
    let too_big = "x".repeat(MAX_INPUT_BYTES as usize + 1);
    assert_eq!(
        read_current(fixture.source(Some(&too_big), false)),
        Err(ReadError::TooLarge)
    );
    fs::write(&fixture.sidecar, &too_big).unwrap();
    assert_eq!(
        read_current(fixture.source(None, true)),
        Err(ReadError::TooLarge)
    );
    let expansion = format!("{}{}", "[00:01]".repeat(1000), "x".repeat(2000));
    assert_eq!(
        read_current(fixture.source(Some(&expansion), false)),
        Err(ReadError::TooLarge)
    );
    fs::write(&fixture.sidecar, &expansion).unwrap();
    assert_eq!(
        read_current(fixture.source(None, true)),
        Err(ReadError::TooLarge)
    );
}

#[test]
fn complete_lyrics_require_precise_current_audio_metadata_and_regular_sources() {
    let fixture = Fixture::new();
    let mut source = fixture.source(Some("words"), false);
    source.mtime_subseconds = source.mtime_subseconds.wrapping_add(1);
    assert_eq!(read_current(source), Err(ReadError::SourceChanged));
    let mut source = fixture.source(Some("words"), false);
    source.size += 1;
    assert_eq!(read_current(source), Err(ReadError::SourceChanged));
    fs::create_dir(&fixture.sidecar).unwrap();
    assert_eq!(
        read_current(fixture.source(None, true)),
        Err(ReadError::SidecarChanged)
    );
}

#[cfg(unix)]
#[test]
fn complete_lyrics_refuse_pre_epoch_audio_instead_of_accepting_zero_identity() {
    let fixture = Fixture::new();
    let modified = std::time::UNIX_EPOCH - std::time::Duration::from_secs(1);
    File::options()
        .write(true)
        .open(&fixture.audio)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    assert_eq!(
        fs::metadata(&fixture.audio).unwrap().modified().unwrap(),
        modified
    );
    let source = CurrentTrack {
        mtime: 0,
        mtime_subseconds: 0,
        ..fixture.source(Some("stale tag words"), false)
    };
    assert_eq!(read_current(source), Err(ReadError::SourceChanged));
    assert_eq!(read_local(&fixture.audio), Err(ReadError::SourceChanged));
}

#[test]
fn complete_sidecars_require_the_same_basename_and_parent_but_accept_uppercase_extension() {
    let fixture = Fixture::new();
    let other = fixture.dir.join("other.lrc");
    fs::write(&other, "private words").unwrap();
    assert_eq!(
        read_current(CurrentTrack {
            sidecar: Some(&other),
            ..fixture.source(None, false)
        }),
        Err(ReadError::SidecarChanged)
    );
    let outside = fixture.dir.join("outside");
    fs::create_dir(&outside).unwrap();
    let other = outside.join("01.lrc");
    fs::write(&other, "private words").unwrap();
    assert_eq!(
        read_current(CurrentTrack {
            sidecar: Some(&other),
            ..fixture.source(None, false)
        }),
        Err(ReadError::SidecarChanged)
    );
    let uppercase = fixture.audio.with_extension("LRC");
    fs::write(&uppercase, "[00:01]complete words").unwrap();
    let lyrics = read_current(CurrentTrack {
        sidecar: Some(&uppercase),
        ..fixture.source(None, false)
    })
    .unwrap()
    .unwrap();
    assert_eq!(lyrics.lines[0].text, "complete words");
}

#[cfg(unix)]
#[test]
fn complete_lyrics_refuse_audio_and_sidecar_symbolic_links() {
    let fixture = Fixture::new();
    let target = fixture.dir.join("other.lrc");
    fs::write(&target, "private words").unwrap();
    std::os::unix::fs::symlink(&target, &fixture.sidecar).unwrap();
    assert_eq!(
        read_current(fixture.source(None, true)),
        Err(ReadError::SidecarChanged)
    );
    let target = fixture.dir.join("other.wav");
    fs::rename(&fixture.audio, &target).unwrap();
    std::os::unix::fs::symlink(&target, &fixture.audio).unwrap();
    assert_eq!(
        read_current(fixture.source(Some("words"), false)),
        Err(ReadError::SourceChanged)
    );
    assert_eq!(read_local(&fixture.audio), Err(ReadError::SourceChanged));
}

#[test]
fn local_uncatalogued_lyrics_use_existing_audio_parsers_and_report_malformed_audio() {
    let fixture = Fixture::new();
    assert_eq!(read_local(&fixture.audio).unwrap(), None);
    fs::write(&fixture.sidecar, "[00:01]local words").unwrap();
    let lyrics = read_local(&fixture.audio).unwrap().unwrap();
    assert_eq!(lyrics.source, Source::Sidecar);
    assert_eq!(lyrics.lines[0].at_ms, Some(1000));

    // Insert one complete Vorbis-comment block into a real FLAC fixture,
    // preserving its encoded audio and existing metadata. Signature-based tag
    // dispatch also covers the intentionally retained .wav fixture filename.
    let mut bytes =
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.flac")).unwrap();
    let mut cursor = 4;
    loop {
        let last = bytes[cursor] & 0x80 != 0;
        let length = usize::from(bytes[cursor + 1]) * 65_536
            + usize::from(bytes[cursor + 2]) * 256
            + usize::from(bytes[cursor + 3]);
        if last {
            bytes[cursor] &= 0x7f;
            cursor += 4 + length;
            break;
        }
        cursor += 4 + length;
    }
    let field = b"LYRICS=[00:02]embedded words";
    let mut comment = Vec::new();
    comment.extend_from_slice(&0_u32.to_le_bytes());
    comment.extend_from_slice(&1_u32.to_le_bytes());
    comment.extend_from_slice(&(field.len() as u32).to_le_bytes());
    comment.extend_from_slice(field);
    let mut block = vec![0x84, 0, 0, comment.len() as u8];
    block.extend_from_slice(&comment);
    bytes.splice(cursor..cursor, block);
    fs::write(&fixture.audio, bytes).unwrap();
    fs::remove_file(&fixture.sidecar).unwrap();
    fs::create_dir(&fixture.sidecar).unwrap();
    let lyrics = read_local(&fixture.audio).unwrap().unwrap();
    assert_eq!(lyrics.source, Source::Tag);
    assert_eq!(lyrics.lines[0].text, "embedded words");
    assert_eq!(lyrics.lines[0].at_ms, Some(2000));

    fs::write(&fixture.audio, "not an audio container").unwrap();
    assert_eq!(read_local(&fixture.audio), Err(ReadError::TagsUnavailable));
}
