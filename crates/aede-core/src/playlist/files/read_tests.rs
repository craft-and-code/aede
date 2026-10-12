use super::*;
use crate::playlist::files::test_support::Fixture;

#[test]
fn playlists_from_the_shared_cli_renderer_are_read_as_both_supported_dialects() {
    use crate::playlist::{Style, try_render};
    let fixture = Fixture::new();
    let catalog = crate::model::build(
        vec![crate::model::ScannedFile {
            path: fixture.0.join("01.flac").to_str().unwrap().into(),
            size: 1,
            mtime: 0,
            tags: crate::tags::RawTags::default(),
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec![],
        0,
        &[],
    );
    for (name, style) in [
        ("extended.m3u", Style::Extended),
        ("simple.m3u", Style::Simple),
    ] {
        let content = try_render(&catalog, &[0, 0], Some(&fixture.0), style).unwrap();
        let path = fixture.0.join(name);
        fs::write(&path, &content).unwrap();
        let document = read(&fixture.roots(), &[], &metadata(&path).unwrap().id).unwrap();
        assert_eq!(document.content, content);
        assert_eq!(document.track_count, 2);
        assert_eq!(document.content.matches("01.flac").count(), 2);
    }
}

#[test]
fn simple_and_extended_text_preserve_bom_crlf_duplicates_and_foreign_entries_without_following_them()
 {
    let fixture = Fixture::new();
    for (filename, content, count) in [
        ("simple.m3u", "01.flac\n./#second.flac\n", 2),
        (
            "extended.m3u8",
            "\u{feff}#EXTM3U\r\n#EXTINF:1,First\r\n01.flac\r\n01.flac\r\n../absent.flac\r\nhttps://invalid.example/radio\r\n",
            4,
        ),
    ] {
        let path = fixture.0.join(filename);
        fs::write(&path, content).unwrap();
        let file = metadata(&path).unwrap();
        let answer = read(&fixture.roots(), &[], &file.id).unwrap();
        assert_eq!(answer.content, content);
        assert_eq!(answer.track_count, count);
        assert_eq!(fs::read(path).unwrap(), content.as_bytes());
    }
}

#[test]
fn invalid_oversized_and_excessive_entry_files_are_listed_but_detail_is_refused() {
    let fixture = Fixture::new();
    for (name, content, reason) in [
        ("utf8.m3u", vec![0xff], Error::InvalidText),
        ("binary.m3u", b"01.flac\0\n".to_vec(), Error::InvalidText),
        (
            "many.m3u",
            b"01.flac\n".repeat(MAX_TRACKS + 1),
            Error::TooLarge,
        ),
    ] {
        let path = fixture.0.join(name);
        fs::write(&path, &content).unwrap();
        assert!(
            matches!(read(&fixture.roots(), &[], &metadata(&path).unwrap().id), Err(error) if error == reason)
        );
        assert_eq!(fs::read(path).unwrap(), content);
    }
    let oversized = fixture.0.join("large.m3u");
    fs::File::create(&oversized)
        .unwrap()
        .set_len(MAX_BYTES + 1)
        .unwrap();
    assert_eq!(discover(&fixture.roots(), &[]).unwrap().files.len(), 4);
    assert!(matches!(
        read(&fixture.roots(), &[], &metadata(&oversized).unwrap().id),
        Err(Error::TooLarge)
    ));
}

#[test]
fn replacement_between_inspection_and_open_is_refused_without_reading_the_replacement() {
    let fixture = Fixture::new();
    let path = fixture.0.join("changed.m3u");
    fs::write(&path, b"original.flac\n").unwrap();
    let file = metadata(&path).unwrap();
    let answer = read_file(file, |path| {
        fs::rename(path, path.with_extension("held")).unwrap();
        fs::write(path, b"replacement.flac\n").unwrap();
        fs::File::open(path)
    });
    assert!(matches!(answer, Err(Error::Changed)));
    assert_eq!(fs::read(&path).unwrap(), b"replacement.flac\n");
    assert_eq!(
        fs::read(path.with_extension("held")).unwrap(),
        b"original.flac\n"
    );
}

#[cfg(unix)]
#[test]
fn a_root_alias_changed_during_read_cannot_publish_content_from_the_previous_root() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let other = Fixture::new();
    let alias = other.0.join("root");
    symlink(&fixture.0, &alias).unwrap();
    let path = fixture.0.join("selection.m3u");
    fs::write(&path, b"01.flac\n").unwrap();
    let roots = vec![alias.to_str().unwrap().into()];
    let answer = read_with_open(&roots, &[], &metadata(&path).unwrap().id, |path| {
        fs::remove_file(&alias).unwrap();
        symlink(&other.0, &alias).unwrap();
        fs::File::open(path)
    });
    assert!(matches!(answer, Err(Error::Changed)));
    assert_eq!(fs::read(path).unwrap(), b"01.flac\n");
}
