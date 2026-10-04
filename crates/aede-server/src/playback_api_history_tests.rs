use super::*;

fn sources(count: usize) -> Vec<TrackSource> {
    vec![
        TrackSource {
            reference: EntityRef::new(EntityKind::Track, "sample"),
            path: PathBuf::from("sample.wav"),
            file: AudioFile::default(),
            mtime_subseconds: 0,
        };
        count
    ]
}

fn begin(timeline: &mut ListeningTimeline, sources: &[TrackSource], index: usize, frames: u64) {
    timeline
        .begin(
            &TrackFrame {
                kind: "track",
                index,
                track: sources[index].reference.to_token(),
                start_frame: frames,
                duration_ms: None,
                position_ms: None,
            },
            frames,
            8_000,
            sources,
        )
        .unwrap();
}

fn end(timeline: &mut ListeningTimeline, sources: &[TrackSource], index: usize, frames: u64) {
    timeline
        .end(
            &TrackEndFrame {
                kind: "track_end",
                index,
                track: sources[index].reference.to_token(),
                end_frame: frames,
            },
            frames,
        )
        .unwrap();
}

#[test]
fn acknowledgements_partition_repeated_occurrences_without_overcounting_the_last() {
    let sources = sources(3);
    let mut timeline = ListeningTimeline::default();
    begin(&mut timeline, &sources, 0, 0);
    end(&mut timeline, &sources, 0, 80);
    begin(&mut timeline, &sources, 1, 80);
    end(&mut timeline, &sources, 1, 160);
    begin(&mut timeline, &sources, 2, 160);
    let listens = timeline.listens(200, &sources);
    assert_eq!(
        listens
            .iter()
            .map(|listen| (listen.index, listen.ms_played, listen.completed))
            .collect::<Vec<_>>(),
        [(0, 10, true), (1, 10, true), (2, 5, false)]
    );
    let listens = timeline.listens(120, &sources);
    assert_eq!(
        listens
            .iter()
            .map(|listen| (listen.index, listen.ms_played, listen.completed))
            .collect::<Vec<_>>(),
        [(0, 10, true), (1, 5, false)]
    );
}

#[test]
fn empty_or_sub_millisecond_occurrences_do_not_invent_history() {
    let sources = sources(3);
    let mut timeline = ListeningTimeline::default();
    begin(&mut timeline, &sources, 0, 0);
    end(&mut timeline, &sources, 0, 0);
    begin(&mut timeline, &sources, 1, 0);
    end(&mut timeline, &sources, 1, 7);
    begin(&mut timeline, &sources, 2, 7);
    end(&mut timeline, &sources, 2, 15);
    let listens = timeline.listens(15, &sources);
    assert_eq!(listens.len(), 1);
    assert_eq!(
        (listens[0].index, listens[0].ms_played, listens[0].completed),
        (2, 1, true)
    );
}

#[test]
fn malformed_track_boundaries_cannot_change_the_attribution() {
    let sources = sources(2);
    let mut timeline = ListeningTimeline::default();
    begin(&mut timeline, &sources, 0, 0);
    let incorrect = TrackEndFrame {
        kind: "track_end",
        index: 1,
        track: sources[0].reference.to_token(),
        end_frame: 80,
    };
    assert!(timeline.end(&incorrect, 80).is_err());
    assert!(!timeline.listens(80, &sources)[0].completed);
    end(&mut timeline, &sources, 0, 80);
    assert!(
        timeline
            .end(
                &TrackEndFrame {
                    index: 0,
                    ..incorrect
                },
                80
            )
            .is_err()
    );
    let wrong_start = TrackFrame {
        kind: "track",
        index: 1,
        track: sources[1].reference.to_token(),
        start_frame: 79,
        duration_ms: None,
        position_ms: None,
    };
    assert!(timeline.begin(&wrong_start, 80, 8_000, &sources).is_err());
    begin(&mut timeline, &sources, 1, 80);
    assert_eq!(timeline.listens(120, &sources).len(), 2);
}
