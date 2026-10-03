//! Persistent API keys with a bounded verifier cache and current-store rechecks.

use std::sync::Mutex as BlockingMutex;

use aede_core::accounts::Role;

use super::*;

const MAX_CACHED_KEYS: usize = 128;

#[derive(Default)]
pub(crate) struct Authentication {
    cached: BlockingMutex<Vec<Cached>>,
    pub(super) now_playing: BlockingMutex<Vec<super::now_playing::Reported>>,
}

struct Cached {
    fingerprint: String,
    identity: Identity,
}

#[derive(Clone, Debug)]
pub(crate) struct Identity {
    pub(crate) owner: String,
    pub(crate) username: String,
    pub(crate) role: Role,
    pub(crate) key_id: String,
    pub(crate) epoch: String,
    pub(crate) revision: u64,
}

fn unavailable() -> ProtocolError {
    ProtocolError::new(
        0,
        "account authentication is unavailable; ask the local operator",
    )
}

fn invalid() -> ProtocolError {
    ProtocolError::new(44, "invalid API key")
}

pub(super) async fn authenticate(
    state: &ApiState,
    parameters: &Parameters,
) -> Result<Identity, ProtocolError> {
    let (token, username) = credential(parameters)?;
    let fingerprint = protocol::sha256(&token)?;
    let accounts = auth::current_accounts(state)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(invalid)?;
    {
        let mut cached = state.subsonic.cached.lock().map_err(|_| unavailable())?;
        cached.retain(|entry| {
            accounts
                .api_key_account(
                    &entry.identity.key_id,
                    &entry.identity.owner,
                    &entry.identity.epoch,
                    entry.identity.revision,
                )
                .is_some()
        });
        if let Some(entry) = cached.iter().find(|entry| entry.fingerprint == fingerprint) {
            return named_identity(entry.identity.clone(), username);
        }
    }
    let key_id = token
        .split_once('.')
        .map(|(id, _)| id)
        .ok_or_else(invalid)?;
    auth::take_attempt(state, key_id).map_err(|_| {
        ProtocolError::new(
            0,
            "too many authentication attempts; retry after one minute",
        )
    })?;
    let permit = state
        .auth
        .password_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| unavailable())?;
    let identity = tokio::task::spawn_blocking(move || -> Result<Identity, ProtocolError> {
        let _permit = permit;
        let (account, key) = accounts.authenticate_api_key(&token).ok_or_else(invalid)?;
        Ok(Identity {
            owner: account.id.clone(),
            username: account.username.clone(),
            role: account.role,
            key_id: key.id.clone(),
            epoch: accounts.epoch().to_string(),
            revision: account.revision(),
        })
    })
    .await
    .map_err(|_| unavailable())??;
    let identity = named_identity(identity, username)?;
    recheck(state, &identity).await?;
    let mut cached = state.subsonic.cached.lock().map_err(|_| unavailable())?;
    if cached.len() >= MAX_CACHED_KEYS {
        cached.remove(0);
    }
    cached.push(Cached {
        fingerprint,
        identity: identity.clone(),
    });
    Ok(identity)
}

fn named_identity(identity: Identity, username: Option<&str>) -> Result<Identity, ProtocolError> {
    if username.is_some_and(|name| !name.eq_ignore_ascii_case(&identity.username)) {
        return Err(invalid());
    }
    Ok(identity)
}

fn credential(parameters: &Parameters) -> Result<(String, Option<&str>), ProtocolError> {
    let token = parameters.get("apiKey");
    let old = ["u", "p", "t", "s"]
        .iter()
        .any(|name| parameters.get(name).is_some());
    if token.is_some() && old {
        return Err(ProtocolError::new(
            43,
            "conflicting authentication mechanisms",
        ));
    }
    if let Some(token) = token {
        return (token.len() == 129)
            .then(|| (token.to_string(), None))
            .ok_or_else(invalid);
    }
    if parameters.get("t").is_some() || parameters.get("s").is_some() {
        return Err(ProtocolError::new(
            if parameters.get("p").is_some() {
                43
            } else {
                41
            },
            "use a revocable API key created with aede accounts keys",
        ));
    }
    let password = parameters.get("p").ok_or_else(|| {
        ProtocolError::new(
            42,
            "use a revocable API key created with aede accounts keys",
        )
    })?;
    let username = parameters.required("u")?;
    let token = if let Some(encoded) = password.strip_prefix("enc:") {
        if encoded.len() != 258 || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ProtocolError::new(
                10,
                "enc: must encode a complete API key",
            ));
        }
        let mut bytes = Vec::with_capacity(129);
        for pair in encoded.as_bytes().as_chunks::<2>().0 {
            let high = (pair[0] as char).to_digit(16).ok_or_else(invalid)?;
            let low = (pair[1] as char).to_digit(16).ok_or_else(invalid)?;
            bytes.push((high * 16 + low) as u8);
        }
        String::from_utf8(bytes).map_err(|_| invalid())?
    } else {
        password.to_string()
    };
    if token.len() != 129 {
        return Err(ProtocolError::new(
            42,
            "the password field must contain a revocable API key",
        ));
    }
    Ok((token, Some(username)))
}

pub(crate) async fn recheck(state: &ApiState, identity: &Identity) -> Result<(), ProtocolError> {
    let accounts = auth::current_accounts(state)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(invalid)?;
    accounts
        .api_key_account(
            &identity.key_id,
            &identity.owner,
            &identity.epoch,
            identity.revision,
        )
        .ok_or_else(invalid)
        .map(|_| ())
}

/// Confirm the current key under a personal-data writer lock, on a bounded worker.
pub(super) fn recheck_blocking(state: &ApiState, identity: &Identity) -> Result<(), ProtocolError> {
    let accounts = auth::load_accounts(state)
        .map_err(|_| unavailable())?
        .ok_or_else(invalid)?;
    accounts
        .api_key_account(
            &identity.key_id,
            &identity.owner,
            &identity.epoch,
            identity.revision,
        )
        .ok_or_else(invalid)
        .map(|_| ())
}

pub(crate) fn require_audio(identity: &Identity) -> Result<(), ProtocolError> {
    if identity.role == Role::Auditor {
        Err(ProtocolError::new(
            50,
            "auditor accounts cannot stream or download audio",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "authentication_tests.rs"]
mod tests;
