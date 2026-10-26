use super::*;

#[test]
fn native_opus_spectrogram_never_resolves_ffmpeg() {
    let files = SpectralFiles::new();
    let audio = files.0.join("native.opus");
    std::fs::copy(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures/opus-stereo.opus"),
        &audio,
    )
    .unwrap();
    let picture = picture_for(&audio);
    let before = std::fs::read(&audio).unwrap();
    render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
        panic!("native Opus must not resolve FFmpeg")
    })
    .unwrap();
    assert!(!out_of_date(&audio, &picture));
    assert_eq!(std::fs::read(&audio).unwrap(), before);
    let png = std::fs::read(&picture).unwrap();
    assert!(
        render_cancellable(&audio, &picture, Size::Half, &|| true, &|| {
            panic!("cancelled Opus must not resolve FFmpeg")
        })
        .is_err()
    );
    assert_eq!(std::fs::read(&picture).unwrap(), png);
}

#[test]
fn rendering_publishes_only_a_completed_picture() {
    for succeeds in [false, true] {
        let dir = std::env::temp_dir().join(format!(
            "aede_spectrum_publication_{}_{succeeds}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let picture = dir.join("one.png");
        std::fs::write(&picture, b"previous picture").unwrap();
        let mut staged = None;
        let result = publish_with(&picture, &|| false, |output| {
            assert_ne!(output, picture.as_path());
            assert_eq!(output.extension().unwrap(), "png");
            staged = Some(output.to_path_buf());
            std::fs::write(output, b"completed picture").unwrap();
            if succeeds {
                Ok(())
            } else {
                Err("render failed".into())
            }
        });
        assert_eq!(result.is_ok(), succeeds);
        assert_eq!(
            std::fs::read(&picture).unwrap(),
            if succeeds {
                b"completed picture".as_slice()
            } else {
                b"previous picture".as_slice()
            }
        );
        assert!(!staged.unwrap().exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn rendering_refuses_destination_links_before_work_and_before_publication() {
    let dir = std::env::temp_dir().join(format!(
        "aede_spectrum_destination_links_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let private = dir.join("accounts.json");
    let picture = dir.join("one.png");
    std::fs::write(&private, b"private account state").unwrap();
    std::os::unix::fs::symlink(&private, &picture).unwrap();
    let error = publish_with(&picture, &|| false, |_| {
        panic!("a linked destination must be refused before starting the renderer")
    })
    .unwrap_err();
    assert!(error.contains("non-regular destination"), "{error}");
    assert_eq!(std::fs::read(&private).unwrap(), b"private account state");
    std::fs::remove_file(&picture).unwrap();

    let mut staged = None;
    let error = publish_with(&picture, &|| false, |output| {
        staged = Some(output.to_path_buf());
        std::fs::write(output, b"new picture").unwrap();
        std::os::unix::fs::symlink(&private, &picture).unwrap();
        Ok(())
    })
    .unwrap_err();
    assert!(error.contains("non-regular destination"), "{error}");
    assert_eq!(std::fs::read(&private).unwrap(), b"private account state");
    assert!(
        std::fs::symlink_metadata(&picture)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(!staged.unwrap().exists());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

fn properties(codec: &str, bits: Option<u16>) -> AudioProperties {
    AudioProperties {
        codec: codec.to_string(),
        sample_rate: Some(44_100),
        bit_depth: bits,
        channels: Some(2),
        bitrate_kbps: Some(320),
        ..Default::default()
    }
}

#[test]
fn a_picture_sits_in_a_folder_of_its_own_beside_its_track() {
    let got = picture_for(Path::new("/m/Album/01 So What.flac"));
    assert_eq!(got, Path::new("/m/Album/spectrograms/01 So What.png"));
    // A name with no extension still gets a picture rather than a panic.
    assert_eq!(
        picture_for(Path::new("/m/Album/oddity")),
        Path::new("/m/Album/spectrograms/oddity.png")
    );
}

#[test]
fn the_caption_says_what_the_file_claims_to_be() {
    assert_eq!(
        caption(&properties("flac", Some(16))),
        "44100 Hz | 16-bit | 2 ch | FLAC | Nyquist 22050 Hz"
    );
    // A lossy codec has no bit depth at all, and saying "float" there —
    // which a tool that only ever sees lossless files can afford — would
    // state something untrue about every MP3 in the library.
    assert_eq!(
        caption(&properties("mp3", None)),
        "44100 Hz | 320 kbps | 2 ch | MP3 | Nyquist 22050 Hz"
    );
}

/// Every character that could end the `drawtext` argument, start another
/// filter, or escape out of the expression must be dropped. The codec name
/// is read from the file, so it is attacker-chosen: a file declaring a
/// codec of `x'a,b` would otherwise inject into the filter graph ffmpeg is
/// handed. This is the test the character filter exists for — widen that
/// set and it fails here rather than in somebody's music folder.
#[test]
fn the_caption_cannot_break_out_of_the_filter_graph() {
    let hostile = "A'B\"C:D,E\\F;G=H[I]J{K}L`M$N\nO\rP\tQ%R*S?T<U>V&W(X)";
    let text = caption(&properties(hostile, Some(16)));
    for bad in [
        '\'', '"', ':', ',', '\\', ';', '=', '[', ']', '{', '}', '`', '$', '\n', '\r', '\t', '%',
        '*', '?', '<', '>', '&', '(', ')',
    ] {
        assert!(!text.contains(bad), "{bad:?} survived in {text:?}");
    }
    assert!(text.contains("ABCDEF"), "and the letters remain: {text:?}");
}

#[test]
fn a_caption_survives_a_file_that_declares_nonsense() {
    // The numbers come from a container that may be malformed. Nothing
    // here may divide by zero or panic.
    let mut p = properties("flac", Some(16));
    p.sample_rate = Some(0);
    p.channels = None;
    assert!(caption(&p).contains("Nyquist 0 Hz"));
    for weird in ["é", "日本語", "🎵", "\u{0}"] {
        let text = caption(&properties(weird, Some(16)));
        assert!(text.is_ascii(), "{text:?}");
        assert!(text.starts_with("44100 Hz"), "{text:?}");
    }
}

#[test]
fn missing_and_unversioned_pictures_are_scheduled_but_missing_sources_are_not_changes() {
    let dir = std::env::temp_dir().join("aede_spectrum_dates");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let audio = dir.join("one.flac");
    let picture = dir.join("one.png");
    std::fs::write(&audio, b"music").unwrap();
    assert!(out_of_date(&audio, &picture), "nothing drawn yet");

    std::fs::write(&picture, b"x").unwrap();
    assert!(
        out_of_date(&audio, &picture),
        "unversioned historical images regenerate once"
    );

    // The music moves on: the picture is now of bytes nobody has any more.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(120);
    std::fs::File::options()
        .write(true)
        .open(&audio)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert!(out_of_date(&audio, &picture), "the track changed since");

    // A track that cannot be read at all is not evidence of a change, and
    // must not make every run redraw it.
    assert!(!out_of_date(&dir.join("gone.flac"), &picture));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn size_is_read_from_what_a_reader_typed() {
    assert_eq!(Size::parse("full"), Some(Size::Full));
    assert_eq!(Size::parse(" Half "), Some(Size::Half));
    for unusable in ["", "1800x940", "large", "fullscreen"] {
        assert_eq!(Size::parse(unusable), None, "unusable: {unusable:?}");
    }
}

#[test]
fn half_shrinks_the_picture_by_a_quarter_not_by_half() {
    // Assert plotted area, not a compression/storage ratio for encoded PNGs.
    assert_eq!(Size::Full.dimensions(), (1800, 940));
    assert_eq!(Size::Half.dimensions(), (900, 470));
}

struct SpectralFiles(std::path::PathBuf);
impl SpectralFiles {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aede_native_spectral_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for SpectralFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn native_pcm_and_dsd_publish_valid_versioned_png_without_probing_a_decoder() {
    let sources = [
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures/tone.dsf"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures/tone.dff"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures/stereo128.dsf"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures/stereo128.dff"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/playback_fixtures/flac/playback-stereo.flac"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.wav"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.aiff"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.ogg"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.mp3"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/playback_fixtures/flac/playback32-mono.flac"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/playback_fixtures/flac/playback-ogg-flac.oga"),
    ];
    for source in sources {
        let files = SpectralFiles::new();
        let audio = files.0.join(source.file_name().unwrap());
        std::fs::copy(&source, &audio).unwrap();
        let original = std::fs::read(&audio).unwrap();
        let picture = picture_for(&audio);
        render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
            panic!("native formats must not resolve an external decoder")
        })
        .unwrap();
        assert_eq!(std::fs::read(&audio).unwrap(), original);
        assert!(
            !out_of_date(&audio, &picture),
            "fresh native PNG must carry its source/version stamp"
        );
        let file = std::fs::File::open(&picture).unwrap();
        let reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .unwrap();
        assert_eq!((reader.info().width, reader.info().height), (1148, 746));
        let record = &reader
            .info()
            .uncompressed_latin1_text
            .iter()
            .find(|t| t.keyword == "Spectrogram")
            .unwrap()
            .text;
        if audio.extension().unwrap() == "dsf" || audio.extension().unwrap() == "dff" {
            let expected_rate = if audio.file_stem().unwrap() == "stereo128" {
                705_600
            } else {
                352_800
            };
            assert!(
                record.contains(&format!("sample_rate={expected_rate}; frames=8192")),
                "{record}"
            );
        }
        let metadata = std::fs::metadata(&audio).unwrap();
        let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let handle = std::fs::OpenOptions::new()
            .write(true)
            .open(&audio)
            .unwrap();
        handle.set_modified(base).unwrap();
        assert!(out_of_date(&audio, &picture));
        handle.set_modified(metadata.modified().unwrap()).unwrap();
        assert!(!out_of_date(&audio, &picture));
        handle.set_len(metadata.len() + 1).unwrap();
        assert!(out_of_date(&audio, &picture));
    }
}
#[test]
fn cancellation_and_failures_never_replace_a_previous_image() {
    use std::cell::Cell;
    let files = SpectralFiles::new();
    let picture = files.0.join("result.png");
    std::fs::write(&picture, b"previous").unwrap();
    let cancelled = Cell::new(false);
    assert!(
        publish_with(&picture, &|| cancelled.get(), |output| {
            std::fs::write(output, b"unfinished").unwrap();
            cancelled.set(true);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(std::fs::read(&picture).unwrap(), b"previous");
    assert_eq!(std::fs::read_dir(&files.0).unwrap().count(), 1);
    assert!(
        render_cancellable(
            &files.0.join("absent.wav"),
            &picture,
            Size::Half,
            &|| true,
            &|| panic!("cancelled before work")
        )
        .unwrap_err()
        .contains("cancelled")
    );
}
#[test]
fn cancelling_native_dsd_never_falls_back_or_replaces_a_previous_image() {
    for name in ["tone.dsf", "stereo128.dff"] {
        let files = SpectralFiles::new();
        let audio = files.0.join(name);
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures")
            .join(name);
        std::fs::copy(source, &audio).unwrap();
        let picture = files.0.join("previous.png");
        std::fs::write(&picture, b"previous image").unwrap();
        let checks = std::cell::Cell::new(0usize);
        let result = render_cancellable(
            &audio,
            &picture,
            Size::Half,
            &|| {
                checks.set(checks.get() + 1);
                checks.get() >= 25
            },
            &|| panic!("native DSD cancellation must never resolve FFmpeg"),
        );
        assert!(result.unwrap_err().contains("cancelled"));
        assert_eq!(std::fs::read(picture).unwrap(), b"previous image");
        assert_eq!(std::fs::read_dir(&files.0).unwrap().count(), 2);
    }
}

#[test]
fn spectrogram_output_cannot_replace_its_source_audio() {
    let files = SpectralFiles::new();
    let audio = files.0.join("track.dsf");
    let bytes = include_bytes!("../../aede-audio/tests/fixtures/tone.dsf");
    std::fs::write(&audio, bytes).unwrap();
    assert!(
        render_cancellable(&audio, &audio, Size::Half, &|| false, &|| panic!(
            "same file refused before work"
        ))
        .is_err()
    );
    assert_eq!(std::fs::read(audio).unwrap(), bytes);
}
#[test]
fn external_measurement_uses_the_decoder_format_instead_of_forcing_catalogue_values() {
    let Some(program) = crate::ffmpeg::find() else {
        return;
    };
    let audio =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aac96.m4a.bin");
    let (mut stream, rate, channels) =
        crate::playback::decoder::FfmpegStream::measurement(&audio, &program).unwrap();
    assert_eq!(rate, 96_000);
    assert!(channels > 0);
    let mut samples = 0;
    while let Some(chunk) = stream.next_chunk().unwrap() {
        assert!(chunk.iter().all(|v| v.is_finite()));
        assert!(chunk.len().is_multiple_of(usize::from(channels)));
        samples += chunk.len();
    }
    assert!(samples > 0);
}

#[test]
fn legacy_codecs_keep_rust_images_and_explicit_lazy_decoder_resolution() {
    let Some(program) = crate::ffmpeg::find() else {
        return;
    };
    for relative in [
        "tests/fixtures/hybrid-lossy.wv",
        "tests/fixtures/aac.m4a",
        "tests/playback_fixtures/flac/playback32-mid-side.flac",
        "tests/fixtures/companded-alaw.wav.bin",
    ] {
        let files = SpectralFiles::new();
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
        let audio = files.0.join(source.file_name().unwrap());
        std::fs::copy(&source, &audio).unwrap();
        let picture = picture_for(&audio);
        let calls = std::cell::Cell::new(0);
        render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
            calls.set(calls.get() + 1);
            Ok(program.clone())
        })
        .unwrap();
        assert!(calls.get() <= 1);
        assert!(!out_of_date(&audio, &picture));
        let reader = png::Decoder::new(std::io::BufReader::new(
            std::fs::File::open(picture).unwrap(),
        ))
        .read_info()
        .unwrap();
        let stamp = &reader
            .info()
            .uncompressed_latin1_text
            .iter()
            .find(|t| t.keyword == "AedeSpectrum")
            .unwrap()
            .text;
        if calls.get() == 1 {
            assert!(stamp.contains("\"decoder\":\"ffmpeg\""));
        } else {
            assert!(
                relative.contains("companded"),
                "known unsupported codec must retain its fallback"
            );
        }
    }
}
