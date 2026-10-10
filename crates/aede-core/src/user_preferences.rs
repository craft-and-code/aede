//! Bounded client preferences, private to an immutable account owner.
use super::UserData;
use crate::{json::Json, store::StoreError};
use std::collections::BTreeSet;

/// One application-owned JSON document; no credentials belong in this table.
#[derive(Debug, Clone)]
pub struct ClientPreference {
    /// Immutable personal owner.
    pub owner: String,
    /// ASCII application preference key.
    pub key: String,
    /// Valid bounded JSON, retained without rewriting unknown application fields.
    pub value: String,
}
/// Validate storage bounds independently of any HTTP/frontend schema.
pub fn valid_preference(key: &str, value: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_:".contains(&b))
        && value.len() <= 8192
        && crate::json::parse(value).is_ok()
}
pub(super) fn write(data: &UserData, root: &mut Json) {
    if data.client_preferences.is_empty() {
        return;
    }
    root.set(
        "client_preferences",
        Json::Arr(
            data.client_preferences
                .iter()
                .map(|entry| {
                    let mut row = Json::obj();
                    row.set("owner", entry.owner.as_str().into());
                    row.set("key", entry.key.as_str().into());
                    row.set("value", entry.value.as_str().into());
                    row
                })
                .collect(),
        ),
    );
}
pub(super) fn read(root: &Json) -> Result<Vec<ClientPreference>, StoreError> {
    let Some(table) = root.get("client_preferences") else {
        return Ok(vec![]);
    };
    let rows = table
        .as_arr()
        .ok_or(StoreError::Invalid("client preferences must be an array"))?;
    if rows.len() > 8192 {
        return Err(StoreError::Invalid("too many client preferences"));
    }
    let mut known = BTreeSet::new();
    rows.iter()
        .map(|row| {
            let owner = row
                .field_str("owner")
                .ok_or(StoreError::Invalid("missing preference owner"))?;
            let key = row
                .field_str("key")
                .ok_or(StoreError::Invalid("missing preference key"))?;
            let value = row
                .field_str("value")
                .ok_or(StoreError::Invalid("missing preference value"))?;
            if owner.is_empty()
                || owner.len() > 256
                || !valid_preference(&key, &value)
                || !known.insert((owner.clone(), key.clone()))
            {
                return Err(StoreError::Invalid(
                    "invalid or duplicate client preference",
                ));
            }
            Ok(ClientPreference { owner, key, value })
        })
        .collect()
}
#[cfg(test)]
#[path = "user_preferences_tests.rs"]
mod tests;
