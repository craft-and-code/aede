//! Stable personal owners, account administration and protected credentials.
//!
//! The first administrator owns `local`, so existing personal data stays put.
//! Other owners are generated independently of their changeable login names.
//! Callers hold the data-directory writer lock for read/modify/save operations.
//! Sessions belong to the server and are never persisted in this store.
//! Optional persistent client keys retain only salted secret verifiers and
//! are independently revocable; account-generation changes purge them.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

use crate::json::{self, Json};
use crate::store::StoreError;

mod keys;
pub use keys::{ApiKey, MAX_API_KEY_LABEL_BYTES, MAX_API_KEYS, MAX_API_KEYS_PER_ACCOUNT};

/// Account-store format, independent of the catalog and personal stores.
pub const FORMAT_VERSION: u32 = 1;
/// Account store inside the data directory.
pub const ACCOUNTS_FILE: &str = "accounts.json";
/// Maximum number of accounts in one installation.
pub const MAX_ACCOUNTS: usize = 256;
/// Maximum accepted UTF-8 password length in bytes.
pub const MAX_PASSWORD_BYTES: usize = 1024;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_REVISION: u64 = (1 << 53) - 1;
const MEMORY_KIB: u32 = 19 * 1024;
// Unknown names still pay the same bounded Argon2 work. This dummy verifier
// does not authenticate anyone, regardless of the verification result.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$YWFhYWFhYWFhYWFhYWFhYQ$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// Installation privileges; every account may access only its own opinions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// May manage accounts and run administrative installation operations.
    Administrator,
    /// May browse the shared catalog and manage their own personal data.
    User,
    /// May browse the shared catalog and read their own personal data only.
    Auditor,
}

impl Role {
    /// Stable spelling used in persisted and HTTP documents.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Administrator => "admin",
            Self::User => "user",
            Self::Auditor => "auditor",
        }
    }

    /// Parse an explicit role without silently selecting a default.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Self::Administrator),
            "user" => Some(Self::User),
            "auditor" => Some(Self::Auditor),
            _ => None,
        }
    }
}

/// An account with an immutable personal-data owner and private verifier.
#[derive(Clone, PartialEq, Eq)]
pub struct Account {
    /// Stable owner of annotations, history and collections.
    pub id: String,
    /// Canonical lowercase login name, independent of ownership.
    pub username: String,
    /// Installation privileges.
    pub role: Role,
    /// Disabled accounts retain their personal data and cannot authenticate.
    pub enabled: bool,
    /// Creation time in Unix seconds.
    pub created_at: u64,
    /// Most recent administration time in Unix seconds.
    pub updated_at: u64,
    revision: u64,
    password_hash: String,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Account")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("role", &self.role)
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl Account {
    /// Credential generation captured by a session; changes invalidate it.
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// Independently versioned account store; an empty store is never published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accounts {
    epoch: String,
    records: Vec<Account>,
    api_keys: Vec<ApiKey>,
}

impl Accounts {
    /// Create the first administrator, retaining the existing `local` owner.
    pub fn bootstrap(username: &str, password: &str, now: u64) -> Result<Self, String> {
        Ok(Self {
            epoch: random_token()?,
            records: vec![Account {
                id: crate::user::LOCAL_USER.into(),
                username: login_name(username)?,
                role: Role::Administrator,
                enabled: true,
                created_at: now,
                updated_at: now,
                revision: 1,
                password_hash: hash_password(password)?,
            }],
            api_keys: Vec::new(),
        })
    }

    /// Account metadata in creation order; verifiers remain private.
    pub fn all(&self) -> &[Account] {
        &self.records
    }

    /// Epoch captured by every session; restoring a backup rotates it.
    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    /// Find a canonical login name, accepting an uppercase spelling.
    pub fn find(&self, username: &str) -> Option<&Account> {
        let name = login_name(username).ok()?;
        self.records.iter().find(|account| account.username == name)
    }

    /// Authenticate without distinguishing missing, disabled or wrong-password accounts.
    /// Password work must be scheduled on a bounded blocking worker by HTTP callers.
    pub fn authenticate(&self, username: &str, password: &str) -> Option<&Account> {
        if password.len() > MAX_PASSWORD_BYTES {
            return None;
        }
        let account = self.find(username);
        let hash = account.map_or(DUMMY_HASH, |account| account.password_hash.as_str());
        let parsed = PasswordHash::new(hash).ok()?;
        let matches = hasher()
            .ok()?
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
        account.filter(|account| account.enabled && matches)
    }

    /// Validate the owner and generation captured by a session after a reload.
    pub fn session_account(&self, owner: &str, epoch: &str, revision: u64) -> Option<&Account> {
        if self.epoch != epoch {
            return None;
        }
        self.records
            .iter()
            .find(|account| account.id == owner && account.enabled && account.revision == revision)
    }

    /// Add an enabled account with a generated, immutable personal owner.
    pub fn create(
        &mut self,
        username: &str,
        password: &str,
        role: Role,
        now: u64,
    ) -> Result<(), String> {
        let username = login_name(username)?;
        if self.records.len() >= MAX_ACCOUNTS {
            return Err("the installation already has 256 accounts".into());
        }
        if self.find(&username).is_some() {
            return Err("this login name is already in use".into());
        }
        let id = format!("account-{}", random_token()?);
        if self.records.iter().any(|account| account.id == id) {
            return Err("could not generate a unique account owner; retry".into());
        }
        self.records.push(Account {
            id,
            username,
            role,
            enabled: true,
            created_at: now,
            updated_at: now,
            revision: 1,
            password_hash: hash_password(password)?,
        });
        Ok(())
    }

    /// Change privileges while retaining at least one enabled administrator.
    pub fn set_role(&mut self, username: &str, role: Role, now: u64) -> Result<(), String> {
        self.edit(username, now, |account| account.role = role)
    }

    /// Disable or enable access, preserving every personal record.
    pub fn set_enabled(&mut self, username: &str, enabled: bool, now: u64) -> Result<(), String> {
        self.edit(username, now, |account| account.enabled = enabled)
    }

    /// Rename a login without moving personal data.
    pub fn rename(&mut self, username: &str, replacement: &str, now: u64) -> Result<(), String> {
        let replacement = login_name(replacement)?;
        if self
            .find(&replacement)
            .is_some_and(|other| !other.username.eq_ignore_ascii_case(username))
        {
            return Err("this login name is already in use".into());
        }
        self.edit(username, now, |account| account.username = replacement)
    }

    /// Replace a salted password verifier and invalidate this account's sessions and API keys.
    pub fn set_password(&mut self, username: &str, password: &str, now: u64) -> Result<(), String> {
        if self.find(username).is_none() {
            return Err("no account has this login name".into());
        }
        let hash = hash_password(password)?;
        self.edit(username, now, |account| account.password_hash = hash)
    }

    /// Revoke one account's sessions and API keys without changing its password or opinions.
    pub fn revoke(&mut self, username: &str, now: u64) -> Result<(), String> {
        let account = self
            .records
            .iter_mut()
            .find(|account| account.username.eq_ignore_ascii_case(username))
            .ok_or("no account has this login name")?;
        account.revision = next_revision(account.revision)?;
        account.updated_at = now;
        let owner = account.id.clone();
        self.purge_api_keys(&owner);
        Ok(())
    }

    /// Invalidate all sessions and API keys, including credentials from before a backup restore.
    pub fn revoke_all(&mut self) -> Result<(), String> {
        self.epoch = random_token()?;
        self.api_keys.clear();
        Ok(())
    }

    fn edit(
        &mut self,
        username: &str,
        now: u64,
        edit: impl FnOnce(&mut Account),
    ) -> Result<(), String> {
        let index = self
            .records
            .iter()
            .position(|account| account.username.eq_ignore_ascii_case(username))
            .ok_or("no account has this login name")?;
        let mut candidate = self.records[index].clone();
        edit(&mut candidate);
        if candidate == self.records[index] {
            return Ok(());
        }
        if !(candidate.enabled && candidate.role == Role::Administrator)
            && !self.records.iter().enumerate().any(|(other, account)| {
                other != index && account.enabled && account.role == Role::Administrator
            })
        {
            return Err("keep at least one enabled administrator".into());
        }
        candidate.revision = next_revision(candidate.revision)?;
        candidate.updated_at = now;
        let owner = candidate.id.clone();
        self.records[index] = candidate;
        self.purge_api_keys(&owner);
        Ok(())
    }
}

fn next_revision(previous: u64) -> Result<u64, String> {
    previous
        .checked_add(1)
        .filter(|next| *next <= MAX_REVISION)
        .ok_or_else(|| "account generation is exhausted".into())
}

/// Validate and canonicalize a login name without treating it as a path or owner.
/// Accept 1–64 ASCII letters, digits, dots, underscores or hyphens, with at
/// least one letter or digit. The canonical spelling is lowercase.
pub fn login_name(name: &str) -> Result<String, String> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || !name.bytes().any(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(
            "login names need 1–64 ASCII letters, digits, dots, underscores or hyphens, including at least one letter or digit".into(),
        );
    }
    Ok(name.to_ascii_lowercase())
}

fn hasher() -> Result<Argon2<'static>, String> {
    let params =
        Params::new(MEMORY_KIB, 2, 1, Some(32)).map_err(|_| "invalid password hash policy")?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

fn hash_password(password: &str) -> Result<String, String> {
    if password.chars().count() < 15
        || password.len() > MAX_PASSWORD_BYTES
        || password.contains('\0')
    {
        return Err(
            "passwords need at least 15 characters and at most 1024 UTF-8 bytes, without NUL"
                .into(),
        );
    }
    let mut bytes = [0_u8; 16];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| "secure random generation failed")?;
    let salt = SaltString::encode_b64(&bytes).map_err(|_| "could not encode password salt")?;
    hasher()?
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| "password hashing failed".into())
}

/// Generate an opaque 256-bit secret from the operating system's secure RNG.
/// Failures are reported; there is no timestamp or deterministic fallback.
pub fn random_token() -> Result<String, String> {
    use std::fmt::Write;
    let mut bytes = [0_u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| "secure random generation failed")?;
    let mut token = String::with_capacity(64);
    for byte in bytes {
        write!(&mut token, "{byte:02x}").map_err(|_| "could not encode random token")?;
    }
    Ok(token)
}

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
    let mut root = Json::obj();
    root.set("format_version", FORMAT_VERSION.into());
    root.set("epoch", accounts.epoch.clone().into());
    root.set(
        "account",
        Json::Arr(
            accounts
                .records
                .iter()
                .map(|account| {
                    let mut row = Json::obj();
                    row.set("id", account.id.clone().into());
                    row.set("username", account.username.clone().into());
                    row.set("role", account.role.as_str().into());
                    row.set("enabled", account.enabled.into());
                    row.set("created_at", account.created_at.into());
                    row.set("updated_at", account.updated_at.into());
                    row.set("revision", account.revision.into());
                    row.set("password_hash", account.password_hash.clone().into());
                    row
                })
                .collect(),
        ),
    );
    if !accounts.api_keys.is_empty() {
        root.set("api_keys", keys::to_json(&accounts.api_keys));
    }
    root
}

fn valid_token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_hash(hash: &str) -> bool {
    if hash.len() > 256 {
        return false;
    }
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    let Ok(params) = Params::try_from(&parsed) else {
        return false;
    };
    parsed.algorithm.as_str() == "argon2id"
        && parsed.version == Some(19)
        && params.m_cost() == MEMORY_KIB
        && params.t_cost() == 2
        && params.p_cost() == 1
        && parsed.hash.is_some_and(|hash| hash.len() == 32)
        && parsed.salt.is_some_and(|salt| salt.len() == 22)
        && parsed.params.iter().count() == 3
}

/// Refuse malformed, duplicate or administrator-less credentials as one whole store.
/// Stored hash parameters are fixed to the bounded policy supported by this format.
pub fn from_json(root: &Json) -> Result<Accounts, StoreError> {
    let invalid = || StoreError::AccountsInvalid("malformed account store or unsupported version");
    if root.field_u32("format_version") != Some(FORMAT_VERSION) {
        return Err(invalid());
    }
    let epoch = root
        .get("epoch")
        .and_then(Json::as_str)
        .filter(|epoch| valid_token(epoch))
        .ok_or_else(invalid)?;
    let rows = root
        .get("account")
        .and_then(Json::as_arr)
        .filter(|rows| !rows.is_empty() && rows.len() <= MAX_ACCOUNTS)
        .ok_or_else(invalid)?;
    let mut records = Vec::with_capacity(rows.len());
    let mut names = BTreeSet::new();
    let mut owners = BTreeSet::new();
    for row in rows {
        let string = |key| row.get(key).and_then(Json::as_str).ok_or_else(invalid);
        let number = |key| {
            row.get(key)
                .and_then(Json::as_u64)
                .filter(|n| *n <= MAX_REVISION)
                .ok_or_else(invalid)
        };
        let id = string("id")?;
        if id != crate::user::LOCAL_USER && !id.strip_prefix("account-").is_some_and(valid_token) {
            return Err(invalid());
        }
        let username = string("username")?;
        if login_name(username).ok().as_deref() != Some(username)
            || !names.insert(username)
            || !owners.insert(id)
        {
            return Err(invalid());
        }
        let hash = string("password_hash")?;
        if !valid_hash(hash) {
            return Err(invalid());
        }
        let revision = number("revision")?;
        if revision == 0 {
            return Err(invalid());
        }
        records.push(Account {
            id: id.into(),
            username: username.into(),
            role: Role::parse(string("role")?).ok_or_else(invalid)?,
            enabled: row
                .get("enabled")
                .and_then(Json::as_bool)
                .ok_or_else(invalid)?,
            created_at: number("created_at")?,
            updated_at: number("updated_at")?,
            revision,
            password_hash: hash.into(),
        });
    }
    if !records
        .iter()
        .any(|account| account.enabled && account.role == Role::Administrator)
    {
        return Err(invalid());
    }
    let api_keys = keys::from_json(root.get("api_keys"), &records)?;
    Ok(Accounts {
        epoch: epoch.into(),
        records,
        api_keys,
    })
}

#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "accounts_test_support.rs"]
pub(super) mod test_support;
