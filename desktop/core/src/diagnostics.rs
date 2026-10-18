//! Explicit read-only probes over the same pinned HTTPS connection used for administration.
use crate::{Host, logs};
use serde::{Deserialize, Serialize};
use std::time::Instant;
/// Minimal catalog counts; no account, file paths or personal data are returned.
#[derive(Serialize, Deserialize)]
pub struct Library {
    scanned_at: u64,
    files: u64,
    artists: u64,
    releases: u64,
    tracks: u64,
}
/// Results from one explicit health check. Process existence is distinct from API availability.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    checked_at: u64,
    running: bool,
    api_responding: bool,
    api_version: Option<u64>,
    latency_ms: u64,
    catalog_loaded: bool,
    library: Option<Library>,
    error: Option<String>,
}
impl Host {
    /// Probe only fixed status/catalog endpoints; never scan, mutate or contact another host.
    pub fn diagnostics(&mut self) -> Result<Diagnostics, String> {
        let start = Instant::now();
        let mut result = Diagnostics {
            checked_at: logs::now_ms(),
            running: self.status()?.running,
            api_responding: false,
            api_version: None,
            latency_ms: 0,
            catalog_loaded: false,
            library: None,
            error: None,
        };
        if result.running {
            let check = (|| {
                let status = self.request("GET", "/api/v1/status", None)?;
                if status["status"] != "ok" {
                    return Err("État de l’API inattendu.".into());
                }
                result.api_version = Some(
                    status["api_version"]
                        .as_u64()
                        .ok_or("Version API indisponible.")?,
                );
                result.catalog_loaded = status["catalog_loaded"]
                    .as_bool()
                    .ok_or("État du catalogue indisponible.")?;
                result.api_responding = true;
                if result.catalog_loaded {
                    result.library = Some(
                        serde_json::from_value(self.request("GET", "/api/v1/library", None)?)
                            .map_err(|_| "Comptage du catalogue illisible.")?,
                    );
                }
                Ok::<_, String>(())
            })();
            result.error = check.err();
        }
        result.latency_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
        Ok(result)
    }
}
