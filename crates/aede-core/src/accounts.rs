//! Installation persistence facade for the independent account domain.
//!
//! Identities, roles, credential policy and revocable client keys belong to
//! `aede-accounts`. This adapter retains bounded private-file access, the
//! existing JSON parser, atomic writes and compatibility with full backups.
//! Callers hold the data-directory writer lock for read/modify/save operations.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::json::{self, Json};
use crate::store::StoreError;

pub use aede_accounts::{
    ACCOUNTS_FILE, Account, Accounts, ApiKey, FORMAT_VERSION, LOCAL_OWNER, MAX_ACCOUNTS,
    MAX_API_KEY_LABEL_BYTES, MAX_API_KEYS, MAX_API_KEYS_PER_ACCOUNT, MAX_PASSWORD_BYTES, Role,
    login_name, random_token,
};

const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Build the private credential-store path.
pub fn accounts_path(data_dir: &Path) -> PathBuf {
    data_dir.join(ACCOUNTS_FILE)
}

/// Refuse links, special files and Unix credentials readable by other users.
/// An absent destination is accepted for a new atomic write.
pub fn check_private_file(path: &Path) -> Result<(), StoreError> {
    inspect_private_file(path).map(|_| ())
}

fn inspect_private_file(path: &Path) -> Result<Option<std::fs::Metadata>, StoreError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            check_private_metadata(&metadata)?;
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn check_private_metadata(metadata: &std::fs::Metadata) -> Result<(), StoreError> {
    if !metadata.is_file() {
        return Err(StoreError::AccountsInvalid(
            "credentials require a regular file, never a symbolic link",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(StoreError::AccountsInvalid(
                "credential files must be private (chmod 600)",
            ));
        }
    }
    Ok(())
}

/// Validate the inspected source and the descriptor that supplied credentials.
/// On Unix their device/inode identity must also match, so an intervening
/// replacement cannot supply a different account store or private backup.
pub(crate) fn validate_private_read(
    inspected: &std::fs::Metadata,
    opened: &std::fs::Metadata,
) -> Result<(), StoreError> {
    check_private_metadata(inspected)?;
    check_private_metadata(opened)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if inspected.dev() != opened.dev() || inspected.ino() != opened.ino() {
            return Err(StoreError::AccountsInvalid(
                "credential source changed while it was opened; retry",
            ));
        }
    }
    Ok(())
}

/// Read a bounded private account file; initial absence is distinct from malformed data.
/// The opened descriptor is rechecked for regular-file type and private Unix
/// permissions, including device/inode identity on Unix. Disappearance after
/// inspection is an error, so it cannot silently disable authentication.
pub fn load(path: &Path) -> Result<Option<Accounts>, StoreError> {
    load_with_open(path, |path| std::fs::File::open(path))
}

fn load_with_open(
    path: &Path,
    open: impl FnOnce(&Path) -> std::io::Result<std::fs::File>,
) -> Result<Option<Accounts>, StoreError> {
    let Some(inspected) = inspect_private_file(path)? else {
        return Ok(None);
    };
    let file = open(path)?;
    let opened = file.metadata()?;
    validate_private_read(&inspected, &opened)?;
    if opened.len() > MAX_FILE_BYTES {
        return Err(StoreError::AccountsInvalid("account file exceeds 1 MiB"));
    }
    let mut text = String::new();
    file.take(MAX_FILE_BYTES + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(StoreError::AccountsInvalid("account file exceeds 1 MiB"));
    }
    from_json(&json::parse(&text).map_err(StoreError::AccountsParse)?).map(Some)
}

/// Save credentials atomically, preserving private permissions and validating all rows.
/// The caller must hold the existing data-directory writer lock.
pub fn save(accounts: &Accounts, path: &Path) -> Result<(), StoreError> {
    check_private_file(path)?;
    let document = to_json(accounts);
    from_json(&document)?;
    crate::atomic_file::write(path, document.to_string_compact().as_bytes())?;
    Ok(())
}

/// Serialize the credential store for protected persistence and full backups only.
/// Never expose this document as an account-list or catalog API response.
pub fn to_json(accounts: &Accounts) -> Json {
    from_value(aede_accounts::to_json(accounts))
}

/// Refuse malformed, duplicate or administrator-less credentials as one whole store.
/// The independent domain retains the bounded hash policy and numeric contract.
pub fn from_json(root: &Json) -> Result<Accounts, StoreError> {
    aede_accounts::from_json(&to_value(root, 0)?)
        .map_err(|error| StoreError::AccountsInvalid(error.reason()))
}

// Keep the existing parser and public Json API at the persistence boundary.
// Its numbers are f64; translating the tree never reparses or rounds a value.
fn to_value(value: &Json, depth: usize) -> Result<serde_json::Value, StoreError> {
    use serde_json::Value;
    if depth >= 128 && matches!(value, Json::Arr(_) | Json::Obj(_)) {
        return Err(StoreError::AccountsInvalid(
            "malformed account store or unsupported version",
        ));
    }
    Ok(match value {
        Json::Null => Value::Null,
        Json::Bool(value) => Value::Bool(*value),
        // Json's writer also emits null for a non-finite programmatic number.
        Json::Num(value) => serde_json::Number::from_f64(*value).map_or(Value::Null, Value::Number),
        Json::Str(value) => Value::String(value.clone()),
        Json::Arr(values) => Value::Array(
            values
                .iter()
                .map(|value| to_value(value, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        Json::Obj(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), to_value(value, depth + 1)?)))
                .collect::<Result<_, StoreError>>()?,
        ),
    })
}

// Only the domain's generated, shallow document reaches this direction.
// Every emitted number is an ordinary finite serde_json number.
fn from_value(value: serde_json::Value) -> Json {
    use serde_json::Value;
    match value {
        Value::Null => Json::Null,
        Value::Bool(value) => Json::Bool(value),
        Value::Number(value) => value.as_f64().map_or(Json::Null, Json::Num),
        Value::String(value) => Json::Str(value),
        Value::Array(values) => Json::Arr(values.into_iter().map(from_value).collect()),
        Value::Object(values) => Json::Obj(
            values
                .into_iter()
                .map(|(key, value)| (key, from_value(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "accounts_test_support.rs"]
mod test_support;
