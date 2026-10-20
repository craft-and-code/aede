use super::*;
use crate::args::Args;
use aede_server::{FetchRequest, ScanRequest};

#[test]
fn enrichment_identifies_before_secondary_passes_and_keeps_one_task_result() {
    let request = JobRequest::Fetch(FetchRequest {
        musicbrainz: true,
        targets: vec!["Black Sabbath".into(), "--data=/unwanted".into()],
        covers: true,
        fanart: true,
        yes: true,
        ..Default::default()
    });
    let mut requests = Vec::new();
    let output = run_sequence(request, &AtomicBool::new(false), |request| {
        requests.push(arguments(&request));
        Ok(JobOutput {
            stdout: format!("stage {}\n", requests.len()),
            ..Default::default()
        })
    })
    .unwrap();
    assert_eq!(requests.len(), 2);
    let base = Args::parse(requests[0].clone());
    assert!(base.has("yes"));
    assert!(!base.has("covers"));
    assert!(!base.has("fanart"));
    assert_eq!(base.positionals, ["Black Sabbath", "--data=/unwanted"]);
    assert!(!base.has("data"));
    let artwork = Args::parse(requests[1].clone());
    assert!(artwork.has("covers") && artwork.has("fanart"));
    assert_eq!(output.stdout, "stage 1\nstage 2\n");
}

#[test]
fn failed_or_cancelled_identification_does_not_start_secondary_work() {
    for cancel in [false, true] {
        let cancellation = AtomicBool::new(false);
        let mut calls = 0;
        let result = run_sequence(
            JobRequest::Fetch(FetchRequest {
                musicbrainz: true,
                covers: true,
                ..Default::default()
            }),
            &cancellation,
            |_| {
                calls += 1;
                if cancel {
                    cancellation.store(true, Ordering::Release);
                }
                Ok(JobOutput {
                    exit_code: if cancel { 0 } else { 1 },
                    stderr: "diagnostic".into(),
                    ..Default::default()
                })
            },
        );
        assert_eq!(calls, 1);
        if cancel {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap().stderr, "diagnostic");
        }
    }
}

#[test]
fn every_fetch_option_reaches_the_existing_parser_without_option_injection() {
    let job = JobRequest::Fetch(FetchRequest {
        service_keys: None,
        artist: None,
        musicbrainz: false,
        targets: vec![
            "Miles Davis".into(),
            "--data=/unwanted".into(),
            "/music/Album".into(),
        ],
        summaries: true,
        discography: true,
        covers: true,
        lyrics: true,
        identify: true,
        credits: true,
        recordings: true,
        portraits: true,
        logos: true,
        labels: true,
        discogs: true,
        fanart: true,
        banners: true,
        images: true,
        full: true,
        dry_run: true,
        yes: true,
        no_logo: true,
        no_label_logo: true,
        no_portrait: true,
        no_background: true,
        no_banner: true,
        no_album_cover: true,
        no_cdart: true,
        size: Some("1200".into()),
        lang: Some("fr,en".into()),
    });
    let args = Args::parse(arguments(&job));
    assert_eq!(args.command, "fetch");
    for option in [
        "summaries",
        "discography",
        "covers",
        "lyrics",
        "identify",
        "credits",
        "recordings",
        "portraits",
        "logos",
        "labels",
        "discogs",
        "fanart",
        "banners",
        "images",
        "full",
        "dry-run",
        "yes",
        "no-logo",
        "no-label-logo",
        "no-portrait",
        "no-background",
        "no-banner",
        "no-album-cover",
        "no-cdart",
    ] {
        assert!(args.has(option), "missing {option}");
    }
    assert_eq!(args.value("size"), Some("1200"));
    assert_eq!(args.value("lang"), Some("fr,en"));
    assert!(!args.has("data"));
    assert_eq!(
        args.positionals,
        ["Miles Davis", "--data=/unwanted", "/music/Album"]
    );
}

#[test]
fn scan_parameters_preserve_cli_roots_and_read_options() {
    let args = Args::parse(arguments(&JobRequest::Scan(ScanRequest {
        folders: vec!["/music/New Shelf".into()],
        unavailable_folders: Vec::new(),
        replace: true,
        full: true,
        threads: Some(2),
        include_hidden: true,
        follow_symlinks: true,
    })));
    assert_eq!(args.command, "scan");
    assert_eq!(args.positionals, ["/music/New Shelf"]);
    assert_eq!(args.value("threads"), Some("2"));
    for flag in ["replace", "full", "include-hidden", "follow-symlinks"] {
        assert!(args.has(flag));
    }
    let defaults = Args::parse(arguments(&JobRequest::Scan(ScanRequest::default())));
    assert!(defaults.positionals.is_empty());
    assert!(!defaults.has("replace"));
}

#[test]
fn captured_output_is_bounded_but_all_child_bytes_are_drained() {
    let mut bytes = std::io::Cursor::new(vec![b'x'; MAX_OUTPUT * 3]);
    let (output, truncated) = capture(&mut bytes).unwrap();
    assert_eq!(output.len(), MAX_OUTPUT);
    assert!(truncated);
    assert_eq!(bytes.position(), (MAX_OUTPUT * 3) as u64);
    assert_eq!(
        redact(
            "key=secret, token=private".into(),
            &["secret".into(), "private".into()]
        ),
        "key=[redacted], token=[redacted]"
    );
}

#[test]
fn encoded_overlapping_credentials_stay_masked_within_the_capture_bound() {
    let secrets = ["fictional/a".into(), "fictional/a+private-tail".into()];
    for variant in [
        "fictional/a+private-tail",
        "fictional%2Fa%2Bprivate-tail",
        "fictional%2fa%2bprivate-tail",
    ] {
        let bytes = format!("{}{}", "x".repeat(MAX_OUTPUT - variant.len()), variant).into_bytes();
        let (text, truncated) = redact_capture((bytes, false), &secrets);
        assert!(text.len() <= MAX_OUTPUT);
        assert!(!text.contains("private-tail"));
        assert!(text.ends_with("[redacted]"));
        assert!(!truncated);
    }
}

#[cfg(unix)]
#[test]
fn credentials_cut_by_the_capture_limit_are_redacted_in_both_streams() {
    let secrets = [
        "  aB/c+SECRET  ".into(),
        "éSecret".into(),
        "quoted\"private\\tail".into(),
    ];
    for (variant, kept) in [
        ("aB/c+SECRET", 4),
        ("  aB/c+SECRET  ", 6),
        ("aB%2Fc%2BSECRET", 7),
        ("aB%2fc%2bSECRET", 7),
        ("éSecret", 1),
        ("quoted\\\"private\\\\tail", 7),
        ("quoted\\\"private\\\\tail", 16),
    ] {
        let mut command = Command::new("sh");
        command.args(["-c", "printf '%*s' \"$1\" ''; printf '%s' \"$2\"; printf '%*s' \"$1\" '' >&2; printf '%s' \"$2\" >&2", "redaction-fixture"])
            .arg((MAX_OUTPUT - kept).to_string()).arg(variant)
            .stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
        let output = execute(command, Arc::new(AtomicBool::new(false)), &secrets).unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.output_truncated);
        for text in [&output.stdout, &output.stderr] {
            assert!(text.len() <= MAX_OUTPUT);
            assert!(
                text.ends_with("[redacted]"),
                "truncated credential variant {variant} was not masked"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn cancelled_execution_returns_130_and_preserves_prior_output() {
    let mut command = Command::new("sh");
    command
        .args(["-c", "printf 'saved result'; while :; do :; done"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let cancellation = Arc::new(AtomicBool::new(false));
    let signal = cancellation.clone();
    let trigger = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        signal.store(true, Ordering::Release);
    });
    let output = execute(command, cancellation, &[]).unwrap();
    trigger.join().unwrap();
    assert_eq!(output.exit_code, 130);
    assert_eq!(output.stdout, "saved result");
}

#[test]
fn media_jobs_keep_catalog_folder_in_one_literal_argument() {
    let media = aede_server::MediaJobRequest {
        targets: vec!["/music/album with spaces".into()],
        force: true,
        threads: Some(2),
    };
    let analysis = arguments(&JobRequest::Analyze(media.clone()));
    assert_eq!(
        analysis,
        vec![
            "analyze",
            "--force",
            "--json-layout=album",
            "--show-results",
            "--threads=2",
            "--",
            "/music/album with spaces"
        ]
    );
    let spectrum = arguments(&JobRequest::Spectrum(media));
    assert_eq!(
        spectrum,
        vec![
            "spectrum",
            "--full",
            "--threads=2",
            "--",
            "/music/album with spaces"
        ]
    );
}

#[cfg(unix)]
#[test]
fn client_service_keys_are_child_only_and_redacted_from_captured_output() {
    let job = JobRequest::Fetch(FetchRequest {
        service_keys: Some(aede_server::FetchServiceKeys {
            fanarttv: Some("client-fictional-key".into()),
            acoustid: None,
        }),
        dry_run: true,
        ..Default::default()
    });
    assert!(
        !arguments(&job)
            .iter()
            .any(|arg| arg.contains("client-fictional-key"))
    );
    assert!(!format!("{job:?}").contains("client-fictional-key"));
    let mut command = Command::new("sh");
    command
        .args(["-c", "printf '%s' \"$AEDE_FANARTTV_KEY\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let before = std::env::var("AEDE_FANARTTV_KEY").ok();
    let secrets = service_environment(&mut command, &job);
    let output = execute(command, Arc::new(AtomicBool::new(false)), &secrets).unwrap();
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout, "[redacted]");
    assert_eq!(std::env::var("AEDE_FANARTTV_KEY").ok(), before);
}

#[test]
fn combined_stage_output_stays_bounded_at_utf8_boundaries_and_plain_fetch_runs_once() {
    let mut calls = 0;
    let output = run_sequence(
        JobRequest::Fetch(FetchRequest {
            musicbrainz: true,
            covers: true,
            ..Default::default()
        }),
        &AtomicBool::new(false),
        |_| {
            calls += 1;
            Ok(JobOutput {
                stdout: if calls == 1 {
                    "x".repeat(MAX_OUTPUT - 1)
                } else {
                    "é".repeat(10)
                },
                ..Default::default()
            })
        },
    )
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(output.stdout.len(), MAX_OUTPUT - 1);
    assert!(output.output_truncated);
    calls = 0;
    run_sequence(
        JobRequest::Fetch(FetchRequest {
            musicbrainz: true,
            ..Default::default()
        }),
        &AtomicBool::new(false),
        |_| {
            calls += 1;
            Ok(JobOutput::default())
        },
    )
    .unwrap();
    assert_eq!(calls, 1);
}

#[test]
fn generation_keeps_order_and_bounded_workers_and_reports_partial_stage_failure() {
    use aede_server::{GenerationRequest, GenerationStep};
    let stages = [
        GenerationStep::Analyze,
        GenerationStep::Spectrum,
        GenerationStep::M3u,
        GenerationStep::Lyrics,
    ];
    let mut calls = Vec::new();
    run_sequence(
        JobRequest::Generate(GenerationRequest {
            steps: stages.to_vec(),
            roots: vec!["/music".into()],
        }),
        &AtomicBool::new(false),
        |request| {
            calls.push(arguments(&request));
            Ok(JobOutput::default())
        },
    )
    .unwrap();
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls
            .iter()
            .map(|args| args[0].as_str())
            .collect::<Vec<_>>(),
        ["analyze", "spectrum", "playlist", "fetch"]
    );
    assert!(calls[0].contains(&"--threads=1".into()) && calls[1].contains(&"--threads=1".into()));
    assert!(!calls[2].contains(&"--artists".into()));
    assert_eq!(Args::parse(calls[2].clone()).positionals, ["/music"]);
    assert!(calls[3].contains(&"--lyrics".into()) && calls[3].contains(&"--yes".into()));
    let mut visited = 0;
    let output = run_sequence(
        JobRequest::Generate(GenerationRequest {
            steps: stages.to_vec(),
            roots: vec!["/music".into()],
        }),
        &AtomicBool::new(false),
        |_| {
            visited += 1;
            Ok(JobOutput {
                exit_code: 1,
                stderr: "failed stage".into(),
                ..Default::default()
            })
        },
    )
    .unwrap();
    assert_eq!(visited, 4);
    assert_eq!(output.exit_code, 1);
}
#[test]
fn cancelling_a_generation_never_launches_the_next_stage() {
    use aede_server::{GenerationRequest, GenerationStep};
    let cancellation = AtomicBool::new(false);
    let mut visited = 0;
    let result = run_sequence(
        JobRequest::Generate(GenerationRequest {
            steps: vec![GenerationStep::M3u, GenerationStep::Lyrics],
            roots: vec!["/music".into()],
        }),
        &cancellation,
        |_| {
            visited += 1;
            cancellation.store(true, Ordering::Release);
            Ok(JobOutput::default())
        },
    );
    assert_eq!(result.unwrap().exit_code, 130);
    assert_eq!(visited, 1);
}

#[cfg(unix)]
#[test]
fn cancellation_releases_nested_worker_pipes_before_another_stage_can_start() {
    use std::sync::mpsc;
    let directory = std::env::temp_dir().join(format!("aede_job_tree_{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let pid_file = directory.join("worker.pid");
    let mut command = Command::new("sh");
    command
        .args(["-c", "sleep 30 & echo $! > \"$1\"; wait", "aede-test"])
        .arg(&pid_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let cancellation = Arc::new(AtomicBool::new(false));
    let signal = cancellation.clone();
    let (send, receive) = mpsc::channel();
    let runner = std::thread::spawn(move || {
        let _ = send.send(execute(command, signal, &[]));
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !pid_file.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let pid: u32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    cancellation.store(true, Ordering::Release);
    let outcome = receive.recv_timeout(Duration::from_secs(2));
    // Clean only this fixture's recorded descendant even on the regression baseline.
    if outcome.is_err() {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    runner.join().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    assert_eq!(
        outcome
            .expect("nested worker must not keep the task alive")
            .unwrap()
            .exit_code,
        130
    );
}

#[test]
fn album_decode_arguments_preserve_literal_files_without_folder_expansion() {
    let path = std::env::temp_dir()
        .join("an album/track;literal.flac")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        arguments(&JobRequest::Check(vec![path.clone()].into())),
        ["check", "--decode-test", "--json", "--", path.as_str()]
    );
}

#[cfg(unix)]
#[test]
fn cancellation_still_stops_descendants_after_the_command_leader_exits() {
    use std::sync::mpsc;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("aede_job_orphan_{}_{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let pid_file = directory.join("owned-pids");
    let mut command = Command::new("sh");
    command
        .args([
            "-c",
            "sleep 30 & printf '%s %s\\n' \"$$\" \"$!\" > \"$1\"; exit 0",
            "aede-test",
        ])
        .arg(&pid_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let cancellation = Arc::new(AtomicBool::new(false));
    let signal = cancellation.clone();
    let (send, receive) = mpsc::channel();
    let runner = std::thread::spawn(move || {
        let _ = send.send(execute(command, signal, &[]));
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let pids = loop {
        if let Ok(text) = std::fs::read_to_string(&pid_file) {
            let pids: Vec<_> = text
                .split_whitespace()
                .filter_map(|part| part.parse::<u32>().ok())
                .collect();
            if pids.len() == 2 {
                break Some(pids);
            }
        }
        if std::time::Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut leader_exited = false;
    if let Some(pids) = &pids {
        while std::time::Instant::now() < deadline {
            if let Ok(output) = Command::new("ps")
                .args(["-o", "stat=", "-p", &pids[0].to_string()])
                .output()
            {
                let state = String::from_utf8_lossy(&output.stdout);
                // A finished leader is either already reaped or held as a zombie.
                if state.trim().is_empty() || state.trim().starts_with('Z') {
                    leader_exited = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    cancellation.store(true, Ordering::Release);
    let outcome = receive.recv_timeout(Duration::from_secs(2));
    if outcome.is_err()
        && let Some(pids) = &pids
    {
        // Only the descendant launched by this fixture may be cleaned up.
        let _ = Command::new("/bin/kill")
            .args(["-KILL", &pids[1].to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    runner.join().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        leader_exited,
        "the fixture leader must exit before cancellation"
    );
    assert_eq!(
        outcome
            .expect("a descendant must not retain task pipes after cancellation")
            .unwrap()
            .exit_code,
        130
    );
}

#[test]
fn exact_artist_summary_jobs_use_a_value_option_without_positional_name_expansion() {
    let job = JobRequest::Fetch(FetchRequest {
        artist: Some("artist:example: --data=elsewhere".into()),
        summaries: true,
        yes: true,
        ..Default::default()
    });
    let parsed = Args::parse(arguments(&job));
    assert_eq!(
        parsed.value("artist-ref"),
        Some("artist:example: --data=elsewhere")
    );
    assert!(parsed.positionals.is_empty());
}
