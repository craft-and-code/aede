//! Private, bounded per-file snapshots for one admitted album check.
use super::{ApiState, JobOutput, MAX_OUTPUT, Status};
use std::sync::Arc;

/// Exact admitted files with an optional process-local progress sink; never supplied over HTTP.
#[derive(Clone)]
pub struct CheckRequest {
    /// Current edition file paths, bounded and validated by the task admission policy.
    pub files: Vec<String>,
    publish: Option<Arc<dyn Fn(JobOutput) + Send + Sync>>,
}
impl std::fmt::Debug for CheckRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckRequest")
            .field("files", &self.files)
            .finish_non_exhaustive()
    }
}
impl From<Vec<String>> for CheckRequest {
    fn from(files: Vec<String>) -> Self {
        Self {
            files,
            publish: None,
        }
    }
}
impl CheckRequest {
    /// Publish only fully decoded file verdicts; no raw bytes enter public activity events.
    pub fn report(&self, result: JobOutput) {
        if let Some(publish) = &self.publish {
            publish(result);
        }
    }
}
pub(super) fn bind(state: &ApiState, id: u64, mut check: CheckRequest) -> CheckRequest {
    let jobs = Arc::downgrade(&state.jobs);
    let expected = check.files.clone();
    check.publish = Some(Arc::new(move |output| {
        let Some(count) = snapshot_count(&output, &expected) else {
            return;
        };
        let Some(jobs) = jobs.upgrade() else {
            return;
        };
        let Ok(mut data) = jobs.data.lock() else {
            return;
        };
        let Some(task) = data.tasks.get_mut(&id) else {
            return;
        };
        if task.status != Status::Running || task.task_kind != "album_decode_test" {
            return;
        }
        let previous = task
            .partial_result
            .as_ref()
            .and_then(|held| snapshot_count(held, &expected))
            .unwrap_or(0);
        if count > previous {
            task.partial_result = Some(output);
        }
    }));
    check
}
fn snapshot_count(output: &JobOutput, expected: &[String]) -> Option<usize> {
    if output.output_truncated
        || output.stdout.len() > MAX_OUTPUT
        || !output.stderr.is_empty()
        || !matches!(output.exit_code, 0 | 1)
    {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(&output.stdout).ok()?;
    if value.get("version")?.as_u64()? != 1 {
        return None;
    }
    let files = value.get("files")?.as_array()?;
    if files.is_empty() || files.len() > expected.len() {
        return None;
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut failed = false;
    for file in files {
        let path = file.get("path")?.as_str()?;
        if !expected.iter().any(|known| known == path) || !seen.insert(path) {
            return None;
        }
        match file.get("status")?.as_str()? {
            "ko" => failed = true,
            "ok" | "no_hash" => {}
            _ => return None,
        }
        let verifier = file.get("verifier")?.as_str()?;
        if verifier.trim().is_empty() || verifier.len() > 256 {
            return None;
        }
        if let Some(message) = file.get("message")
            && message.as_str()?.len() > 4096
        {
            return None;
        }
    }
    (failed == (output.exit_code == 1)).then_some(files.len())
}
#[cfg(test)]
#[path = "job_check_progress_tests.rs"]
mod tests;
