//! Observe complete JSON row lines while preserving the ordinary final JSON capture.
use super::{JobOutput, MAX_OUTPUT, redact};
use aede_core::json::{self, Json};
pub(super) struct Snapshots {
    check: Option<aede_server::CheckRequest>,
    secrets: Vec<String>,
    offset: usize,
    started: bool,
    files: Vec<Json>,
}
impl Snapshots {
    pub(super) fn new(check: Option<aede_server::CheckRequest>, secrets: Vec<String>) -> Self {
        Self {
            check,
            secrets,
            offset: 0,
            started: false,
            files: Vec::new(),
        }
    }
    pub(super) fn observe(&mut self, bytes: &[u8], truncated: bool) {
        if truncated {
            self.check = None;
        }
        let Some(check) = &self.check else {
            return;
        };
        while let Some(end) = bytes[self.offset..].iter().position(|byte| *byte == b'\n') {
            let line = &bytes[self.offset..self.offset + end];
            self.offset += end + 1;
            if !self.started {
                self.started = line == b"{\"version\":1,\"files\":[";
                continue;
            }
            let Ok(line) = std::str::from_utf8(line) else {
                continue;
            };
            if line == "]}" {
                continue;
            }
            let Ok(file) = json::parse(line.trim_start_matches(',')) else {
                continue;
            };
            if file.get("path").and_then(Json::as_str).is_none() {
                continue;
            }
            if self.files.len() >= check.files.len() {
                return;
            }
            self.files.push(file);
            let output = self.output();
            if output.stdout.len() > MAX_OUTPUT {
                return;
            }
            check.report(output);
        }
    }
    fn output(&self) -> JobOutput {
        let mut value = Json::obj();
        value.set("version", Json::Num(1.0));
        value.set("files", Json::Arr(self.files.clone()));
        let failed = self
            .files
            .iter()
            .any(|file| file.get("status").and_then(Json::as_str) == Some("ko"));
        JobOutput {
            stdout: redact(value.to_string_compact(), &self.secrets),
            exit_code: i32::from(failed),
            ..Default::default()
        }
    }
}
#[cfg(test)]
#[path = "server_job_decode_tests.rs"]
mod tests;
