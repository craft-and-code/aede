//! Typed HTTP administration reuses the ordinary CLI parser and store lock.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use aede_server::{JobOutput, JobRequest};

use super::super::secret_output::{redact, secret_variants};

#[path = "server_job_decode.rs"]
mod decode;
#[path = "server_job_process.rs"]
mod process;

const MAX_OUTPUT: usize = 64 * 1024;

fn arguments(request: &JobRequest) -> Vec<String> {
    let mut args = Vec::new();
    let targets: &[String] = match request {
        JobRequest::Generate(plan) => {
            match plan.steps.first() {
                Some(aede_server::GenerationStep::Analyze) => args.extend(
                    [
                        "analyze",
                        "--threads=1",
                        "--json-layout=album",
                        "--show-results",
                    ]
                    .map(String::from),
                ),
                Some(aede_server::GenerationStep::Spectrum) => {
                    args.extend(["spectrum", "--threads=1"].map(String::from))
                }
                Some(aede_server::GenerationStep::M3u) => {
                    args.extend(["playlist"].map(String::from))
                }
                Some(aede_server::GenerationStep::Lyrics) => {
                    args.extend(["fetch", "--lyrics", "--yes"].map(String::from))
                }
                None => {} // run_one refuses an unresolved or empty plan before launching.
            }
            &plan.roots
        }
        JobRequest::Check(check) => {
            args.extend(["check", "--decode-test", "--json"].map(String::from));
            &check.files
        }
        JobRequest::Scan(scan) => {
            args.push("scan".into());
            for (enabled, flag) in [
                (scan.full, "full"),
                (scan.replace, "replace"),
                (scan.follow_symlinks, "follow-symlinks"),
                (scan.include_hidden, "include-hidden"),
            ] {
                if enabled {
                    args.push(format!("--{flag}"));
                }
            }
            if let Some(threads) = scan.threads {
                args.push(format!("--threads={threads}"));
            }
            &scan.folders
        }
        JobRequest::Analyze(media) | JobRequest::Spectrum(media) => {
            let spectrum = matches!(request, JobRequest::Spectrum(_));
            args.push(if spectrum { "spectrum" } else { "analyze" }.into());
            if media.force {
                args.push(if spectrum { "--full" } else { "--force" }.into());
            }
            if !spectrum {
                args.push("--json-layout=album".into());
                args.push("--show-results".into());
            }
            if let Some(threads) = media.threads {
                args.push(format!("--threads={threads}"));
            }
            &media.targets
        }
        JobRequest::Fetch(fetch) => {
            args.push("fetch".into());
            for (enabled, flag) in [
                (fetch.summaries, "summaries"),
                (fetch.discography, "discography"),
                (fetch.covers, "covers"),
                (fetch.lyrics, "lyrics"),
                (fetch.identify, "identify"),
                (fetch.credits, "credits"),
                (fetch.recordings, "recordings"),
                (fetch.portraits, "portraits"),
                (fetch.logos, "logos"),
                (fetch.labels, "labels"),
                (fetch.discogs, "discogs"),
                (fetch.fanart, "fanart"),
                (fetch.banners, "banners"),
                (fetch.images, "images"),
                (fetch.full, "full"),
                (fetch.dry_run, "dry-run"),
                (fetch.yes, "yes"),
                (fetch.no_logo, "no-logo"),
                (fetch.no_label_logo, "no-label-logo"),
                (fetch.no_portrait, "no-portrait"),
                (fetch.no_background, "no-background"),
                (fetch.no_banner, "no-banner"),
                (fetch.no_album_cover, "no-album-cover"),
                (fetch.no_cdart, "no-cdart"),
            ] {
                if enabled {
                    args.push(format!("--{flag}"));
                }
            }
            for (name, value) in [("size", &fetch.size), ("lang", &fetch.lang)] {
                if let Some(value) = value {
                    args.push(format!("--{name}={value}"));
                }
            }
            &fetch.targets
        }
    };
    // Every value remains one OS argument. The separator prevents a target
    // such as "--data=/elsewhere" from becoming a command option.
    args.push("--".into());
    args.extend(targets.iter().cloned());
    args
}

pub(super) fn run(
    data_dir: &Path,
    request: JobRequest,
    cancellation: Arc<AtomicBool>,
) -> Result<JobOutput, String> {
    run_sequence(request, &cancellation, |step| {
        run_one(data_dir, step, cancellation.clone())
    })
}

/// Keep identification and enrichment inside the same cancellable server-owned task.
fn run_sequence(
    request: JobRequest,
    cancellation: &AtomicBool,
    mut execute_step: impl FnMut(JobRequest) -> Result<JobOutput, String>,
) -> Result<JobOutput, String> {
    if let JobRequest::Generate(plan) = &request {
        let unique: std::collections::BTreeSet<_> = plan.steps.iter().collect();
        if plan.roots.is_empty()
            || plan.steps.is_empty()
            || plan.steps.len() > 4
            || unique.len() != plan.steps.len()
        {
            return Err("generation needs one to four distinct stages".into());
        }
    }
    let generation = matches!(&request, JobRequest::Generate(_));
    let mut steps = Vec::new();
    if let JobRequest::Fetch(fetch) = &request
        && fetch.musicbrainz
        && [
            fetch.summaries,
            fetch.discography,
            fetch.covers,
            fetch.lyrics,
            fetch.identify,
            fetch.credits,
            fetch.recordings,
            fetch.portraits,
            fetch.logos,
            fetch.labels,
            fetch.discogs,
            fetch.fanart,
        ]
        .into_iter()
        .any(|selected| selected)
    {
        steps.push(JobRequest::Fetch(aede_server::FetchRequest {
            service_keys: fetch.service_keys.clone(),
            targets: fetch.targets.clone(),
            full: fetch.full,
            dry_run: fetch.dry_run,
            yes: fetch.yes,
            ..Default::default()
        }));
    }
    if let JobRequest::Generate(plan) = request {
        for step in plan.steps {
            steps.push(JobRequest::Generate(aede_server::GenerationRequest {
                steps: vec![step],
                roots: plan.roots.clone(),
            }));
        }
    } else {
        steps.push(request);
    }
    let mut output = JobOutput::default();
    for step in steps {
        if cancellation.load(Ordering::Acquire) {
            if generation {
                output.exit_code = 130;
                return Ok(output);
            }
            return Err("administrative task cancelled before the next enrichment stage".into());
        }
        let next = execute_step(step)?;
        if next.exit_code != 0 || !generation {
            output.exit_code = next.exit_code;
        }
        output.output_truncated |= next.output_truncated;
        for (held, addition) in [
            (&mut output.stdout, next.stdout),
            (&mut output.stderr, next.stderr),
        ] {
            let available = MAX_OUTPUT.saturating_sub(held.len());
            let mut end = addition.len().min(available);
            while !addition.is_char_boundary(end) {
                end -= 1;
            }
            held.push_str(&addition[..end]);
            output.output_truncated |= end < addition.len();
        }
        if output.exit_code != 0 && !generation {
            break;
        }
    }
    Ok(output)
}

fn run_one(
    data_dir: &Path,
    request: JobRequest,
    cancellation: Arc<AtomicBool>,
) -> Result<JobOutput, String> {
    if let JobRequest::Generate(plan) = &request
        && (plan.steps.len() != 1 || plan.roots.is_empty())
    {
        return Err("generation plan must be resolved into one stage before launching".into());
    }

    let data_dir = std::fs::canonicalize(data_dir).map_err(|error| error.to_string())?;
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut command = Command::new(exe);
    command
        .env_remove("AEDE_DESKTOP_LEASE")
        .env_remove("AEDE_UNAVAILABLE_ROOTS");
    if let JobRequest::Scan(scan) = &request {
        let value = aede_core::json::Json::Arr(
            scan.unavailable_folders
                .iter()
                .map(|folder| aede_core::json::Json::Str(folder.clone()))
                .collect(),
        );
        command.env("AEDE_UNAVAILABLE_ROOTS", value.to_string_compact());
    }

    command
        .args(arguments(&request))
        .env("AEDE_DELEGATED_CHILD", "1")
        .env("AEDE_DELEGATED_DATA_DIR", data_dir)
        .env("NO_COLOR", "1")
        .env_remove("AEDE_ADMIN_TOKEN")
        .env_remove("AEDE_DELEGATED_STDIN_TTY")
        .env_remove("AEDE_DELEGATED_STDOUT_TTY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut secrets: Vec<String> = ["AEDE_ACOUSTID_KEY", "AEDE_FANARTTV_KEY", "AEDE_ADMIN_TOKEN"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .filter(|value| !value.is_empty())
        .collect();
    secrets.extend(service_environment(&mut command, &request));
    let check = match request {
        JobRequest::Check(check) => Some(check),
        _ => None,
    };
    execute_with_progress(command, cancellation, &secrets, check)
}

/// Set only the two supported service variables for this child, preserving the parent environment.
fn service_environment(command: &mut Command, request: &JobRequest) -> Vec<String> {
    let mut secrets = Vec::new();
    if let JobRequest::Fetch(fetch) = request
        && let Some(keys) = &fetch.service_keys
    {
        for (name, key) in [
            ("AEDE_FANARTTV_KEY", &keys.fanarttv),
            ("AEDE_ACOUSTID_KEY", &keys.acoustid),
        ] {
            if let Some(secret) = key {
                command.env(name, secret);
                secrets.push(secret.clone());
            }
        }
    }
    secrets
}

#[cfg(test)]
fn execute(
    command: Command,
    cancellation: Arc<AtomicBool>,
    secrets: &[String],
) -> Result<JobOutput, String> {
    execute_with_progress(command, cancellation, secrets, None)
}

fn execute_with_progress(
    mut command: Command,
    cancellation: Arc<AtomicBool>,
    secrets: &[String],
    check: Option<aede_server::CheckRequest>,
) -> Result<JobOutput, String> {
    if cancellation.load(Ordering::Acquire) {
        return Ok(JobOutput {
            exit_code: 130,
            ..Default::default()
        });
    }
    process::isolate(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start administrative command: {error}"))?;
    let stdout = child.stdout.take().ok_or("missing command stdout")?;
    let stderr = child.stderr.take().ok_or("missing command stderr")?;
    let progress_secrets = secrets.to_vec();
    let out = std::thread::spawn(move || {
        let mut snapshots = decode::Snapshots::new(check, progress_secrets);
        capture_observed(stdout, |bytes, truncated| {
            snapshots.observe(bytes, truncated)
        })
    });
    let err = std::thread::spawn(move || capture(stderr));
    let mut cancelled = false;
    let status = loop {
        // A command may exit before its descendants close inherited pipes.
        // Keep its PID reserved until capture finishes so cancellation can
        // still signal the owned process group, never a recycled leader PID.
        if out.is_finished() && err.is_finished() {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(format!("cannot wait for administrative command: {error}"));
                }
            }
        }
        if cancellation.load(Ordering::Acquire) && process::terminate(&mut child).is_ok() {
            cancelled = true;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = out.join().map_err(|_| "command output reader failed")??;
    let stderr = err.join().map_err(|_| "command error reader failed")??;
    let status = status?;
    let (stdout, stdout_truncated) = redact_capture(stdout, secrets);
    let (stderr, stderr_truncated) = redact_capture(stderr, secrets);
    Ok(JobOutput {
        exit_code: if cancelled {
            130
        } else {
            status.code().unwrap_or(1)
        },
        stdout,
        stderr,
        output_truncated: stdout_truncated || stderr_truncated,
    })
}

fn capture(input: impl Read) -> Result<(Vec<u8>, bool), String> {
    capture_observed(input, |_, _| {})
}
fn capture_observed(
    mut input: impl Read,
    mut observe: impl FnMut(&[u8], bool),
) -> Result<(Vec<u8>, bool), String> {
    let mut captured = Vec::new();
    let mut truncated = false;
    let mut buffer = [0; 8192];
    loop {
        match input.read(&mut buffer) {
            Ok(0) => return Ok((captured, truncated)),
            Ok(size) => {
                let keep = size.min(MAX_OUTPUT.saturating_sub(captured.len()));
                captured.extend_from_slice(&buffer[..keep]);
                truncated |= keep < size;
                observe(&captured, truncated);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("cannot read command output: {error}")),
        }
    }
}

fn redact_capture((bytes, truncated): (Vec<u8>, bool), secrets: &[String]) -> (String, bool) {
    // Match bytes before UTF-8 decoding: a credential can be cut in the middle
    // of a character or a percent escape, not only between whole characters.
    let suffix = if truncated {
        secret_variants(secrets)
            .iter()
            .filter_map(|secret| {
                (1..=secret.len().min(bytes.len()))
                    .rev()
                    .find(|&length| bytes.ends_with(&secret.as_bytes()[..length]))
            })
            .max()
            .unwrap_or(0)
    } else {
        0
    };
    let mut text = redact(
        String::from_utf8_lossy(&bytes[..bytes.len() - suffix]).into_owned(),
        secrets,
    );
    let marker = if suffix == 0 { "" } else { "[redacted]" };
    let limit = MAX_OUTPUT - marker.len();
    let shortened = text.len() > limit;
    if shortened {
        let mut boundary = limit;
        while !text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        text.truncate(boundary);
    }
    // Reserve the marker's bytes so redaction cannot exceed the capture bound
    // or leave an ambiguous, partially truncated redaction marker itself.
    text.push_str(marker);
    (text, truncated || shortened)
}

#[cfg(test)]
#[path = "server_jobs_tests.rs"]
mod tests;
