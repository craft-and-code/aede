//! Tests for [`super`], split out of `doctor.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;
use crate::model::{self, IntegrityRecord, ScannedFile};
use crate::tags::RawTags;

fn file(path: &str, fields: &[(&str, &str)], duration: Option<u64>, codec: &str) -> ScannedFile {
    let mut tags = RawTags::default();
    for (k, v) in fields {
        tags.insert(k, *v);
    }
    tags.properties.codec = codec.into();
    tags.properties.lossless = codec == "flac";
    tags.properties.sample_rate = Some(44_100);
    tags.properties.duration_ms = duration;
    ScannedFile {
        path: path.into(),
        size: 5_000_000,
        mtime: 0,
        tags,
        folder_cover: None,
        sidecar: None,
        integrity: None,
        fingerprint: None,
    }
}

fn count(issues: &[Issue], kind: IssueKind) -> usize {
    issues.iter().filter(|i| i.kind == kind).count()
}

#[test]
fn detects_missing_tags() {
    let c = model::build(
        vec![file("/m/a/01 no tags.flac", &[], Some(1000), "flac")],
        vec!["/m".into()],
        0,
    );
    let issues = diagnose(&c, &crate::sources::Sources::default());
    assert_eq!(count(&issues, IssueKind::MissingTitle), 1);
    assert_eq!(count(&issues, IssueKind::MissingArtist), 1);
    assert_eq!(count(&issues, IssueKind::MissingAlbum), 1);
    assert_eq!(count(&issues, IssueKind::MissingDate), 1);
}

#[test]
fn detects_an_unreadable_duration() {
    let c = model::build(
        vec![file(
            "/m/a/01.flac",
            &[
                ("title", "X"),
                ("artist", "Y"),
                ("album", "Z"),
                ("date", "2000"),
            ],
            None,
            "flac",
        )],
        vec!["/m".into()],
        0,
    );
    let issues = diagnose(&c, &crate::sources::Sources::default());
    assert_eq!(count(&issues, IssueKind::MissingDuration), 1);
    assert_eq!(
        issues
            .iter()
            .find(|i| i.kind == IssueKind::MissingDuration)
            .unwrap()
            .severity(),
        Severity::Error
    );
}

#[test]
fn a_copied_album_is_reported_once_and_not_per_track() {
    // The same three tracks in two folders. Before, this produced three
    // "likely duplicate" lines saying the same thing.
    let common = |n: &'static str, title: &'static str| {
        vec![
            ("title", title),
            ("artist", "Danzig"),
            ("albumartist", "Danzig"),
            ("album", "Danzig 4"),
            ("date", "1994"),
            ("tracknumber", n),
        ]
    };
    let mut files = Vec::new();
    for folder in ["/m/A", "/m/B"] {
        for (n, title) in [
            ("1", "Brand New God"),
            ("2", "Little Whip"),
            ("3", "Cantspeak"),
        ] {
            files.push(file(
                &format!("{folder}/{n}.flac"),
                &common(n, title),
                Some(100_000 + n.parse::<u64>().unwrap_or(0) * 1000),
                "flac",
            ));
        }
    }
    let c = model::build(files, vec!["/m".into()], 0);
    let issues = diagnose(&c, &crate::sources::Sources::default());
    assert_eq!(count(&issues, IssueKind::DuplicateAlbum), 1);
    assert_eq!(
        count(&issues, IssueKind::DuplicateTrack),
        0,
        "the album line already says it"
    );
    let album = issues
        .iter()
        .find(|i| i.kind == IssueKind::DuplicateAlbum)
        .unwrap();
    assert_eq!(album.files.len(), 2, "both folders are named");
    assert_eq!(album.severity(), Severity::Warning);
}

#[test]
fn detects_a_duplicate_but_not_a_live_version() {
    let common = |album: &'static str| {
        vec![
            ("title", "So What"),
            ("artist", "Miles Davis"),
            ("album", album),
            ("date", "1959"),
        ]
    };
    let c = model::build(
        vec![
            file(
                "/m/a/01.flac",
                &common("Kind of Blue"),
                Some(545_000),
                "flac",
            ),
            // Near-identical copy: duplicate.
            file("/m/b/01.mp3", &common("Best of"), Some(546_000), "mp3"),
            // Live version, markedly longer: not a duplicate.
            file("/m/c/01.flac", &common("Live"), Some(900_000), "flac"),
        ],
        vec!["/m".into()],
        0,
    );
    let issues = diagnose(&c, &crate::sources::Sources::default());
    let duplicates: Vec<&Issue> = issues
        .iter()
        .filter(|i| i.kind == IssueKind::DuplicateTrack)
        .collect();
    assert_eq!(duplicates.len(), 1, "only one duplicate group expected");
    assert_eq!(duplicates[0].files.len(), 2);
}

#[test]
fn an_album_cut_short_at_the_end_is_incomplete_too() {
    // Counting gaps alone, an album truncated after track 2 of 5 looks
    // whole: there is nothing between 1 and 2 left to be missing. That is
    // the shape an interrupted rip has, and the check was blind to it.
    let track = |n: &'static str| {
        vec![
            ("title", "T"),
            ("artist", "A"),
            ("album", "Album"),
            ("date", "1990"),
            ("tracknumber", n),
            ("tracktotal", "5"),
        ]
    };
    let c = model::build(
        vec![
            file("/m/a/01.flac", &track("1"), Some(1000), "flac"),
            file("/m/a/02.flac", &track("2"), Some(2000), "flac"),
        ],
        vec!["/m".into()],
        0,
    );
    let gap = diagnose(&c, &crate::sources::Sources::default())
        .into_iter()
        .find(|i| i.kind == IssueKind::IncompleteAlbum)
        .expect("the missing tail must be seen");
    for n in ["3", "4", "5"] {
        assert!(gap.detail.contains(n), "detail: {}", gap.detail);
    }
}

#[test]
fn a_total_smaller_than_what_is_there_is_a_wrong_tag_not_a_gap() {
    // A disc announcing 2 while holding 3 has a bad tag; inventing missing
    // tracks from it would report a defect that is not one.
    let track = |n: &'static str| {
        vec![
            ("title", "T"),
            ("artist", "A"),
            ("album", "Album"),
            ("date", "1990"),
            ("tracknumber", n),
            ("tracktotal", "2"),
        ]
    };
    let c = model::build(
        vec![
            file("/m/b/01.flac", &track("1"), Some(1000), "flac"),
            file("/m/b/02.flac", &track("2"), Some(2000), "flac"),
            file("/m/b/03.flac", &track("3"), Some(3000), "flac"),
        ],
        vec!["/m".into()],
        0,
    );
    assert!(
        !diagnose(&c, &crate::sources::Sources::default())
            .iter()
            .any(|i| i.kind == IssueKind::IncompleteAlbum),
        "nothing is missing here"
    );
}

/// One disc of a set, complete in itself.
fn disc(path: &'static str, disc: &'static str, total: &'static str) -> ScannedFile {
    file(
        path,
        &[
            ("title", "T"),
            ("artist", "A"),
            ("album", "Box"),
            ("date", "1997"),
            ("tracknumber", "1"),
            ("tracktotal", "1"),
            ("discnumber", disc),
            ("disctotal", total),
        ],
        Some(1000),
        "flac",
    )
}

#[test]
fn a_set_missing_a_whole_disc_is_incomplete() {
    // Every disc that is there is complete, so the gap check sees nothing:
    // a four-disc soundtrack ripped as three looks perfect until the day
    // it is played. Only the announced total can say otherwise.
    let c = model::build(
        vec![
            disc("/m/box/Disc 1/01.flac", "1", "4"),
            disc("/m/box/Disc 2/01.flac", "2", "4"),
            disc("/m/box/Disc 3/01.flac", "3", "4"),
        ],
        vec!["/m".into()],
        0,
    );
    let issue = diagnose(&c, &crate::sources::Sources::default())
        .into_iter()
        .find(|i| i.kind == IssueKind::IncompleteAlbum)
        .expect("the missing disc must be seen");
    assert_eq!(issue.detail, "\"Box\": missing disc 4 of 4");
}

#[test]
fn a_hole_in_the_disc_numbers_needs_no_total() {
    // Nothing announces four here; discs 1 and 3 alone are enough to say
    // that the second is not on the shelf.
    let c = model::build(
        vec![
            disc("/m/box/Disc 1/01.flac", "1", ""),
            disc("/m/box/Disc 3/01.flac", "3", ""),
        ],
        vec!["/m".into()],
        0,
    );
    let issue = diagnose(&c, &crate::sources::Sources::default())
        .into_iter()
        .find(|i| i.kind == IssueKind::IncompleteAlbum)
        .expect("the hole must be seen");
    assert_eq!(issue.detail, "\"Box\": missing disc 2");
}

#[test]
fn a_complete_set_and_a_plain_album_say_nothing() {
    // The check has to be silent on the ordinary case, or it becomes the
    // line everybody learns to skip.
    let c = model::build(
        vec![
            disc("/m/box/Disc 1/01.flac", "1", "2"),
            disc("/m/box/Disc 2/01.flac", "2", "2"),
            file(
                "/m/plain/01.flac",
                &[
                    ("title", "T"),
                    ("artist", "A"),
                    ("album", "Plain"),
                    ("date", "1990"),
                    ("tracknumber", "1"),
                    ("tracktotal", "1"),
                ],
                Some(1000),
                "flac",
            ),
        ],
        vec!["/m".into()],
        0,
    );
    assert!(
        !diagnose(&c, &crate::sources::Sources::default())
            .iter()
            .any(|i| i.kind == IssueKind::IncompleteAlbum),
        "nothing is missing in either"
    );
}

#[test]
fn a_disc_in_a_folder_of_its_own_is_not_a_disc_that_is_missing() {
    // "Box CD1" beside "Box CD2" is a layout the disc-folder rule does not
    // recognise — the names are not bare disc folders — so the set arrives
    // as two releases. Each announces two discs and holds one, and each
    // would report the other as missing while it sits right there.
    let c = model::build(
        vec![
            disc("/m/Box CD1/01.flac", "1", "2"),
            disc("/m/Box CD2/01.flac", "2", "2"),
        ],
        vec!["/m".into()],
        0,
    );
    assert_eq!(c.releases.len(), 2, "two folders, two releases");
    assert!(
        !diagnose(&c, &crate::sources::Sources::default())
            .iter()
            .any(|i| i.kind == IssueKind::IncompleteAlbum),
        "both discs are in the library: {:#?}",
        diagnose(&c, &crate::sources::Sources::default())
            .iter()
            .filter(|i| i.kind == IssueKind::IncompleteAlbum)
            .map(|i| i.detail.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn detects_an_incomplete_album() {
    let track = |n: &'static str| {
        vec![
            ("title", "T"),
            ("artist", "A"),
            ("album", "Album"),
            ("date", "1990"),
            ("tracknumber", n),
        ]
    };
    let c = model::build(
        vec![
            file("/m/a/01.flac", &track("1"), Some(1000), "flac"),
            file("/m/a/03.flac", &track("3"), Some(2000), "flac"),
        ],
        vec!["/m".into()],
        0,
    );
    let issues = diagnose(&c, &crate::sources::Sources::default());
    let gap = issues
        .iter()
        .find(|i| i.kind == IssueKind::IncompleteAlbum)
        .unwrap();
    assert!(gap.detail.contains('2'), "detail: {}", gap.detail);
}

#[test]
fn detects_mixed_quality() {
    let track = |n: &'static str| {
        vec![
            ("title", "T"),
            ("artist", "A"),
            ("album", "Album"),
            ("date", "1990"),
            ("tracknumber", n),
        ]
    };
    let c = model::build(
        vec![
            file("/m/a/01.flac", &track("1"), Some(1000), "flac"),
            file("/m/a/02.mp3", &track("2"), Some(2000), "mp3"),
        ],
        vec!["/m".into()],
        0,
    );
    assert_eq!(
        count(
            &diagnose(&c, &crate::sources::Sources::default()),
            IssueKind::MixedQuality
        ),
        1
    );
}

#[test]
fn healthy_library_reports_nothing() {
    let track = |title: &'static str, n: &'static str| {
        vec![
            ("title", title),
            ("artist", "A"),
            ("album", "Album"),
            ("date", "1990"),
            ("tracknumber", n),
        ]
    };
    let mut a = file("/m/a/01.flac", &track("Overture", "1"), Some(1000), "flac");
    let mut b = file("/m/a/02.flac", &track("Finale", "2"), Some(2000), "flac");
    a.tags.has_embedded_art = true;
    b.tags.has_embedded_art = true;
    let c = model::build(vec![a, b], vec!["/m".into()], 0);
    assert!(
        diagnose(&c, &crate::sources::Sources::default()).is_empty(),
        "got: {:?}",
        diagnose(&c, &crate::sources::Sources::default())
    );
}

#[test]
fn two_identical_tracks_are_not_silently_ignored() {
    // Two genuinely identical tracks must be reported — as tracks, or as
    // the album that holds them, but never passed over.
    let fields = vec![
        ("title", "Intro"),
        ("artist", "A"),
        ("album", "Album"),
        ("date", "1990"),
    ];
    let c = model::build(
        vec![
            file("/m/a/01.flac", &fields, Some(30_000), "flac"),
            file("/m/b/01.flac", &fields, Some(30_500), "flac"),
        ],
        vec!["/m".into()],
        0,
    );
    let issues = diagnose(&c, &crate::sources::Sources::default());
    let reported =
        count(&issues, IssueKind::DuplicateTrack) + count(&issues, IssueKind::DuplicateAlbum);
    assert_eq!(reported, 1, "reported once, one way or the other");
    assert!(
        issues
            .iter()
            .filter(|i| matches!(
                i.kind,
                IssueKind::DuplicateTrack | IssueKind::DuplicateAlbum
            ))
            .any(|i| i.files.len() == 2),
        "both copies are named"
    );
}

/// Builds a healthy one-file catalog to hang an imported analysis on.
fn catalog_of_one() -> Catalog {
    let fields = vec![
        ("title", "Intro"),
        ("artist", "A"),
        ("album", "Album"),
        ("date", "1990"),
    ];
    let mut f = file("/m/a/01.flac", &fields, Some(30_000), "flac");
    f.tags.has_embedded_art = true;
    model::build(vec![f], vec!["/m".into()], 0)
}

fn analysis_of(c: &Catalog) -> crate::analysis::FileAnalysis {
    let f = &c.files[0];
    crate::analysis::FileAnalysis {
        path: f.path.clone(),
        source: "flaccompagnon".into(),
        source_version: 1,
        size_bytes: f.size,
        modified_unix: f.mtime,
        ..Default::default()
    }
}

#[test]
fn a_failed_md5_is_an_error_even_when_the_checksums_passed() {
    // The two checks answer different questions: the frame CRCs prove the
    // container was not corrupted, the MD5 proves the audio is the audio
    // that was encoded. Passing the first and failing the second is not a
    // contradiction to arbitrate but a finding to report.
    let mut c = catalog_of_one();
    c.files[0].integrity = Some(IntegrityRecord {
        verdict: crate::audit::integrity::Verdict::Intact,
        method: crate::audit::integrity::FLAC_METHOD.into(),
        checked_at: 0,
    });
    c.analyses.push(crate::analysis::FileAnalysis {
        md5_state: Some("Mismatch".into()),
        ..analysis_of(&c)
    });

    let issues = diagnose(&c, &crate::sources::Sources::default());
    assert_eq!(count(&issues, IssueKind::Md5Mismatch), 1);
    let issue = issues
        .iter()
        .find(|i| i.kind == IssueKind::Md5Mismatch)
        .unwrap();
    assert_eq!(issue.kind.severity(), Severity::Error);
    assert!(
        issue.detail.contains("re-encoded"),
        "the disagreement is explained, not hidden: {}",
        issue.detail
    );
    assert!(
        issue.detail.contains("flaccompagnon"),
        "and attributed: {}",
        issue.detail
    );
}

#[test]
fn an_imported_verdict_dies_with_the_bytes_it_was_reached_on() {
    let mut c = catalog_of_one();
    let stale = crate::analysis::FileAnalysis {
        md5_state: Some("Mismatch".into()),
        transcoding: Some("detected".into()),
        size_bytes: c.files[0].size + 1,
        ..analysis_of(&c)
    };
    c.analyses.push(stale);
    let issues = diagnose(&c, &crate::sources::Sources::default());
    assert_eq!(count(&issues, IssueKind::Md5Mismatch), 0);
}

#[test]
fn a_spectral_verdict_is_stored_and_never_reported() {
    // The three detections another tool makes from the spectrum —
    // transcoding, upscaling, upsampling — are read, kept, and said
    // nowhere. They are heuristics whose own author hedges them ("could be
    // a naturally dark master"), and a report that restates a hedge as a
    // warning of its own has stopped describing the library and started
    // arguing about it. The measurements behind them stay on the file's
    // page, attributed, where the person who ran the tool can read them.
    let mut c = catalog_of_one();
    c.analyses.push(crate::analysis::FileAnalysis {
        transcoding: Some("detected".into()),
        upscaling: Some(true),
        upsampling: Some(true),
        detail: Some("cut at 16 kHz".into()),
        ..analysis_of(&c)
    });
    let issues = diagnose(&c, &crate::sources::Sources::default());
    assert!(
        issues.iter().all(|i| !i.detail.contains("cut at 16 kHz")),
        "no line may carry the verdict: {issues:#?}"
    );
    for word in ["transcod", "upscal", "upsampl", "lossy"] {
        assert!(
            issues.iter().all(|i| !i.detail.contains(word)),
            "\"{word}\" must not appear in the report: {issues:#?}"
        );
    }
    // And the record itself is untouched, so the day the tool is trusted
    // the display is all there is to write back.
    assert!(c.analyses[0].suspect_encoding(), "still stored, still true");
}

#[test]
fn an_analysis_of_a_file_the_catalog_does_not_hold_is_not_a_problem() {
    // It is waiting for its folder to be scanned, which is not a defect in
    // the library and must not be reported as one.
    let mut c = catalog_of_one();
    c.analyses.push(crate::analysis::FileAnalysis {
        path: "/elsewhere/never scanned.flac".into(),
        source: "flaccompagnon".into(),
        md5_state: Some("Mismatch".into()),
        transcoding: Some("detected".into()),
        ..Default::default()
    });
    assert!(
        diagnose(&c, &crate::sources::Sources::default()).is_empty(),
        "got: {:?}",
        diagnose(&c, &crate::sources::Sources::default())
    );
    assert_eq!(c.pending_analyses(), 1, "but it is counted as waiting");
}

#[test]
fn a_clean_analysis_adds_nothing_to_report() {
    let mut c = catalog_of_one();
    c.analyses.push(crate::analysis::FileAnalysis {
        md5_state: Some("Match".into()),
        transcoding: Some("none".into()),
        upscaling: Some(false),
        upsampling: Some(false),
        ..analysis_of(&c)
    });
    assert!(
        diagnose(&c, &crate::sources::Sources::default()).is_empty(),
        "got: {:?}",
        diagnose(&c, &crate::sources::Sources::default())
    );
}

#[test]
fn files_with_one_fingerprint_are_the_same_audio_whatever_their_tags_say() {
    // What a fingerprint buys beyond identifying a file, and worth more than
    // the identification for a library somebody has kept for years: the
    // tag-based duplicate check compares artist, title and duration, so it
    // only finds copies whose *tags* already agree. Two rips of one track
    // filed under different names are invisible to it.
    let mut catalog = model::build(
        vec![
            file(
                "/m/Miles/01.flac",
                &[
                    ("title", "So What"),
                    ("artist", "Miles Davis"),
                    ("album", "Kind of Blue"),
                ],
                Some(545_000),
                "flac",
            ),
            file(
                "/m/Rip/track03.flac",
                &[("title", "Track 03"), ("album", "Unknown")],
                Some(545_000),
                "flac",
            ),
        ],
        vec!["/m".into()],
        0,
    );
    let print = crate::fingerprint::Fingerprint {
        data: "AQAAcxUmUaEk".to_string(),
        seconds: 545,
    };
    catalog.files[0].fingerprint = Some(print.clone());
    catalog.files[1].fingerprint = Some(print);

    let issues = diagnose(&catalog, &crate::sources::Sources::default());
    let same: Vec<&Issue> = issues
        .iter()
        .filter(|i| i.kind == IssueKind::SameAudio)
        .collect();
    assert_eq!(same.len(), 1, "one report for the pair, not one each");
    assert_eq!(same[0].files.len(), 2);
    assert!(
        same[0].detail.contains("2 files are the same recording"),
        "{}",
        same[0].detail
    );
    // Not "likely". The word is what separates this from the guess, and a
    // reader deciding whether to delete a file needs to know which they have.
    assert_eq!(IssueKind::SameAudio.label(), "the same audio");
}

#[test]
fn a_library_nobody_has_fingerprinted_reports_no_identical_audio() {
    // A silence, not a clean bill of health — and it must not be reported as
    // one. Only files that have been fingerprinted take part.
    let catalog = model::build(
        vec![file(
            "/m/Miles/01.flac",
            &[("title", "So What"), ("artist", "Miles Davis")],
            Some(545_000),
            "flac",
        )],
        vec!["/m".into()],
        0,
    );
    let issues = diagnose(&catalog, &crate::sources::Sources::default());
    assert!(!issues.iter().any(|i| i.kind == IssueKind::SameAudio));
}
