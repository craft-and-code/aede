//! Authenticated, source-guarded complete-track waveform cache.
use super::auth::confirm_access;
use super::playback_api::{source_from_catalog, validate_source};
use super::*;
use aede_core::playback::waveform;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    #[serde(rename = "ref")]
    reference: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct View {
    reference: String,
    duration_ms: u64,
    sample_rate: u32,
    bucket_frames: u64,
    peaks: Vec<f32>,
}
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/v1/waveform", get(read))
}
fn failure(message: &str) -> ApiError {
    error(StatusCode::CONFLICT, "waveform_unavailable", message)
}
async fn read(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Json<View>, ApiError> {
    let Query(query) = input.map_err(|failure| invalid_query(failure.body_text()))?;
    let identity = auth::SocketIdentity {
        session: request.extensions().get::<auth::Principal>().cloned(),
        administrative: request
            .extensions()
            .get::<auth::AdministrativeAccess>()
            .is_some(),
    };
    let requested = EntityRef::parse_token(&query.reference)
        .filter(|value| value.kind == EntityKind::Track && !value.key.is_empty())
        .ok_or_else(|| invalid_query("ref must name a track"))?;
    let source = {
        let guard = state.catalog.read().await;
        source_from_catalog(guard.as_ref().ok_or_else(unavailable)?, &requested)
            .map_err(|_| failure("current track source unavailable; rescan first"))?
    };
    let permit = state
        .inspection_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "waveform_busy",
                "waveform workers busy",
            )
        })?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        validate_source(&source)
            .map_err(|_| failure("source changed or unavailable; rescan first"))?;
        let mut hash = DefaultHasher::new();
        (
            source.reference.to_token(),
            &source.path,
            source.file.size,
            source.file.mtime,
            source.mtime_subseconds,
        )
            .hash(&mut hash);
        let path = state.data_dir.join("waveforms").join(format!(
            "v1-{}-{:016x}.json",
            aede_core::PCM_DECODER_PROVENANCE,
            hash.finish()
        ));
        if let Ok(metadata) = fs::symlink_metadata(&path)
            && metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= 65536
            && let Ok(bytes) = cached_bytes(&path)
            && let Ok(view) = serde_json::from_slice::<View>(&bytes)
            && view.reference == query.reference
            && valid(&view)
        {
            validate_source(&source).map_err(|_| failure("source changed"))?;
            confirm_access(&state, &identity)?;
            return Ok(Json(view));
        }
        let decoded =
            waveform::decode(&source.path, Duration::from_secs(60)).map_err(|message| {
                eprintln!("waveform decode failed: {message}");
                failure("audio could not be decoded within waveform budget")
            })?;
        validate_source(&source).map_err(|_| failure("source changed while decoding"))?;
        let view = View {
            reference: query.reference,
            duration_ms: decoded.duration_ms,
            sample_rate: decoded.sample_rate,
            bucket_frames: decoded.bucket_frames,
            peaks: decoded.peaks,
        };
        let bytes =
            serde_json::to_vec(&view).map_err(|_| failure("waveform serialization failed"))?;
        fs::create_dir_all(path.parent().ok_or_else(|| failure("invalid cache path"))?)
            .map_err(|_| failure("waveform cache unavailable"))?;
        aede_core::store::write_new_atomic(&path, &bytes)
            .map_err(|_| failure("waveform cache write failed"))?;
        confirm_access(&state, &identity)?;
        Ok(Json(view))
    })
    .await
    .map_err(|_| failure("waveform worker failed"))?
}
fn cached_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(std::io::Error::other("waveform cache too large"));
    }
    Ok(bytes)
}
fn valid(view: &View) -> bool {
    view.duration_ms > 0
        && view.duration_ms <= 86400000
        && (8000..=768000).contains(&view.sample_rate)
        && view.bucket_frames > 0
        && !view.peaks.is_empty()
        && view.peaks.len() <= 2048
        && view
            .peaks
            .iter()
            .all(|peak| peak.is_finite() && (0.0..=1.0).contains(peak))
}
#[cfg(test)]
#[path = "waveform_tests.rs"]
mod tests;
