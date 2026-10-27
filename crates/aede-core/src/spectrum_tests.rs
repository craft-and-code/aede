use super::*;

#[test]
fn native_alac_spectrogram_never_resolves_ffmpeg() {
    let files = SpectralFiles::new();
    for name in ["mono16", "stereo24", "stereo24-clock48k"] {
        let audio = files.0.join(format!("{name}.m4a"));
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../aede-audio/tests/fixtures/native-alac")
                .join(format!("{name}.m4a")),
            &audio,
        )
        .unwrap();
        let picture = picture_for(&audio);
        render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
            panic!("qualified native ALAC must not resolve FFmpeg")
        })
        .unwrap();
        assert!(picture.is_file());
        assert!(!out_of_date(&audio, &picture));
        assert!(
            render_cancellable(&audio, &picture, Size::Half, &|| true, &|| {
                panic!("cancelled ALAC must not resolve FFmpeg")
            })
            .is_err()
        );
    }
}

#[test]
fn corrupt_admitted_alac_cannot_publish_a_recovered_shorter_spectrogram() {
    let files = SpectralFiles::new();
    let audio = files.0.join("corrupt.m4a");
    let mut bytes =
        include_bytes!("../../aede-audio/tests/fixtures/native-alac/stereo24.m4a").to_vec();
    let packet = bytes.windows(4).position(|b| b == b"mdat").unwrap() + 4;
    bytes[packet] = 0xe0;
    std::fs::write(&audio, bytes).unwrap();
    let picture = picture_for(&audio);
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    std::fs::write(&picture, b"previous complete image").unwrap();
    let external_called = std::cell::Cell::new(false);
    let result = render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
        external_called.set(true);
        Ok(crate::ffmpeg::find().unwrap_or_else(|| "absent-reference-ffmpeg".into()))
    });
    assert!(
        result.is_err(),
        "corrupt ALAC cannot publish a completed image"
    );
    assert!(
        !external_called.get(),
        "native data failure is not unsupported-codec admission"
    );
    assert_eq!(std::fs::read(picture).unwrap(), b"previous complete image");
}

#[test]
fn unadmitted_alac_configuration_retains_explicit_spectral_compatibility() {
    let files = SpectralFiles::new();
    let audio = files.0.join("multichannel.m4a");
    std::fs::write(
        &audio,
        include_bytes!("../../aede-audio/tests/fixtures/native-alac/surround16.m4a"),
    )
    .unwrap();
    let called = std::cell::Cell::new(false);
    let result = render_cancellable(&audio, &picture_for(&audio), Size::Half, &|| false, &|| {
        called.set(true);
        Err("optional compatibility decoder is unavailable".into())
    });
    assert!(result.is_err());
    assert!(
        called.get(),
        "unadmitted configurations keep the optional external route"
    );
}

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
fn spectral_compatibility_is_lazy_and_admitted_aac_stays_native() {
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
        if matches!(
            relative,
            "tests/fixtures/aac.m4a" | "tests/fixtures/hybrid-lossy.wv"
        ) {
            assert_eq!(
                calls.get(),
                0,
                "admitted native codecs no longer need compatibility"
            );
            assert!(stamp.contains("\"decoder\":\"native\""));
        } else if calls.get() == 1 {
            assert!(stamp.contains("\"decoder\":\"ffmpeg\""));
        } else {
            assert!(
                relative.contains("companded"),
                "known unsupported codec must retain its fallback"
            );
        }
    }
}

#[test]
fn admitted_aac_spectrograms_never_resolve_external_tools() {
    let files = SpectralFiles::new();
    for name in ["mono441", "stereo48", "stereo96", "coarse48"] {
        let audio = files.0.join(format!("{name}.m4a"));
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../aede-audio/tests/fixtures/native-aac")
                .join(format!("{name}.m4a")),
            &audio,
        )
        .unwrap();
        let picture = picture_for(&audio);
        render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
            panic!("qualified AAC must not use FFmpeg/FFprobe")
        })
        .unwrap();
        assert!(!out_of_date(&audio, &picture));
        let previous = std::fs::read(&picture).unwrap();
        let checks = std::cell::Cell::new(0);
        let result = render_cancellable(
            &audio,
            &picture,
            Size::Half,
            &|| {
                checks.set(checks.get() + 1);
                checks.get() > 12
            },
            &|| panic!("AAC cancellation must not use FFmpeg"),
        );
        assert!(result.unwrap_err().contains("cancelled"));
        assert_eq!(std::fs::read(picture).unwrap(), previous);
    }
}

#[test]
fn unqualified_aac_edit_retains_explicit_compatibility() {
    let files = SpectralFiles::new();
    let audio = files.0.join("no-edit.m4a");
    let mut bytes =
        include_bytes!("../../aede-audio/tests/fixtures/native-aac/stereo48.m4a").to_vec();
    let edit = bytes.windows(4).position(|b| b == b"edts").unwrap();
    bytes[edit..edit + 4].copy_from_slice(b"free");
    std::fs::write(&audio, bytes).unwrap();
    let called = std::cell::Cell::new(false);
    let result = render_cancellable(&audio, &picture_for(&audio), Size::Half, &|| false, &|| {
        called.set(true);
        Err("optional compatibility decoder unavailable".into())
    });
    assert!(result.is_err());
    assert!(called.get());
}

#[test]
fn incomplete_admitted_aac_cannot_publish_external_recovery() {
    let files = SpectralFiles::new();
    let audio = files.0.join("corrupt-aac.m4a");
    let mut bytes =
        include_bytes!("../../aede-audio/tests/fixtures/native-aac/stereo48.m4a").to_vec();
    let table = bytes.windows(4).position(|b| b == b"stsz").unwrap() + 4;
    let count = u32::from_be_bytes(bytes[table + 8..table + 12].try_into().unwrap()) as usize;
    let last = table + 12 + (count - 1) * 4;
    let missing = u32::from_be_bytes(bytes[last..last + 4].try_into().unwrap()) as usize;
    bytes.truncate(bytes.len() - missing);
    let mdat = bytes.windows(4).position(|b| b == b"mdat").unwrap() - 4;
    let size = (bytes.len() - mdat) as u32;
    bytes[mdat..mdat + 4].copy_from_slice(&size.to_be_bytes());
    std::fs::write(&audio, bytes).unwrap();
    let picture = picture_for(&audio);
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    std::fs::write(&picture, b"previous complete image").unwrap();
    let result = render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
        panic!("native AAC data failures must not start FFmpeg")
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(picture).unwrap(), b"previous complete image");
}

#[test]
fn sample_accurate_aac_edits_do_not_publish_an_unedited_native_spectrum() {
    let files = SpectralFiles::new();
    let audio = files.0.join("edited-aac.m4a");
    let mut bytes =
        include_bytes!("../../aede-audio/tests/fixtures/native-aac/stereo48.m4a").to_vec();
    let edit = bytes.windows(4).position(|b| b == b"elst").unwrap() + 4;
    bytes[edit + 8..edit + 12].copy_from_slice(&48012u32.to_be_bytes());
    std::fs::write(&audio, &bytes).unwrap();
    let picture = picture_for(&audio);
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    std::fs::write(&picture, b"previous image").unwrap();
    let calls = std::cell::Cell::new(0);
    assert!(
        render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
            calls.set(calls.get() + 1);
            Err("optional compatibility decoder unavailable".into())
        })
        .is_err()
    );
    assert_eq!(
        calls.get(),
        1,
        "an exact crop is outside full-track native admission"
    );
    assert_eq!(std::fs::read(&picture).unwrap(), b"previous image");
    assert_eq!(std::fs::read(audio).unwrap(), bytes);
}

#[test]
fn images_from_the_previous_aac_edit_admission_are_stale() {
    let files = SpectralFiles::new();
    let audio = files.0.join("aac.m4a");
    std::fs::write(
        &audio,
        include_bytes!("../../aede-audio/tests/fixtures/native-aac/stereo48.m4a"),
    )
    .unwrap();
    let picture = picture_for(&audio);
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    let record = serde_json::json!({
        "generation":"rust-spectrum-v1-dsd-wideband-v1-opus-v1-alac-config-v4-aac-edit-v1",
        "input":cache::Input::read(&audio).unwrap(),
        "decoder":"native"
    })
    .to_string();
    let mut encoder = png::Encoder::new(std::fs::File::create(&picture).unwrap(), 1, 1);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .add_text_chunk("AedeSpectrum".into(), record)
        .unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[0]).unwrap();
    writer.finish().unwrap();
    assert!(
        out_of_date(&audio, &picture),
        "old AAC timing results cannot remain fresh after admission changes"
    );
}

#[test]
fn additional_native_codecs_render_without_resolving_ffmpeg() {
    for name in [
        "track.wv",
        "float32.wv",
        "hybrid-lossy.wv",
        "ramp.ape",
        "tone-sv8.mpc",
        "speex-nb.spx",
        "speex-wb.spx",
        "speex-uwb.spx",
    ] {
        let files = SpectralFiles::new();
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures/native-extra")
            .join(name);
        let audio = files.0.join(name);
        std::fs::copy(source, &audio).unwrap();
        let picture = picture_for(&audio);
        let calls = std::cell::Cell::new(0);
        render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
            calls.set(calls.get() + 1);
            Err("external tools deliberately unavailable".into())
        })
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(calls.get(), 0, "{name}");
        assert!(!out_of_date(&audio, &picture));
        let bytes = std::fs::read(&picture).unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(String::from_utf8_lossy(&bytes).contains("native-extra-v3"));
    }
}

#[test]
fn cancelled_additional_codec_render_preserves_the_previous_picture() {
    let files = SpectralFiles::new();
    let audio = files.0.join("source.ape");
    std::fs::copy(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures/native-extra/ramp.ape"),
        &audio,
    )
    .unwrap();
    let picture = picture_for(&audio);
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    std::fs::write(&picture, b"previous result").unwrap();
    let polls = std::cell::Cell::new(0);
    let result = render_cancellable(
        &audio,
        &picture,
        Size::Half,
        &|| {
            polls.set(polls.get() + 1);
            polls.get() >= 7
        },
        &|| panic!("cancellation must not select another decoder"),
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read(picture).unwrap(), b"previous result");
}

#[test]
fn admitted_speex_data_failure_never_recovers_through_ffmpeg_or_replaces_an_image() {
    let files = SpectralFiles::new();
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-audio/tests/fixtures/native-extra/speex-wb.spx");
    let mut bytes = std::fs::read(source).unwrap();
    // First audio page, after both valid identification/comment pages.
    bytes[200] ^= 0x55;
    let audio = files.0.join("corrupt.spx");
    std::fs::write(&audio, bytes).unwrap();
    assert!(aede_audio::decode::PcmStreamDecoder::open(&audio).is_ok());
    let picture = picture_for(&audio);
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    std::fs::write(&picture, b"retained picture").unwrap();
    let calls = std::cell::Cell::new(0);
    let result = render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
        calls.set(calls.get() + 1);
        Err("no external decoder".into())
    });
    assert!(result.is_err());
    assert_eq!(calls.get(), 0);
    assert_eq!(std::fs::read(picture).unwrap(), b"retained picture");
}

#[test]
fn contradictory_wavpack_clocks_cannot_publish_a_native_image() {
    let files = SpectralFiles::new();
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-audio/tests/fixtures/native-extra/track.wv");
    let mut bytes = std::fs::read(source).unwrap();
    let second = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8;
    let flags = u32::from_le_bytes(bytes[second + 24..second + 28].try_into().unwrap());
    bytes[second + 24..second + 28]
        .copy_from_slice(&((flags & !(15 << 23)) | (10 << 23)).to_le_bytes());
    let audio = files.0.join("changed.wv");
    std::fs::write(&audio, bytes).unwrap();
    let picture = picture_for(&audio);
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    std::fs::write(&picture, b"retained picture").unwrap();
    let calls = std::cell::Cell::new(0);
    let result = render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
        calls.set(calls.get() + 1);
        Err("external compatibility unavailable".into())
    });
    assert!(result.is_err());
    assert_eq!(calls.get(), 1);
    assert_eq!(std::fs::read(picture).unwrap(), b"retained picture");
}

#[test]
fn invalid_seek_entries_and_pcm_precision_cannot_publish_native_images() {
    for name in ["tone-sv8.mpc", "track.wv"] {
        let files = SpectralFiles::new();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures/native-extra")
            .join(name);
        let mut bytes = std::fs::read(fixture).unwrap();
        if name.ends_with(".mpc") {
            bytes[17] &= 0xf8;
            let mut crc = !0u32;
            for &byte in &bytes[11..18] {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
                }
            }
            bytes[7..11].copy_from_slice(&(!crc).to_be_bytes());
            bytes.splice(
                11564..11570,
                b"ST\x09\x28\x00\x00\x00\x00\x00".iter().copied(),
            );
        } else {
            let second = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8;
            let flags = u32::from_le_bytes(bytes[second + 24..second + 28].try_into().unwrap());
            bytes[second + 24..second + 28].copy_from_slice(&(flags | (1 << 13)).to_le_bytes());
        }
        let audio = files.0.join(name);
        std::fs::write(&audio, bytes).unwrap();
        let picture = picture_for(&audio);
        std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
        std::fs::write(&picture, b"retained picture").unwrap();
        let calls = std::cell::Cell::new(0);
        assert!(
            render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
                calls.set(calls.get() + 1);
                Err("external compatibility unavailable".into())
            })
            .is_err()
        );
        assert_eq!(calls.get(), 1, "{name}");
        assert_eq!(std::fs::read(picture).unwrap(), b"retained picture");
    }
}

#[test]
fn images_from_before_native_header_validation_are_stale() {
    let files = SpectralFiles::new();
    let audio = files.0.join("track.wv");
    std::fs::write(&audio, b"unchanged source identity").unwrap();
    let picture = picture_for(&audio);
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    for generation in [
        "rust-spectrum-v1-dsd-wideband-v1-opus-v1-alac-config-v4-aac-edit-v2-native-extra-v1",
        "rust-spectrum-v1-dsd-wideband-v1-opus-v1-alac-config-v4-aac-edit-v2-native-extra-v2",
    ] {
        let record = serde_json::json!({
            "generation": generation,
            "input": cache::Input::read(&audio).unwrap(),
            "decoder": "native"
        })
        .to_string();
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder
                .add_text_chunk("AedeSpectrum".into(), record)
                .unwrap();
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[0, 0, 0, 0]).unwrap();
        }
        std::fs::write(&picture, bytes).unwrap();
        assert!(out_of_date(&audio, &picture));
    }
}

#[test]
fn custom_wavpack_clocks_keep_consistent_native_spectrum_admission() {
    for (rates, accepted) in [
        ([Some(43210u32), Some(47654u32)], false),
        ([Some(43210), Some(43210)], true),
        ([None, Some(44100)], true),
    ] {
        let files = SpectralFiles::new();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures/native-extra/track.wv");
        let original = std::fs::read(fixture).unwrap();
        let mut at = 0;
        let mut bytes = Vec::new();
        for rate in rates {
            let size =
                u32::from_le_bytes(original[at + 4..at + 8].try_into().unwrap()) as usize + 8;
            let mut block = original[at..at + size].to_vec();
            at += size;
            if let Some(rate) = rate {
                let flags = u32::from_le_bytes(block[24..28].try_into().unwrap());
                block[24..28].copy_from_slice(&((flags & !(15 << 23)) | (15 << 23)).to_le_bytes());
                let mut metadata = vec![0x67, 2];
                metadata.extend_from_slice(&rate.to_le_bytes()[..3]);
                metadata.push(0);
                block.splice(32..32, metadata);
                block[4..8].copy_from_slice(&((size - 8 + 6) as u32).to_le_bytes());
            }
            bytes.extend(block);
        }
        bytes.extend_from_slice(&original[at..]);
        let audio = files.0.join("clock.wv");
        std::fs::write(&audio, bytes).unwrap();
        let picture = picture_for(&audio);
        std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
        std::fs::write(&picture, b"retained picture").unwrap();
        let calls = std::cell::Cell::new(0);
        let result = render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
            calls.set(calls.get() + 1);
            Err("external compatibility unavailable".into())
        });
        assert_eq!(result.is_ok(), accepted, "{rates:?}: {result:?}");
        assert_eq!(calls.get(), usize::from(!accepted));
        if accepted {
            assert!(!out_of_date(&audio, &picture));
        } else {
            assert_eq!(std::fs::read(picture).unwrap(), b"retained picture");
        }
    }
}

#[test]
fn zero_frame_sv7_headers_cannot_replace_an_existing_spectrogram() {
    let files = SpectralFiles::new();
    let mut bytes = vec![0; 64];
    bytes[..4].copy_from_slice(b"MP+\x07");
    bytes[8..12].copy_from_slice(&0x01000000u32.to_le_bytes());
    let audio = files.0.join("zero-frames.mpc");
    std::fs::write(&audio, &bytes).unwrap();
    let picture = picture_for(&audio);
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    std::fs::write(&picture, b"retained picture").unwrap();
    assert!(
        render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
            Err("external compatibility unavailable".into())
        })
        .is_err()
    );
    assert_eq!(std::fs::read(picture).unwrap(), b"retained picture");
    assert_eq!(std::fs::read(audio).unwrap(), bytes);
}

#[test]
fn historical_ape_spectrograms_never_resolve_an_external_decoder() {
    let files = SpectralFiles::new();
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-audio/tests/fixtures/native-extra/silence-3970.ape");
    let audio = files.0.join("legacy.ape");
    let original = std::fs::read(source).unwrap();
    std::fs::write(&audio, &original).unwrap();
    let picture = picture_for(&audio);
    render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
        panic!("historical APE must not resolve FFmpeg")
    })
    .unwrap();
    assert!(!out_of_date(&audio, &picture));
    let previous = std::fs::read(&picture).unwrap();
    let mut corrupt = original;
    corrupt[44] ^= 1;
    std::fs::write(&audio, &corrupt).unwrap();
    assert!(
        render_cancellable(&audio, &picture, Size::Half, &|| false, &|| {
            panic!("admitted CRC failure cannot select an external decoder")
        })
        .is_err()
    );
    assert_eq!(std::fs::read(picture).unwrap(), previous);
    assert_eq!(std::fs::read(audio).unwrap(), corrupt);
}
