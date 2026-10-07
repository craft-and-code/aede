#![cfg(unix)]

mod playback_transport_support;

use playback_transport_support::{Library, pcm};

#[test]
fn flac_md5_mismatch_stops_the_queue_and_never_publishes_complete_bad_audio_or_loudness() {
    let mut library = Library::new();
    let valid = library.copy_core_fixture("track.flac");
    let invalid = library.flac_with_wrong_audio_md5(&valid);
    let unvisited = library.wav("unvisited.wav", 4800, 7000);
    let valid_key = valid.canonicalize().unwrap();
    let invalid_key = invalid.canonicalize().unwrap();
    let unvisited_key = unvisited.canonicalize().unwrap();
    for mode in ["track", "album"] {
        let result = library.play(
            &[&valid, &invalid, &unvisited],
            &["--normalize", mode],
            true,
        );
        assert!(
            !result.output.status.success(),
            "MD5 mismatch must fail playback"
        );
        assert!(String::from_utf8_lossy(&result.output.stderr).contains("MD5"));
        let history = result.history().expect("audio before failure is retained");
        assert_eq!(history.plays.len(), 2);
        assert_eq!(std::path::Path::new(&history.plays[0].track.key), valid_key);
        assert!(history.plays[0].completed);
        assert_eq!(history.plays[0].ms_played, 1000);
        assert_eq!(
            std::path::Path::new(&history.plays[1].track.key),
            invalid_key
        );
        assert!(!history.plays[1].completed);
        assert!(history.plays[1].ms_played > 0 && history.plays[1].ms_played <= 1000);
        assert_eq!(history.counts.len(), 2, "the later track was never visited");
        let cache =
            aede_core::conclusions::load(&aede_core::conclusions::conclusions_path(&result.data))
                .unwrap();
        if mode == "track" {
            assert!(
                cache.as_ref().is_some_and(|cache| {
                    cache.loudness_tracks.len() == 1
                        && cache
                            .loudness_tracks
                            .contains_key(&valid_key.to_string_lossy().into_owned())
                }),
                "the completed valid track keeps its measured loudness"
            );
        }
        if let Some(cache) = cache {
            assert!(
                cache
                    .loudness_tracks
                    .keys()
                    .all(|track| std::path::Path::new(track) != invalid_key
                        && std::path::Path::new(track) != unvisited_key)
            );
            assert!(
                cache.loudness_programmes.is_empty(),
                "a failed source cannot complete its album programme"
            );
        }
    }
}

#[test]
fn a_flac_md5_mismatch_after_initial_seek_is_a_decode_failure_not_a_complete_suffix_listen() {
    let mut library = Library::new();
    let valid = library.copy_core_fixture("track.flac");
    let invalid = library.flac_with_wrong_audio_md5(&valid);
    let result = library.play(&[&invalid], &["--playback", "dsp", "--seek", "0.1"], false);
    assert!(!result.output.status.success());
    assert!(String::from_utf8_lossy(&result.output.stderr).contains("MD5"));
    let history = result
        .history()
        .expect("played suffix is retained as incomplete");
    assert_eq!(history.plays.len(), 1);
    assert!(!history.plays[0].completed);
    assert!(history.plays[0].ms_played > 0 && history.plays[0].ms_played <= 900);
    result.assert_no_loudness_cache();
}

#[test]
fn seeking_past_flac_eof_checks_all_discarded_audio_md5_without_creating_a_listen_or_cache() {
    let mut library = Library::new();
    let valid = library.copy_core_fixture("track.flac");
    let invalid = library.flac_with_wrong_audio_md5(&valid);
    let result = library.play(&[&invalid], &["--playback", "dsp", "--seek", "10"], false);
    assert!(!result.output.status.success());
    assert!(String::from_utf8_lossy(&result.output.stderr).contains("MD5"));
    assert!(result.pcm.is_empty());
    assert!(
        result
            .history()
            .is_none_or(|history| history.plays.is_empty() && history.counts.is_empty())
    );
    result.assert_no_loudness_cache();
}

#[test]
fn stopping_flac_before_eof_records_only_partial_audio_without_claiming_an_md5_result() {
    let mut library = Library::new();
    let valid = library.copy_core_fixture("track.flac");
    let invalid = library.flac_with_wrong_audio_md5(&valid);
    let result = library.terminal(&[&invalid], "flac-md5-stop");
    result.assert_success();
    let history = result.history().expect("partial audio is retained");
    assert_eq!(history.plays.len(), 1);
    assert!(!history.plays[0].completed);
    assert!(history.plays[0].ms_played > 0 && history.plays[0].ms_played < 1000);
    result.assert_no_loudness_cache();
}

#[test]
fn lyric_cues_follow_pause_both_seek_directions_and_manual_next_in_a_real_terminal() {
    let mut library = Library::new();
    let first = library.timeline("first.wav", 24);
    let second = library.timeline("second.wav", 24);
    std::fs::write(
        first.with_extension("lrc"),
        "[00:00]first-opening\n[00:10]first-later",
    )
    .unwrap();
    std::fs::write(second.with_extension("lrc"), "[00:00]second-opening").unwrap();
    let result = library.terminal(&[&first, &second], "lyrics-transport");
    result.assert_success();
    let evidence = aede_core::json::parse(&String::from_utf8_lossy(&result.output.stdout)).unwrap();
    let lyrics = evidence.get("lyrics").unwrap().as_arr().unwrap();
    assert_eq!(lyrics.len(), 4);
    for (line, expected) in lyrics.iter().zip([
        "first-opening",
        "first-later",
        "first-opening",
        "second-opening",
    ]) {
        assert!(line.as_str().unwrap().contains(expected));
    }
    assert_eq!(
        result.history().unwrap().plays.len(),
        2,
        "seeking keeps one listen per occurrence"
    );
}

#[test]
fn natural_repeat_replays_lyric_cues_without_reopening_the_output() {
    let mut library = Library::new();
    let first = library.timeline("first.wav", 2);
    std::fs::write(first.with_extension("lrc"), "[00:00]repeated-opening").unwrap();
    let result = library.terminal(&[&first], "lyrics-repeat");
    result.assert_success();
    let evidence = aede_core::json::parse(&String::from_utf8_lossy(&result.output.stdout)).unwrap();
    assert_eq!(
        evidence
            .get("first_samples")
            .unwrap()
            .as_arr()
            .unwrap()
            .len(),
        1
    );
    assert!(evidence.get("lyrics").unwrap().as_arr().unwrap().len() >= 2);
}

#[test]
fn redirected_lyric_playback_is_refused_before_audio_or_history_is_written() {
    let mut library = Library::new();
    let first = library.timeline("first.wav", 1);
    let result = library.play(&[&first], &["--lyrics"], false);
    assert!(!result.output.status.success());
    assert!(String::from_utf8_lossy(&result.output.stderr).contains("requires terminal output"));
    assert!(result.pcm.is_empty());
    assert!(result.history().is_none());
}

#[test]
fn initial_seek_submits_the_exact_suffix_and_counts_only_audio_actually_played() {
    let mut library = Library::new();
    let track = library.wav("seek.wav", 48_000, 2_000);
    let original = pcm(&track);
    for time in ["0.137", "00:00.137", "00:00:00.137"] {
        let result = library.play(&[&track], &["--normalize", "off", "--seek", time], false);
        result.assert_success();
        assert_eq!(result.pcm, original[6_576 * 4..]);
        let history = result.history().expect("played suffix is recorded");
        assert_eq!(history.plays.len(), 1);
        assert_eq!(history.plays[0].ms_played, 863);
        assert!(
            !history.plays[0].completed,
            "seeking skipped part of the source"
        );
        assert_eq!(history.counts.len(), 1);
        assert_eq!(history.counts[0].count, 1);
    }
}

#[test]
fn seeking_into_a_track_never_publishes_suffix_loudness_as_a_whole_track_measurement() {
    let mut library = Library::new();
    let track = library.copy_core_fixture("playback-stereo.flac");
    let original = pcm(&track);
    let result = library.play(&[&track], &["--playback", "dsp", "--seek", "0.1"], false);
    result.assert_success();
    assert_eq!(result.pcm, original[4_410 * 2 * 4..]);
    result.assert_no_loudness_cache();
    let history = result.history().expect("suffix listen");
    assert_eq!(history.plays.len(), 1);
    assert_eq!(
        history.plays[0].ms_played,
        (original.len() / 8 - 4_410) as u64 * 1000 / 44_100
    );
    assert!(!history.plays[0].completed);
}

#[test]
fn initial_seek_past_actual_eof_produces_no_audio_listen_or_loudness_cache() {
    let mut library = Library::new();
    let track = library.copy_core_fixture("playback-stereo.flac");
    let result = library.play(&[&track], &["--playback", "dsp", "--seek", "10"], false);
    result.assert_success();
    assert!(result.pcm.is_empty());
    assert!(
        result
            .history()
            .is_none_or(|history| history.plays.is_empty() && history.counts.is_empty())
    );
    result.assert_no_loudness_cache();
}

#[test]
fn initial_seek_applies_only_to_the_first_entry_and_clamped_eof_advances_the_queue() {
    let mut library = Library::new();
    let first = library.wav("first.wav", 4_800, 1_000);
    let second = library.wav("second.wav", 4_800, 4_000);
    for seek in ["0.02", "1"] {
        let result = library.play(
            &[&first, &second],
            &["--normalize", "off", "--seek", seek],
            false,
        );
        result.assert_success();
        let mut expected = if seek == "0.02" {
            pcm(&first)[960 * 4..].to_vec()
        } else {
            Vec::new()
        };
        expected.extend_from_slice(&pcm(&second));
        assert_eq!(result.pcm, expected);
        let history = result.history().expect("remaining queue listen");
        assert_eq!(history.plays.len(), if seek == "0.02" { 2 } else { 1 });
        let last = history.plays.last().unwrap();
        assert_eq!(
            std::path::Path::new(&last.track.key),
            second.canonicalize().unwrap()
        );
        assert_eq!(last.ms_played, 100);
        assert!(
            last.completed,
            "later entries are played from their beginning"
        );
    }
}

#[test]
fn terminal_next_before_playback_keeps_initial_seek_for_the_first_started_entry() {
    for (scenario, next_count) in [("initial-seek-next", 1), ("initial-seek-next-twice", 2)] {
        let mut library = Library::new();
        let paths = [
            library.wav("first.wav", 24_000, 1_000),
            library.wav("second.wav", 24_000, 4_000),
            library.wav("third.wav", 24_000, 7_000),
        ];
        let entries: Vec<_> = paths[..=next_count].iter().map(|path| &**path).collect();
        let expected_track = &paths[next_count];
        let result = library.terminal(&entries, scenario);
        result.assert_success();
        let expected = pcm(expected_track)[960 * 4..].to_vec();
        assert_eq!(
            result.pcm.len(),
            expected.len(),
            "the initial 20 ms must be skipped on the first started entry"
        );
        assert!(
            result.pcm == expected,
            "submitted suffix must match source bytes exactly"
        );
        let history = result.history().expect("first started entry history");
        assert_eq!(history.plays.len(), 1);
        assert_eq!(
            std::path::Path::new(&history.plays[0].track.key),
            expected_track.canonicalize().unwrap()
        );
        assert_eq!(history.plays[0].ms_played, 480);
        assert!(!history.plays[0].completed);
        assert_eq!(history.counts.len(), 1);
        assert_eq!(history.counts[0].count, 1);
        result.assert_no_loudness_cache();
    }
}

#[test]
fn classic_shuffle_is_seeded_and_keeps_every_playlist_entry_including_repeated_files() {
    let mut library = Library::new();
    let paths = [
        library.wav("first.wav", 4_800, 1_000),
        library.wav("second.wav", 4_800, 4_000),
        library.wav("third.wav", 4_800, 7_000),
        library.wav("fourth.wav", 4_800, 10_000),
    ];
    let entries = [&*paths[0], &*paths[1], &*paths[0], &*paths[2], &*paths[3]];
    let options = ["--normalize", "off", "--shuffle", "random", "--seed", "42"];
    let first = library.play(&entries, &options, false);
    let second = library.play(&entries, &options, false);
    first.assert_success();
    second.assert_success();
    assert_eq!(
        first.pcm, second.pcm,
        "seed reproduces the complete PCM order"
    );
    let mut expected = entries.iter().map(|path| pcm(path)).collect::<Vec<_>>();
    expected.sort();
    let mut actual = first
        .pcm
        .chunks(4_800 * 4)
        .map(<[u8]>::to_vec)
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(
        actual, expected,
        "shuffle permutes queue entries without deduplicating paths"
    );
    let history = first.history().expect("all queue entries recorded");
    assert_eq!(history.plays.len(), entries.len());
    assert!(
        history
            .plays
            .iter()
            .all(|play| play.completed && play.ms_played == 100)
    );
    let key = paths[0]
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        history
            .counts
            .iter()
            .find(|count| count.track.key == key)
            .unwrap()
            .count,
        2
    );
}

#[test]
fn smart_shuffle_drives_a_reproducible_complete_catalogued_selection() {
    for metadata in ["connected", "disconnected", "missing"] {
        let mut library = Library::new();
        let paths = [
            library.wav("first.wav", 4_800, 1_000),
            library.wav("second.wav", 4_800, 4_000),
            library.wav("third.wav", 4_800, 7_000),
            library.wav("fourth.wav", 4_800, 10_000),
        ];
        for (index, path) in paths.iter().enumerate() {
            match metadata {
                "disconnected" => library.set_genres(
                    path,
                    &[if index < 2 {
                        "Synthetic style azure"
                    } else {
                        "Synthetic style ochre"
                    }],
                ),
                "missing" => library.set_genres(path, &[]),
                _ => {}
            }
        }
        let entries = [&*paths[0], &*paths[1], &*paths[2], &*paths[0], &*paths[3]];
        let options = ["--normalize", "off", "--shuffle", "smart", "--seed", "314"];
        let first = library.play(&entries, &options, true);
        let second = library.play(&entries, &options, true);
        first.assert_success();
        second.assert_success();
        assert_eq!(first.pcm, second.pcm, "{metadata}: seeded order");
        let mut expected = entries.iter().map(|path| pcm(path)).collect::<Vec<_>>();
        expected.sort();
        let mut actual = first
            .pcm
            .chunks(4_800 * 4)
            .map(<[u8]>::to_vec)
            .collect::<Vec<_>>();
        actual.sort();
        assert_eq!(
            actual, expected,
            "{metadata}: every occurrence is played once"
        );
        let history = first.history().expect("smart queue histories");
        assert_eq!(history.plays.len(), entries.len());
        assert!(
            history
                .plays
                .iter()
                .all(|play| play.completed && play.ms_played == 100)
        );
        let warning = String::from_utf8_lossy(&first.output.stderr);
        match metadata {
            "disconnected" => {
                assert!(warning.contains("Smart transition:"), "{warning}");
                assert!(warning.contains("style distance 1000/1000"), "{warning}");
                assert!(
                    warning.contains("every selection entry is retained"),
                    "{warning}"
                );
            }
            "missing" => {
                assert!(warning.contains("5 tracks without genres"), "{warning}");
                assert!(warning.contains("Smart transition:"), "{warning}");
                assert!(warning.contains("genre evidence unavailable"), "{warning}");
            }
            _ => assert!(!warning.contains("Smart transition:"), "{warning}"),
        }
    }
}

#[test]
fn terminal_repeat_one_replays_the_same_entry_until_the_user_stops() {
    let mut library = Library::new();
    let first = library.wav("first.wav", 24_000, 2_000);
    let second = library.wav("second.wav", 24_000, 5_000);
    let result = library.terminal(&[&first, &second], "repeat-one");
    result.assert_success();
    let history = result.history().expect("repeated listens");
    assert!(history.plays.len() >= 2);
    assert!(
        history
            .plays
            .iter()
            .take(2)
            .all(|play| play.completed && play.ms_played == 500)
    );
    assert!(
        history
            .plays
            .iter()
            .all(|play| std::path::Path::new(&play.track.key) == first.canonicalize().unwrap())
    );
    assert!(history.counts[0].count >= 2);
}

#[test]
fn terminal_repeat_all_returns_to_the_selection_start_without_losing_order() {
    let mut library = Library::new();
    let first = library.wav("first.wav", 24_000, 2_000);
    let second = library.wav("second.wav", 24_000, 5_000);
    let result = library.terminal(&[&first, &second], "repeat-all");
    result.assert_success();
    let history = result.history().expect("repeated queue listens");
    assert!(history.plays.len() >= 4);
    let expected = [&first, &second, &first, &second];
    for (play, path) in history.plays.iter().zip(expected) {
        assert_eq!(
            std::path::Path::new(&play.track.key),
            path.canonicalize().unwrap()
        );
        assert!(play.completed);
        assert_eq!(play.ms_played, 500);
    }
}

#[test]
fn terminal_forward_and_backward_seek_keep_one_partial_listen_without_caching_fragments() {
    let mut library = Library::new();
    let track = library.timeline("timeline.wav", 24);
    let result = library.terminal(&[&track], "seek");
    result.assert_success();
    let evidence = aede_core::json::parse(&String::from_utf8_lossy(&result.output.stdout))
        .expect("terminal probe evidence");
    let samples = evidence.get("first_samples").unwrap().as_arr().unwrap();
    assert_eq!(
        samples.len(),
        3,
        "each seek discards the previous queued output"
    );
    let first = samples[0].as_f64().unwrap();
    let forward = samples[1].as_f64().unwrap();
    let backward = samples[2].as_f64().unwrap();
    assert_eq!(first, 2_000.0 / 32_768.0);
    assert!(
        forward >= 12_000.0 / 32_768.0,
        "forward seek passes ten source seconds"
    );
    assert!(
        backward <= forward - 8_000.0 / 32_768.0,
        "backward seek returns towards the beginning"
    );
    let history = result.history().expect("partial seek listen");
    assert_eq!(history.plays.len(), 1, "seek segments belong to one visit");
    assert!(!history.plays[0].completed);
    assert!(history.plays[0].ms_played > 0);
    let elapsed = evidence
        .field_u64("elapsed_ms")
        .expect("bounded probe elapsed time");
    assert!(
        history.plays[0].ms_played <= elapsed + 500,
        "skipped position is not counted as listening time"
    );
    assert_eq!(history.counts.len(), 1);
    assert_eq!(history.counts[0].count, 1);
    result.assert_no_loudness_cache();
}

#[test]
fn terminal_seek_past_end_closes_the_partial_visit_before_playing_the_next_entry() {
    let mut library = Library::new();
    let first = library.wav("first.wav", 48_000 * 5, 2_000);
    let second = library.wav("second.wav", 48_000, 5_000);
    let result = library.terminal(&[&first, &second], "seek-past-end");
    result.assert_success();
    let history = result.history().expect("interrupted and following visits");
    assert_eq!(history.plays.len(), 2);
    let first_key = first.canonicalize().unwrap();
    let second_key = second.canonicalize().unwrap();
    assert_eq!(std::path::Path::new(&history.plays[0].track.key), first_key);
    assert!(!history.plays[0].completed);
    assert!(history.plays[0].ms_played > 0 && history.plays[0].ms_played < 5_000);
    assert_eq!(
        std::path::Path::new(&history.plays[1].track.key),
        second_key
    );
    assert!(history.plays[1].completed);
    assert_eq!(history.plays[1].ms_played, 1_000);
    assert_eq!(history.counts.len(), 2);
    assert!(history.counts.iter().all(|count| count.count == 1));
    let cache =
        aede_core::conclusions::load(&aede_core::conclusions::conclusions_path(&result.data))
            .expect("playback conclusions")
            .expect("complete second track cache");
    assert!(
        !cache
            .loudness_tracks
            .contains_key(&first_key.to_string_lossy().into_owned())
    );
    assert!(
        cache
            .loudness_tracks
            .contains_key(&second_key.to_string_lossy().into_owned())
    );
}

#[test]
fn terminal_repeat_and_shuffle_modes_change_while_paused_without_resuming_audio() {
    let mut library = Library::new();
    let first = library.wav("first.wav", 48_000 * 5, 2_000);
    let second = library.wav("second.wav", 48_000, 5_000);
    let third = library.wav("third.wav", 48_000, 8_000);
    let result = library.terminal(&[&first, &second, &third], "paused-modes");
    result.assert_success();
    let history = result.history().expect("resumed partial listen");
    assert_eq!(history.plays.len(), 1);
    assert_eq!(
        std::path::Path::new(&history.plays[0].track.key),
        first.canonicalize().unwrap()
    );
    assert!(!history.plays[0].completed);
    assert!(history.plays[0].ms_played > 0);
    result.assert_no_loudness_cache();
}
