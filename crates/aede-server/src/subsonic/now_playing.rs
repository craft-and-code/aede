//! Short-lived client declarations, scoped to their owner and current API key.
//! They never become listening evidence or fabricate a played duration.

use aede_core::user::EntityRef;

use super::*;
use authentication::Identity;

const MAX_REPORTS: usize = 128;

#[derive(Clone)]
pub(super) struct Reported {
    identity: Identity,
    track: EntityRef,
    at_ms: u64,
    expires: Instant,
    client: String,
}

pub(super) async fn report(
    state: &ApiState,
    identity: &Identity,
    parameters: &Parameters,
) -> Result<Value, ProtocolError> {
    parameters.allowed(&["id", "time", "submission"])?;
    authentication::require_audio(identity)?;
    if parameters.values("id").len() != 1 || parameters.values("time").len() > 1 {
        return Err(ProtocolError::new(
            10,
            "now-playing needs exactly one song and at most one time",
        ));
    }
    let at_ms = if let Some(time) = parameters.get("time") {
        let at = time
            .parse::<u64>()
            .ok()
            .filter(|_| !time.is_empty() && time.bytes().all(|byte| byte.is_ascii_digit()))
            .ok_or_else(|| ProtocolError::new(10, "time must be Unix milliseconds"))?;
        let now = aede_core::clock::now_seconds().saturating_mul(1000);
        if at > now.saturating_add(86_400_000) {
            return Err(ProtocolError::new(
                10,
                "time cannot be more than one day in the future",
            ));
        }
        at
    } else {
        aede_core::clock::now_seconds().saturating_mul(1000)
    };
    let wanted = parameters.required("id")?.to_string();
    let (track, lifetime) = inspect(state.clone(), move |catalog| {
        let track = catalog::track_reference(catalog, &wanted)?;
        let id = track
            .resolve(catalog)
            .ok_or_else(|| ProtocolError::new(70, "song unavailable"))?;
        let duration = catalog
            .track(id)
            .and_then(|track| track.duration_ms)
            .unwrap_or(300_000);
        Ok((
            track,
            Duration::from_millis(duration.clamp(30_000, 86_400_000)),
        ))
    })
    .await?;
    authentication::recheck(state, identity).await?;
    let now = Instant::now();
    let mut reports = state
        .subsonic
        .now_playing
        .lock()
        .map_err(|_| ProtocolError::new(0, "now-playing state unavailable"))?;
    reports.retain(|report| report.expires > now && report.identity.key_id != identity.key_id);
    if reports.len() >= MAX_REPORTS {
        reports.remove(0);
    }
    reports.push(Reported {
        identity: identity.clone(),
        track,
        at_ms,
        expires: now + lifetime,
        client: parameters.required("c")?.to_string(),
    });
    Ok(json!({}))
}

pub(super) async fn list(
    state: &ApiState,
    identity: &Identity,
    parameters: &Parameters,
) -> Result<Value, ProtocolError> {
    parameters.allowed(&[])?;
    let accounts = auth::current_accounts(state)
        .await
        .map_err(|_| ProtocolError::new(0, "accounts unavailable"))?
        .ok_or_else(|| ProtocolError::new(44, "invalid API key"))?;
    let now = Instant::now();
    let now_ms = aede_core::clock::now_seconds().saturating_mul(1000);
    let selected = {
        let mut reports = state
            .subsonic
            .now_playing
            .lock()
            .map_err(|_| ProtocolError::new(0, "now-playing state unavailable"))?;
        reports.retain(|report| {
            report.expires > now
                && accounts
                    .api_key_account(
                        &report.identity.key_id,
                        &report.identity.owner,
                        &report.identity.epoch,
                        report.identity.revision,
                    )
                    .is_some()
        });
        reports
            .iter()
            .filter(|report| report.identity.owner == identity.owner)
            .cloned()
            .collect::<Vec<_>>()
    };
    inspect(state.clone(), move |catalog| {
        let index = catalog::Index::new(catalog)?;
        let mut entries = Vec::new();
        for report in selected {
            if let Ok(mut value) = index.render_reference(&report.track) {
                value["username"] = json!(report.identity.username);
                value["minutesAgo"] = json!(now_ms.saturating_sub(report.at_ms) / 60_000);
                value["playerName"] = json!(report.client);
                entries.push(value);
            }
        }
        Ok(json!({"nowPlaying": {"entry": entries}}))
    })
    .await
}

#[cfg(test)]
#[path = "now_playing_tests.rs"]
mod tests;
