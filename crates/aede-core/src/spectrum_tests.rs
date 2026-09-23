use super::*;

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
fn a_missing_picture_is_drawn_and_a_fresh_one_is_left_alone() {
    let dir = std::env::temp_dir().join("aede_spectrum_dates");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let audio = dir.join("one.flac");
    let picture = dir.join("one.png");
    std::fs::write(&audio, b"music").unwrap();
    assert!(out_of_date(&audio, &picture), "nothing drawn yet");

    std::fs::write(&picture, b"x").unwrap();
    assert!(
        !out_of_date(&audio, &picture),
        "drawn after the music was written"
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
    // The whole point of offering it: a spectrogram is mostly noise,
    // which a PNG encoder cannot compress away, so a picture's size on
    // disk tracks its pixel count closely. Halving both dimensions is
    // what actually shrinks the file on disk by about four times.
    assert!(filter(Size::Full).contains("s=1800x940"));
    assert!(filter(Size::Half).contains("s=900x470"));
}
