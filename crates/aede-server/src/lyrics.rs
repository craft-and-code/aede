//! Complete local lyrics delivery, separate from the PCM playback timeline.

use super::*;
use aede_core::lyrics::{self, CurrentTrack, ReadError, Source};
use std::io::{self, Write};

const MAX_INPUT_BYTES: usize = 256 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const WORKER_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/v1/lyrics", get(read))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    track: String,
}

struct Selected {
    reference: String,
    path: PathBuf,
    size: u64,
    mtime: u64,
    mtime_subseconds: u32,
    tag: Option<String>,
    sidecar: Option<PathBuf>,
}

fn select(catalog: &Catalog, requested: &EntityRef) -> Result<Selected, ApiError> {
    let id = requested.resolve(catalog).ok_or_else(|| {
        error(
            StatusCode::NOT_FOUND,
            "entity_not_found",
            "no track matches this reference in the current catalog",
        )
    })?;
    let track = catalog.track(id).ok_or_else(unavailable)?;
    let file = catalog.file(track.file_id).ok_or_else(unavailable)?;
    let mtime_subseconds = catalog
        .file_mtime_subseconds
        .get(&file.path)
        .copied()
        .ok_or_else(|| read_failure(ReadError::SourceChanged))?;
    let tag = file.first_tag("lyrics");
    if tag.is_some_and(|text| text.len() > MAX_INPUT_BYTES) {
        return Err(too_large());
    }
    Ok(Selected {
        reference: reference(catalog, EntityKind::Track, id).ok_or_else(unavailable)?,
        path: PathBuf::from(&file.path),
        size: file.size,
        mtime: file.mtime,
        mtime_subseconds,
        tag: tag.map(str::to_owned),
        sidecar: file.lyrics_path.as_ref().map(PathBuf::from),
    })
}

#[derive(Serialize)]
struct Answer<'a> {
    track: &'a str,
    lyrics: Option<Words<'a>>,
}

#[derive(Serialize)]
struct Words<'a> {
    source: &'static str,
    synced: bool,
    lines: Vec<Line<'a>>,
}

#[derive(Serialize)]
struct Line<'a> {
    at_ms: Option<u64>,
    text: &'a str,
}

fn too_large() -> ApiError {
    error(
        StatusCode::PAYLOAD_TOO_LARGE,
        "lyrics_too_large",
        "complete lyrics exceed the input, expansion or response size limit",
    )
}

fn read_failure(failure: ReadError) -> ApiError {
    let (status, code) = match failure {
        ReadError::SourceUnavailable => (StatusCode::NOT_FOUND, "source_unavailable"),
        ReadError::SourceChanged => (StatusCode::CONFLICT, "source_changed"),
        ReadError::SidecarUnavailable => (StatusCode::NOT_FOUND, "lyrics_unavailable"),
        ReadError::SidecarChanged => (StatusCode::CONFLICT, "lyrics_changed"),
        ReadError::TooLarge => return too_large(),
        ReadError::ReadFailed | ReadError::TagsUnavailable => {
            (StatusCode::INTERNAL_SERVER_ERROR, "lyrics_read_failed")
        }
    };
    error(status, code, failure.to_string())
}

/// Bound the serialized bytes as well as text: JSON escaping and line metadata
/// can grow well beyond the original source, even for timestamp-only lines.
#[derive(Default)]
struct LimitedJson(Vec<u8>);

impl Write for LimitedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_RESPONSE_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("lyrics response size limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn serialized(selected: &Selected) -> Result<Vec<u8>, ApiError> {
    let lyrics = lyrics::read_current(CurrentTrack {
        path: &selected.path,
        size: selected.size,
        mtime: selected.mtime,
        mtime_subseconds: selected.mtime_subseconds,
        tag: selected.tag.as_deref(),
        sidecar: selected.sidecar.as_deref(),
    })
    .map_err(read_failure)?;
    let answer = Answer {
        track: &selected.reference,
        lyrics: lyrics.as_ref().map(|lyrics| Words {
            source: match lyrics.source {
                Source::Tag => "tag",
                Source::Sidecar => "sidecar",
            },
            synced: lyrics.synced(),
            lines: lyrics
                .lines
                .iter()
                .map(|line| Line {
                    at_ms: line.at_ms,
                    text: &line.text,
                })
                .collect(),
        }),
    };
    let mut output = LimitedJson::default();
    serde_json::to_writer(&mut output, &answer).map_err(|_| too_large())?;
    Ok(output.0)
}

fn confirm_access(state: &ApiState, identity: &auth::SocketIdentity) -> Result<(), ApiError> {
    if let Some(principal) = &identity.session {
        auth::recheck(state, principal)
    } else if identity.administrative || auth::load_accounts(state)?.is_none() {
        Ok(())
    } else {
        // An initially anonymous read must not publish after account activation.
        Err(auth::unauthorized())
    }
}

async fn read(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Response, ApiError> {
    let Query(parameters) = input.map_err(|rejection| invalid_query(rejection.body_text()))?;
    bounded_text(&parameters.track, "track", MAX_REFERENCE_BYTES)?;
    let requested = EntityRef::parse_token(&parameters.track)
        .filter(|reference| reference.kind == EntityKind::Track && !reference.key.trim().is_empty())
        .ok_or_else(|| invalid_query("track must be a stable track reference"))?;
    let identity = auth::SocketIdentity {
        session: request.extensions().get::<auth::Principal>().cloned(),
        administrative: request
            .extensions()
            .get::<auth::AdministrativeAccess>()
            .is_some(),
    };
    let Json(bytes) = tokio::time::timeout(
        WORKER_TIMEOUT,
        inspection::inspect(state, move |state| {
            let selected = {
                let guard = state.catalog.blocking_read();
                let catalog = guard.as_ref().ok_or_else(unavailable)?;
                select(catalog, &requested)?
            };
            let bytes = serialized(&selected)?;
            confirm_access(state, &identity)?;
            Ok(bytes)
        }),
    )
    .await
    .map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "lyrics_timeout",
            "the local lyrics read did not finish in time",
        )
    })??;
    Ok(([(header::CONTENT_TYPE, "application/json")], bytes).into_response())
}

#[cfg(test)]
#[path = "lyrics_tests.rs"]
mod tests;
