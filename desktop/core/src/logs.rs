//! Bounded process-local diagnostics; raw child output never reaches the webview.
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::BufRead,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
const CAPACITY: usize = 500;
const LINE_LIMIT: usize = 4096;
/// One safe diagnostic entry. IDs are monotonic even after clearing the view.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    id: u64,
    time_ms: u64,
    level: &'static str,
    message: String,
}
/// Only entries newer than the requested cursor are copied across IPC.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    entries: Vec<Entry>,
    next_id: u64,
}
#[derive(Default)]
struct Buffer {
    entries: VecDeque<Entry>,
    next: u64,
}
#[derive(Clone, Default)]
pub(crate) struct Logs(Arc<Mutex<Buffer>>);
impl Logs {
    pub(crate) fn push(&self, level: &'static str, message: String) {
        if let Ok(mut buffer) = self.0.lock() {
            buffer.next += 1;
            let id = buffer.next;
            if buffer.entries.len() == CAPACITY {
                buffer.entries.pop_front();
            }
            buffer.entries.push_back(Entry {
                id,
                time_ms: now_ms(),
                level,
                message,
            });
        }
    }
    pub(crate) fn snapshot(&self, after: u64) -> Result<Snapshot, String> {
        let buffer = self.0.lock().map_err(|_| "Journaux indisponibles.")?;
        Ok(Snapshot {
            entries: buffer
                .entries
                .iter()
                .filter(|entry| entry.id > after)
                .cloned()
                .collect(),
            next_id: buffer.next,
        })
    }
    pub(crate) fn export_json(&self) -> Result<String, String> {
        let snapshot = self.snapshot(0)?;
        if snapshot.entries.is_empty() {
            return Err("Aucun journal à exporter.".into());
        }
        serde_json::to_string_pretty(&serde_json::json!({
            "format": "aede-logs", "version": 1, "exportedAtMs": now_ms(),
            "entries": snapshot.entries
        }))
        .map_err(|_| "Export des journaux impossible.".into())
    }
    pub(crate) fn clear(&self) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "Journaux indisponibles.")?
            .entries
            .clear();
        Ok(())
    }
    fn http_line(&self, line: &[u8]) -> bool {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Summary {
            method: String,
            route: String,
            status: u16,
            elapsed_ms: u64,
        }
        let Some(json) = line.strip_prefix(b"AEDE_HTTP ") else {
            return false;
        };
        let Ok(summary) = serde_json::from_slice::<Summary>(json) else {
            return false;
        };
        // Accept only the engine's safe schema, never interpolate an arbitrary stderr line.
        if ![
            "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "OTHER",
        ]
        .contains(&summary.method.as_str())
            || !(100..=599).contains(&summary.status)
            || summary.route.len() > 160
            || !summary
                .route
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/._-{}*:".contains(&b))
        {
            return false;
        }
        self.push(
            if summary.status >= 500 {
                "error"
            } else if summary.status >= 400 {
                "warn"
            } else {
                "info"
            },
            format!(
                "{} {} · {} · {} ms",
                summary.method, summary.route, summary.status, summary.elapsed_ms
            ),
        );
        true
    }
    /// Drain stderr without unbounded line allocations or copying task output/secrets into logs.
    pub(crate) fn drain(&self, mut reader: impl BufRead) {
        let mut line = Vec::with_capacity(256);
        let mut oversized = false;
        let mut diagnostic_reported = false;
        loop {
            let bytes = match reader.fill_buf() {
                Ok(bytes) => bytes,
                Err(_) => {
                    self.push("error", "Lecture des journaux interrompue.".into());
                    break;
                }
            };
            if bytes.is_empty() {
                if !oversized && !line.is_empty() && !self.http_line(&line) && !diagnostic_reported
                {
                    self.push("warn", "Le moteur a émis un diagnostic. Consultez le résultat de la tâche ou l’erreur de démarrage.".into());
                }
                break;
            }
            let end = bytes.iter().position(|&b| b == b'\n');
            let length = end.map_or(bytes.len(), |end| end + 1);
            if !oversized {
                if line.len() + length <= LINE_LIMIT {
                    line.extend_from_slice(&bytes[..length]);
                } else {
                    oversized = true;
                    line.clear();
                }
            }
            reader.consume(length);
            if end.is_some() {
                if (oversized || (!line.is_empty() && !self.http_line(&line)))
                    && !diagnostic_reported
                {
                    self.push("warn", "Le moteur a émis un diagnostic. Consultez le résultat de la tâche ou l’erreur de démarrage.".into());
                    diagnostic_reported = true;
                }
                line.clear();
                oversized = false;
            }
        }
    }
}
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
