//! Independently revocable client credentials, without recoverable passwords.

use std::collections::BTreeMap;

use super::*;

/// Maximum number of persistent API keys in one installation.
pub const MAX_API_KEYS: usize = 512;
/// Maximum number of persistent API keys belonging to one account.
pub const MAX_API_KEYS_PER_ACCOUNT: usize = 8;
/// Maximum UTF-8 label length, measured before any presentation escaping.
pub const MAX_API_KEY_LABEL_BYTES: usize = 128;

/// Safe public metadata for an independently revocable API credential.
/// The owner, credential generation and salted verifier stay private.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey {
    /// Opaque public identifier; this alone never authenticates a client.
    pub id: String,
    /// User-supplied description, preserving its original spelling.
    pub label: String,
    /// Creation time in Unix seconds.
    pub created_at: u64,
    owner: String,
    revision: u64,
    secret_hash: String,
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApiKey")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("created_at", &self.created_at)
            .finish_non_exhaustive()
    }
}

impl Accounts {
    /// List one account's keys in creation order, without secrets or verifiers.
    /// Disabled accounts may still be inspected by the local administrator.
    pub fn api_keys(&self, username: &str) -> Result<Vec<&ApiKey>, String> {
        let account = self
            .find(username)
            .ok_or("no account has this login name")?;
        Ok(self
            .api_keys
            .iter()
            .filter(|key| key.owner == account.id)
            .collect())
    }

    /// Issue an OS-random `id.secret` credential and retain only its salted verifier.
    /// Both components contain 256 random bits encoded as lowercase hexadecimal.
    /// The returned token can be shown once, after the caller successfully saves
    /// the store under its writer lock; it cannot be recovered by later reads.
    /// Password work must run on a bounded blocking worker for HTTP callers.
    pub fn create_api_key(
        &mut self,
        username: &str,
        label: &str,
        now: u64,
    ) -> Result<(ApiKey, String), String> {
        let account = self
            .find(username)
            .ok_or("no account has this login name")?;
        if !account.enabled {
            return Err("disabled accounts cannot receive API keys".into());
        }
        if !valid_label(label) {
            return Err(
                "API key labels need non-whitespace text and at most 128 UTF-8 bytes".into(),
            );
        }
        if self.api_keys.len() >= MAX_API_KEYS {
            return Err("the installation already has 512 API keys".into());
        }
        if self
            .api_keys
            .iter()
            .filter(|key| key.owner == account.id)
            .count()
            >= MAX_API_KEYS_PER_ACCOUNT
        {
            return Err("this account already has eight API keys".into());
        }
        if now > MAX_REVISION {
            return Err("API key creation time is too large".into());
        }
        let id = random_token()?;
        if self.api_keys.iter().any(|key| key.id == id) {
            return Err("could not generate a unique API key identifier; retry".into());
        }
        let secret = random_token()?;
        let key = ApiKey {
            id,
            label: label.into(),
            created_at: now,
            owner: account.id.clone(),
            revision: account.revision,
            secret_hash: hash_password(&secret)?,
        };
        let token = format!("{}.{}", key.id, secret);
        self.api_keys.push(key.clone());
        Ok((key, token))
    }

    /// Revoke exactly one named account's key, leaving other keys and sessions valid.
    pub fn revoke_api_key(&mut self, username: &str, key_id: &str) -> Result<(), String> {
        let account = self
            .find(username)
            .ok_or("no account has this login name")?;
        let index = self
            .api_keys
            .iter()
            .position(|key| key.owner == account.id && key.id == key_id)
            .ok_or("no API key with this identifier belongs to the account")?;
        self.api_keys.remove(index);
        Ok(())
    }

    /// Verify an exact `id.secret` credential and return its enabled account and key.
    /// Well-formed unknown keys perform the same bounded Argon2 work as known
    /// keys. HTTP callers must schedule verification on a bounded worker and
    /// apply admission/rate limits; these persistent tokens are not sessions.
    pub fn authenticate_api_key(&self, token: &str) -> Option<(&Account, &ApiKey)> {
        if token.len() != 129 {
            return None;
        }
        let (id, secret) = token.split_once('.')?;
        if !valid_token(id) || !valid_token(secret) {
            return None;
        }
        let key = self.api_keys.iter().find(|key| key.id == id);
        let hash = key.map_or(DUMMY_HASH, |key| key.secret_hash.as_str());
        let parsed = PasswordHash::new(hash).ok()?;
        let matches = hasher()
            .ok()?
            .verify_password(secret.as_bytes(), &parsed)
            .is_ok();
        let key = key.filter(|_| matches)?;
        let account = self.api_key_account(&key.id, &key.owner, &self.epoch, key.revision)?;
        Some((account, key))
    }

    /// Recheck a previously authenticated key after reloading the current store.
    /// This checks presence, owner, enabled state, epoch and generation without
    /// password work. The caller must already have verified the complete token;
    /// possessing a public key ID or account metadata is never authentication.
    pub fn api_key_account(
        &self,
        key_id: &str,
        owner: &str,
        epoch: &str,
        revision: u64,
    ) -> Option<&Account> {
        let account = self.session_account(owner, epoch, revision)?;
        self.api_keys
            .iter()
            .any(|key| key.id == key_id && key.owner == owner && key.revision == revision)
            .then_some(account)
    }

    pub(super) fn purge_api_keys(&mut self, owner: &str) {
        self.api_keys.retain(|key| key.owner != owner);
    }
}

fn valid_label(label: &str) -> bool {
    label.len() <= MAX_API_KEY_LABEL_BYTES && !label.trim().is_empty()
}

pub(super) fn to_json(keys: &[ApiKey]) -> Json {
    Json::Arr(
        keys.iter()
            .map(|key| {
                let mut row = Json::obj();
                row.set("id", key.id.clone().into());
                row.set("label", key.label.clone().into());
                row.set("created_at", key.created_at.into());
                row.set("owner", key.owner.clone().into());
                row.set("revision", key.revision.into());
                row.set("secret_hash", key.secret_hash.clone().into());
                row
            })
            .collect(),
    )
}

pub(super) fn from_json(
    value: Option<&Json>,
    accounts: &[Account],
) -> Result<Vec<ApiKey>, StoreError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let invalid = || StoreError::AccountsInvalid("malformed or excessive API key records");
    let rows = value
        .as_arr()
        .filter(|rows| rows.len() <= MAX_API_KEYS)
        .ok_or_else(invalid)?;
    let mut keys = Vec::with_capacity(rows.len());
    let mut ids = BTreeSet::new();
    let mut counts = BTreeMap::<&str, usize>::new();
    for row in rows {
        let string = |field| row.get(field).and_then(Json::as_str).ok_or_else(invalid);
        let number = |field| {
            row.get(field)
                .and_then(Json::as_u64)
                .filter(|value| *value <= MAX_REVISION)
                .ok_or_else(invalid)
        };
        let id = string("id")?;
        let label = string("label")?;
        let owner = string("owner")?;
        let secret_hash = string("secret_hash")?;
        let revision = number("revision")?;
        if !valid_token(id)
            || !ids.insert(id)
            || !valid_label(label)
            || !valid_hash(secret_hash)
            || !accounts.iter().any(|account| {
                account.id == owner && account.enabled && account.revision == revision
            })
        {
            return Err(invalid());
        }
        let count = counts.entry(owner).or_default();
        *count += 1;
        if *count > MAX_API_KEYS_PER_ACCOUNT {
            return Err(invalid());
        }
        keys.push(ApiKey {
            id: id.into(),
            label: label.into(),
            created_at: number("created_at")?,
            owner: owner.into(),
            revision,
            secret_hash: secret_hash.into(),
        });
    }
    Ok(keys)
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
