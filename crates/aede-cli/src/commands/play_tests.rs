use std::path::PathBuf;

use aede_core::playback::decoder::FileDecoder;
use aede_core::playback::stream::PcmTrack;
use aede_dsp::{Dsp, ToneControls};

use super::{
    PlaybackClock, PlaybackDiagnostics, PlaybackEnd, next_index, play, record_play, resolve,
    stream_pcm_counted,
};
use crate::args::Args;
use aede_core::conclusions;
use aede_core::model::{Artist, AudioFile, Catalog, Release, Track};
use aede_core::user;

#[test]
fn playback_tone_options_are_bounded_and_flat_by_default() {
    let flat = Args::parse(["play".into(), "song.flac".into()]);
    assert!(super::tone_controls(&flat).expect("default tone").is_flat());
    let shaped = Args::parse([
        "play".into(),
        "song.flac".into(),
        "--bass".into(),
        "-4".into(),
        "--treble".into(),
        "6".into(),
    ]);
    let tone = super::tone_controls(&shaped).expect("valid tone");
    assert_eq!(tone.bass_db(), -4.0);
    assert_eq!(tone.treble_db(), 6.0);
    assert_eq!(tone.safe_preamp_db(), -6.0);

    let missing = Args::parse(["play".into(), "song.flac".into(), "--bass".into()]);
    assert!(super::tone_controls(&missing).is_err());
    let out_of_range = Args::parse(["play".into(), "song.flac".into(), "--treble=13".into()]);
    assert!(super::tone_controls(&out_of_range).is_err());
}

#[test]
fn playing_label_shows_album_and_numbered_filename_without_the_path() {
    let path = PathBuf::from("/music/Ozzy/1991 No More Tears [FLAC]/01 Mr. Tinkertrain.flac");
    assert_eq!(
        super::playing_label(&path, None),
        "1991 No More Tears [FLAC] — 01 Mr. Tinkertrain"
    );
    let catalog = Catalog {
        releases: vec![Release {
            id: 0,
            title: "No More Tears".into(),
            ..Default::default()
        }],
        tracks: vec![Track {
            id: 0,
            file_id: 0,
            release_id: Some(0),
            ..Default::default()
        }],
        files: vec![AudioFile {
            id: 0,
            path: path.to_string_lossy().into_owned(),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(
        super::playing_label(&path, Some(&catalog)),
        "No More Tears — 01 Mr. Tinkertrain"
    );
}

#[test]
fn untagged_cli_track_uses_and_caches_measured_loudness() {
    let root = std::env::temp_dir().join(format!("aede_play_loudness_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("track.wav");
    let rate = 48_000u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + rate * 2).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    for frame in 0..rate {
        let sample = (0.2
            * i16::MAX as f32
            * (std::f32::consts::TAU * 1000.0 * frame as f32 / rate as f32).sin())
            as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(&file, &wav).unwrap();
    let selection = super::PlaybackSelection::mixed(vec![file.clone()]);
    let first = aede_core::playback::gain_plan::plan_normalization(
        &selection.paths,
        selection.is_album,
        None,
        &root,
        aede_core::playback::normalization::Mode::Track,
    )
    .unwrap();
    assert_eq!(first[0].unwrap().label, "measured track");
    let cache = conclusions::load(&conclusions::conclusions_path(&root))
        .unwrap()
        .unwrap();
    assert!(
        cache
            .loudness_tracks
            .contains_key(&file.to_string_lossy().into_owned())
    );
    let second = aede_core::playback::gain_plan::plan_normalization(
        &selection.paths,
        selection.is_album,
        None,
        &root,
        aede_core::playback::normalization::Mode::Track,
    )
    .unwrap();
    assert_eq!(first[0].unwrap().gain_db, second[0].unwrap().gain_db);
    wav[4..8].copy_from_slice(&(38 + rate * 2).to_le_bytes());
    wav[40..44].copy_from_slice(&(rate * 2 + 2).to_le_bytes());
    wav.extend_from_slice(&0i16.to_le_bytes());
    std::fs::write(&file, &wav).unwrap();
    let third = aede_core::playback::gain_plan::plan_normalization(
        &selection.paths,
        selection.is_album,
        None,
        &root,
        aede_core::playback::normalization::Mode::Track,
    )
    .unwrap();
    assert!(third[0].is_some());
    let refreshed = conclusions::load(&conclusions::conclusions_path(&root))
        .unwrap()
        .unwrap();
    assert_eq!(
        refreshed.loudness_tracks[&file.to_string_lossy().into_owned()].size,
        wav.len() as u64
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn m3u_plays_relative_and_absolute_entries_in_written_order() {
    let root = std::env::temp_dir().join(format!("aede_play_m3u_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Album")).unwrap();
    let first = root.join("Album/01.flac");
    let second = root.join("Album/02.flac");
    std::fs::write(&first, b"fixture").unwrap();
    std::fs::write(&second, b"fixture").unwrap();
    let playlist = root.join("Album/list.m3u8");
    std::fs::write(
        &playlist,
        format!(
            "\u{feff}#EXTM3U\r\n#EXTINF:1,second\r\n02.flac\r\n{}\r\n02.flac\r\n",
            first.display()
        ),
    )
    .unwrap();
    assert_eq!(
        super::resolve(&playlist.to_string_lossy(), None, None)
            .unwrap()
            .paths,
        [
            second.canonicalize().unwrap(),
            first.canonicalize().unwrap(),
            second.canonicalize().unwrap()
        ]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn m3u_refuses_missing_entries_and_remote_urls() {
    let root = std::env::temp_dir().join(format!("aede_play_bad_m3u_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let playlist = root.join("list.m3u");
    std::fs::write(&playlist, "missing.flac\n").unwrap();
    assert!(super::resolve(&playlist.to_string_lossy(), None, None).is_err());
    std::fs::write(&playlist, "https://example.com/audio.flac\n").unwrap();
    assert!(super::resolve(&playlist.to_string_lossy(), None, None).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_saved_collection_plays_its_current_query_result_in_catalog_order() {
    let root = std::env::temp_dir().join(format!("aede_play_collection_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let catalog = Catalog {
        tracks: ["Blue one", "Red", "Blue two"]
            .iter()
            .enumerate()
            .map(|(id, title)| Track {
                id: id as u32,
                file_id: id as u32,
                title: (*title).into(),
                ..Default::default()
            })
            .collect(),
        files: ["01.flac", "02.flac", "03.flac"]
            .iter()
            .enumerate()
            .map(|(id, path)| AudioFile {
                id: id as u32,
                path: (*path).into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let mut data = user::UserData::default();
    data.save_collection(user::LOCAL_USER, "Blue songs", "title:Blue", 1);
    user::save(&data, &user::user_path(&root)).unwrap();
    let args = Args::parse([
        "play".to_string(),
        "--data".to_string(),
        root.to_string_lossy().into_owned(),
    ]);
    let expected = [PathBuf::from("01.flac"), PathBuf::from("03.flac")];
    let collection = resolve("collection:Blue songs", Some(&catalog), Some(&args)).unwrap();
    assert!(!collection.is_album);
    assert_eq!(
        collection.normalization_mode(None),
        aede_core::playback::normalization::Mode::Track
    );
    assert_eq!(collection.paths, expected);
    assert_eq!(
        resolve("blue songs", Some(&catalog), Some(&args))
            .unwrap()
            .paths,
        expected
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn album_selection_uses_album_gain_by_default_and_explicit_mode_takes_priority() {
    let catalog = Catalog {
        releases: vec![Release {
            id: 0,
            title: "Quiet album".into(),
            key: "quiet album".into(),
            track_ids: vec![0],
            ..Default::default()
        }],
        tracks: vec![Track {
            id: 0,
            file_id: 0,
            title: "Quiet track".into(),
            release_id: Some(0),
            ..Default::default()
        }],
        files: vec![AudioFile {
            id: 0,
            path: "quiet.flac".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let album = resolve("Quiet album", Some(&catalog), None).unwrap();
    assert_eq!(
        album.normalization_mode(None),
        aede_core::playback::normalization::Mode::Album
    );
    let track = resolve("Quiet track", Some(&catalog), None).unwrap();
    assert_eq!(
        track.normalization_mode(None),
        aede_core::playback::normalization::Mode::Track
    );
    let explicit_off = Args::parse(["play".into(), "--normalize".into(), "off".into()]);
    assert_eq!(
        album.normalization_mode(super::normalization_mode(&explicit_off).unwrap()),
        aede_core::playback::normalization::Mode::Off
    );
}

#[test]
fn an_explicit_collection_name_disambiguates_a_track_title() {
    let root =
        std::env::temp_dir().join(format!("aede_play_collection_name_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let catalog = Catalog {
        tracks: vec![
            Track {
                id: 0,
                file_id: 0,
                title: "Favorites".into(),
                ..Default::default()
            },
            Track {
                id: 1,
                file_id: 1,
                title: "Other".into(),
                ..Default::default()
            },
        ],
        files: vec![
            AudioFile {
                id: 0,
                path: "track.flac".into(),
                ..Default::default()
            },
            AudioFile {
                id: 1,
                path: "other.flac".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let mut data = user::UserData::default();
    data.save_collection(user::LOCAL_USER, "Favorites", "title:Other", 1);
    user::save(&data, &user::user_path(&root)).unwrap();
    let args = Args::parse([
        "play".to_string(),
        "--data".to_string(),
        root.to_string_lossy().into_owned(),
    ]);
    assert_eq!(
        resolve("Favorites", Some(&catalog), Some(&args))
            .unwrap()
            .paths,
        [PathBuf::from("track.flac")]
    );
    assert_eq!(
        resolve("collection:Favorites", Some(&catalog), Some(&args))
            .unwrap()
            .paths,
        [PathBuf::from("other.flac")]
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/fixtures")
        .join(name)
}

#[test]
fn pcm_stream_reaches_output_as_interleaved_little_endian_floats() {
    let path = fixture("audit-stereo.flac");
    let mut track = PcmTrack::open(&path).expect("fixture opens");
    let format = track.format();
    let mut dsp = Dsp::new(format);
    let mut output = Vec::new();
    let mut frames_written = 0;
    let mut clamped_samples = 0;
    let mut meter = aede_dsp::OutputMeter::new(format).expect("output meter");
    stream_pcm_counted(
        &mut track,
        &mut dsp,
        &mut output,
        &mut frames_written,
        &mut clamped_samples,
        PlaybackDiagnostics {
            meter: Some(&mut meter),
            visualizer: None,
            normalization: None,
        },
        None,
        &mut PlaybackClock::new(),
    )
    .expect("stream succeeds");
    assert!(frames_written > 0);
    let measured = meter.snapshot().expect("meter snapshot");
    assert_eq!(measured.frames, frames_written);
    assert_eq!(measured.guarded_samples, clamped_samples);
    assert!(measured.output_true_peak.is_some());

    assert!(!output.is_empty());
    assert_eq!(output.len() % (usize::from(format.channels()) * 4), 0);
    let decoded = output
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect::<Vec<_>>();
    let mut reference = FileDecoder::open(&path).expect("fixture reopens");
    let mut block = vec![0.0; decoded.len()];
    let frames = reference.read_frames(&mut block).expect("first packet");
    assert_eq!(
        decoded[..frames * usize::from(format.channels())],
        block[..frames * usize::from(format.channels())]
    );
    assert!(decoded.iter().all(|sample| sample.is_finite()));
}

#[test]
fn tone_processing_reaches_the_serialized_playback_stream() {
    let path = fixture("audit-stereo.flac");
    let mut track = PcmTrack::open(&path).expect("fixture opens");
    let tone = ToneControls::new(6.0, -3.0).expect("tone");
    let mut dsp = Dsp::new(track.format());
    dsp.set_tone(tone).expect("filters");
    dsp.set_gain_db(tone.safe_preamp_db(), 0).expect("preamp");
    let mut output = Vec::new();
    let mut frames_written = 0;
    let mut clamped_samples = 0;
    stream_pcm_counted(
        &mut track,
        &mut dsp,
        &mut output,
        &mut frames_written,
        &mut clamped_samples,
        PlaybackDiagnostics {
            meter: None,
            visualizer: None,
            normalization: None,
        },
        None,
        &mut PlaybackClock::new(),
    )
    .expect("stream succeeds");
    let decoded = output
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect::<Vec<_>>();
    let mut reference = FileDecoder::open(&path).expect("fixture reopens");
    let mut unchanged = vec![0.0; decoded.len()];
    let frames = reference.read_frames(&mut unchanged).expect("first packet");
    assert!(frames_written > 0);
    assert!(
        decoded[..frames * 2]
            .iter()
            .zip(&unchanged[..frames * 2])
            .any(|(a, b)| (a - b).abs() > 0.0001)
    );
    assert_eq!(clamped_samples, 0);
}

#[test]
fn surround_source_reaches_cli_output_as_stereo_frames() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/channel_fixtures/surround-5_1.wav");
    let mut track = PcmTrack::open_stereo(&path).expect("known layout");
    let mut dsp = Dsp::new(track.format());
    let mut output = Vec::new();
    let mut frames_written = 0;
    let mut clamped_samples = 0;
    stream_pcm_counted(
        &mut track,
        &mut dsp,
        &mut output,
        &mut frames_written,
        &mut clamped_samples,
        PlaybackDiagnostics {
            meter: None,
            visualizer: None,
            normalization: None,
        },
        None,
        &mut PlaybackClock::new(),
    )
    .expect("stream succeeds");
    assert_eq!(frames_written, 120);
    assert_eq!(output.len(), 120 * 2 * 4);
    assert_eq!(clamped_samples, 0);
    let samples = output
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect::<Vec<_>>();
    assert_eq!(&samples[6..8], &[0.0, 0.0]); // LFE omitted
}

#[test]
fn transport_moves_within_the_selection_and_previous_restarts_after_three_seconds() {
    assert_eq!(next_index(0, 3, 0, PlaybackEnd::Next), Some(1));
    assert_eq!(next_index(1, 3, 1_000, PlaybackEnd::Previous), Some(0));
    assert_eq!(next_index(1, 3, 3_001, PlaybackEnd::Previous), Some(1));
    assert_eq!(next_index(0, 3, 100, PlaybackEnd::Previous), Some(0));
    assert_eq!(next_index(2, 3, 0, PlaybackEnd::Next), None);
    assert_eq!(next_index(1, 3, 0, PlaybackEnd::Stop), None);
}

#[test]
fn play_accepts_a_catalog_location_for_history() {
    let args = Args::parse(["play", "track.flac", "--data", "somewhere"].map(str::to_string));
    let error = play(&args).expect_err("unknown file or title");
    assert!(!error.to_string().contains("--data"));
}

#[test]
fn a_folder_plays_audio_in_sorted_recursive_order() {
    let root = std::env::temp_dir().join(format!("aede_play_folder_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("B album")).unwrap();
    std::fs::create_dir_all(root.join("A album")).unwrap();
    let root = root.canonicalize().unwrap();
    for name in [
        "B album/02.flac",
        "A album/02.flac",
        "A album/01.flac",
        "B album/01.flac",
    ] {
        std::fs::write(root.join(name), b"fixture").unwrap();
    }
    std::fs::write(root.join("A album/cover.jpg"), b"image").unwrap();
    let paths = resolve(&root.to_string_lossy(), None, None).unwrap().paths;
    let names: Vec<_> = paths
        .iter()
        .map(|p| p.strip_prefix(&root).unwrap().to_path_buf())
        .collect();
    assert_eq!(
        names,
        [
            PathBuf::from("A album").join("01.flac"),
            PathBuf::from("A album").join("02.flac"),
            PathBuf::from("B album").join("01.flac"),
            PathBuf::from("B album").join("02.flac")
        ]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_artist_name_plays_own_albums_in_year_and_track_order() {
    let catalog = Catalog {
        artists: vec![Artist {
            id: 0,
            name: "Ozzy Osbourne".into(),
            key: "ozzy osbourne".into(),
            ..Default::default()
        }],
        releases: vec![
            Release {
                id: 0,
                title: "Later".into(),
                key: "later".into(),
                album_artist_id: Some(0),
                year: Some(1991),
                track_ids: vec![0, 1],
                ..Default::default()
            },
            Release {
                id: 1,
                title: "Earlier".into(),
                key: "earlier".into(),
                album_artist_id: Some(0),
                year: Some(1980),
                track_ids: vec![2],
                ..Default::default()
            },
        ],
        tracks: (0..3)
            .map(|id| Track {
                id,
                file_id: id,
                title: format!("Track {id}"),
                ..Default::default()
            })
            .collect(),
        files: ["later/01.flac", "later/02.flac", "earlier/01.flac"]
            .iter()
            .enumerate()
            .map(|(id, path)| AudioFile {
                id: id as u32,
                path: (*path).into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let paths = resolve("ozzy", Some(&catalog), None).unwrap().paths;
    assert_eq!(
        paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        ["earlier/01.flac", "later/01.flac", "later/02.flac"]
    );
}

#[test]
fn an_exact_title_plays_every_copy_before_a_partial_artist_match() {
    let catalog = Catalog {
        artists: vec![Artist {
            id: 0,
            name: "Ozzy Osbourne".into(),
            key: "ozzy osbourne".into(),
            ..Default::default()
        }],
        releases: vec![Release {
            id: 0,
            album_artist_id: Some(0),
            track_ids: vec![0],
            ..Default::default()
        }],
        tracks: (0..3)
            .map(|id| Track {
                id,
                file_id: id,
                title: if id == 0 { "Other" } else { "Ozzy" }.into(),
                ..Default::default()
            })
            .collect(),
        files: ["other.flac", "b.flac", "a.flac"]
            .iter()
            .enumerate()
            .map(|(id, path)| AudioFile {
                id: id as u32,
                path: (*path).into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let paths = resolve("ozzy", Some(&catalog), None).unwrap().paths;
    assert_eq!(paths, [PathBuf::from("a.flac"), PathBuf::from("b.flac")]);
}

#[test]
fn a_direct_file_records_history_without_a_catalog() {
    let root = std::env::temp_dir().join(format!("aede_play_history_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let args = Args::parse([
        "play".to_string(),
        "--data".to_string(),
        root.to_string_lossy().into_owned(),
    ]);
    let path = fixture("audit-stereo.flac").canonicalize().unwrap();
    record_play(&args, &path, 123, 456, true).unwrap();
    let data = user::load(&user::user_path(&root)).unwrap().unwrap();
    assert_eq!(data.plays.len(), 1);
    assert_eq!(data.plays[0].track.key, path.to_string_lossy());
    assert_eq!(data.plays[0].ms_played, 456);
    assert!(data.plays[0].completed);
    std::fs::remove_dir_all(root).unwrap();
}
