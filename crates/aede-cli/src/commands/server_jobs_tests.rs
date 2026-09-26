use super::*;
use crate::args::Args;
use aede_server::{FetchRequest, ScanRequest};

#[test]
fn every_fetch_option_reaches_the_existing_parser_without_option_injection() {
    let job = JobRequest::Fetch(FetchRequest {
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
fn credentials_are_redacted_in_raw_trimmed_and_encoded_service_errors() {
    let text = "  aB/c+  ; aB/c+; client=aB%2Fc%2B; client=aB%2fc%2b";
    let redacted = redact(text.into(), &["  aB/c+  ".into()]);
    assert_eq!(
        redacted,
        "[redacted]; [redacted]; client=[redacted]; client=[redacted]"
    );
}

#[cfg(unix)]
#[test]
fn credentials_cut_by_the_capture_limit_are_redacted_in_both_streams() {
    let secrets = ["  aB/c+SECRET  ".into(), "éSecret".into()];
    for (variant, kept) in [
        ("aB/c+SECRET", 4),
        ("  aB/c+SECRET  ", 6),
        ("aB%2Fc%2BSECRET", 7),
        ("aB%2fc%2bSECRET", 7),
        ("éSecret", 1),
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
