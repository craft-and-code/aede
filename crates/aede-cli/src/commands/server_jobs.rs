//! Typed HTTP administration reuses the ordinary CLI parser and store lock.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use aede_server::{JobOutput, JobRequest};

const MAX_OUTPUT: usize = 64 * 1024;

fn arguments(request: &JobRequest) -> Vec<String> {
    let mut args = Vec::new();
    let targets = match request {
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
    execute(command, cancellation, &secrets)
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

fn execute(
    mut command: Command,
    cancellation: Arc<AtomicBool>,
    secrets: &[String],
) -> Result<JobOutput, String> {
    if cancellation.load(Ordering::Acquire) {
        return Ok(JobOutput {
            exit_code: 130,
            ..Default::default()
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start administrative command: {error}"))?;
    let stdout = child.stdout.take().ok_or("missing command stdout")?;
    let stderr = child.stderr.take().ok_or("missing command stderr")?;
    let out = std::thread::spawn(move || capture(stdout));
    let err = std::thread::spawn(move || capture(stderr));
    let mut cancelled = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("cannot wait for administrative command: {error}"));
            }
        }
        if cancellation.load(Ordering::Acquire) && child.kill().is_ok() {
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

fn capture(mut input: impl Read) -> Result<(Vec<u8>, bool), String> {
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

fn redact(mut text: String, secrets: &[String]) -> String {
    for secret in secret_variants(secrets) {
        text = text.replace(&secret, "[redacted]");
    }
    text
}

fn secret_variants(secrets: &[String]) -> Vec<String> {
    let mut variants = Vec::new();
    for secret in secrets {
        for value in [secret.as_str(), secret.trim()] {
            if value.is_empty() {
                continue;
            }
            variants.push(value.to_string());
            // Service keys can also appear in an error's query string. The
            // source adapters trim configuration and percent-encode AcoustID keys.
            let mut encoded = String::new();
            let mut lower_encoded = String::new();
            for byte in value.bytes() {
                if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                    encoded.push(char::from(byte));
                    lower_encoded.push(char::from(byte));
                } else {
                    encoded.push_str(&format!("%{byte:02X}"));
                    lower_encoded.push_str(&format!("%{byte:02x}"));
                }
            }
            variants.push(encoded);
            variants.push(lower_encoded);
        }
    }
    variants
}

#[cfg(test)]
#[path = "server_jobs_tests.rs"]
mod tests;
