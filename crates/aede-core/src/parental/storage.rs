//! Bounded private persistence; callers own the installation writer lock.
use super::intervals::{Interval, union};
use super::{Media, Policy, PolicySnapshot};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const FORMAT_VERSION: u32 = 1;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_REVISION: u64 = (1 << 53) - 1;
pub(super) const RETAIN_MS: u64 = 32 * 86_400_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Reservation {
    pub(super) id: String,
    pub(super) media: Media,
    pub(super) interval: Interval,
    pub(super) consumed_ms: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Owner {
    pub(super) revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) locked_by: Option<String>,
    pub(super) policy: Policy,
    pub(super) settled: Vec<Interval>,
    pub(super) reservations: Vec<Reservation>,
}
/// Protected installation-level parental policies and conservative usage ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParentalStore {
    format_version: u32,
    #[serde(deserialize_with = "unique_owners")]
    pub(super) owners: BTreeMap<String, Owner>,
}
impl Default for ParentalStore {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            owners: BTreeMap::new(),
        }
    }
}
/// The independently versioned, private parental store inside an Aède data directory.
pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("parental.json")
}
pub(super) fn valid_owner(owner: &str) -> bool {
    !owner.is_empty() && owner.len() <= 256 && !owner.chars().any(char::is_control)
}
pub(super) fn valid_lease(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}
impl ParentalStore {
    /// Load a private regular document; only initial absence yields an empty store.
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::load_optional(path).map(Option::unwrap_or_default)
    }
    /// Distinguish initial absence from a present private document at the read boundary.
    /// Servers can latch observed presence and refuse a subsequent disappearance.
    pub fn load_optional(path: &Path) -> Result<Option<Self>, String> {
        let inspected = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(_) => return Err("parental policy could not be read".into()),
        };
        crate::accounts::check_private_file(path)
            .map_err(|_| "parental store must be a private regular file")?;
        let mut file =
            std::fs::File::open(path).map_err(|_| "parental policy could not be opened")?;
        let opened = file
            .metadata()
            .map_err(|_| "parental policy metadata is unavailable")?;
        crate::accounts::validate_private_read(&inspected, &opened)
            .map_err(|_| "parental policy changed while opening")?;
        if opened.len() > MAX_BYTES as u64 {
            return Err("parental store exceeds 8 MiB".into());
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "parental policy read failed")?;
        if bytes.len() > MAX_BYTES {
            return Err("parental store exceeds 8 MiB".into());
        }
        let after = file
            .metadata()
            .map_err(|_| "parental policy metadata is unavailable")?;
        let current =
            std::fs::symlink_metadata(path).map_err(|_| "parental policy changed while reading")?;
        crate::accounts::validate_private_read(&opened, &after)
            .map_err(|_| "parental policy changed while reading")?;
        crate::accounts::validate_private_read(&opened, &current)
            .map_err(|_| "parental policy changed while reading")?;
        if opened.len() != after.len()
            || opened.modified().ok() != after.modified().ok()
            || opened.len() != current.len()
            || opened.modified().ok() != current.modified().ok()
        {
            return Err("parental policy changed while reading".into());
        }
        let store: Self = serde_json::from_slice(&bytes)
            .map_err(|_| "parental store is unreadable or unsupported")?;
        store.validate()?;
        Ok(Some(store))
    }
    /// Publish one validated replacement atomically; the caller holds `StoreLock`.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        crate::accounts::check_private_file(path)
            .map_err(|_| "parental store must remain private")?;
        let mut bytes = Limited(Vec::new());
        serde_json::to_writer(&mut bytes, self)
            .map_err(|_| "parental store exceeds its serialization budget")?;
        crate::atomic_file::write(path, &bytes.0)
            .map_err(|_| "parental store could not be saved".to_owned())
    }
    /// Return only the selected owner's policy, including explicit unrestricted defaults.
    pub fn policy(&self, owner: &str) -> PolicySnapshot {
        let value = self.owners.get(owner);
        PolicySnapshot {
            owner: owner.into(),
            revision: value.map_or(0, |value| value.revision),
            locked_by: value.and_then(|value| value.locked_by.clone()),
            policy: value.map_or_else(Policy::default, |value| value.policy.clone()),
        }
    }
    /// Replace an administrator-approved policy without altering credentials or opinions.
    pub fn set_policy(
        &mut self,
        owner: &str,
        policy: Policy,
        expected_revision: u64,
    ) -> Result<PolicySnapshot, String> {
        self.set_policy_by(owner, None, policy, expected_revision)
    }
    /// Apply an authenticated administrator's edit and claim/release its exclusive lock.
    /// Legacy unclaimed rules may be claimed; a local token cannot override a named owner.
    pub fn set_policy_by(
        &mut self,
        owner: &str,
        administrator: Option<&str>,
        policy: Policy,
        expected_revision: u64,
    ) -> Result<PolicySnapshot, String> {
        if !valid_owner(owner) {
            return Err("invalid parental owner".into());
        }
        let policy = policy.canonicalize()?;
        let old = self.owners.get(owner);
        if administrator.is_some_and(|value| !valid_owner(value) || value == owner)
            || old
                .and_then(|value| value.locked_by.as_deref())
                .is_some_and(|locked| Some(locked) != administrator)
        {
            return Err("parental policy is locked by another administrator".into());
        }
        let locked_by = policy
            .enabled
            .then_some(administrator)
            .flatten()
            .map(str::to_owned);
        if old.map_or(0, |value| value.revision) != expected_revision {
            return Err("parental policy revision conflict".into());
        }
        if old.is_some_and(|value| value.policy == policy && value.locked_by == locked_by) {
            return Ok(self.policy(owner));
        }
        let mut candidate = self.clone();
        let row = candidate.owners.entry(owner.into()).or_default();
        row.revision = row
            .revision
            .checked_add(1)
            .filter(|value| *value <= MAX_REVISION)
            .ok_or("parental policy revision exhausted")?;
        row.policy = policy;
        row.locked_by = locked_by;
        candidate.validate()?;
        *self = candidate;
        Ok(self.policy(owner))
    }
    /// Whether an administrator still owns at least one active supervision lock.
    pub fn supervises(&self, administrator: &str) -> bool {
        self.owners
            .values()
            .any(|owner| owner.locked_by.as_deref() == Some(administrator))
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.format_version != FORMAT_VERSION || self.owners.len() > 256 {
            return Err("unsupported or oversized parental store".into());
        }
        let mut intervals = 0_usize;
        let mut text_bytes = 0_usize;
        for (name, owner) in &self.owners {
            if !valid_owner(name)
                || owner
                    .locked_by
                    .as_deref()
                    .is_some_and(|id| !valid_owner(id) || id == name || !owner.policy.enabled)
                || owner.revision > MAX_REVISION
                || owner.reservations.len() > 8
                || owner.settled.len() > 8192
            {
                return Err("invalid parental owner or ledger bounds".into());
            }
            owner.policy.validate()?;
            text_bytes = text_bytes.saturating_add(
                owner
                    .policy
                    .blocked_artists
                    .iter()
                    .chain(&owner.policy.blocked_podcasts)
                    .map(String::len)
                    .sum::<usize>(),
            );
            intervals += owner.settled.len();
            let mut previous_end = 0;
            for value in &owner.settled {
                if value.start_ms >= value.end_ms
                    || value.end_ms > 253_402_300_799_999
                    || value.end_ms - value.start_ms > RETAIN_MS
                    || value.start_ms < previous_end
                {
                    return Err("invalid parental usage interval".into());
                }
                previous_end = value.end_ms;
            }
            let mut ids = std::collections::BTreeSet::new();
            for value in &owner.reservations {
                if !valid_lease(&value.id)
                    || !ids.insert(&value.id)
                    || value.interval.start_ms >= value.interval.end_ms
                    || value.interval.end_ms > 253_402_300_799_999
                    || value.interval.end_ms - value.interval.start_ms > 10_000
                    || value.consumed_ms > value.interval.end_ms - value.interval.start_ms
                {
                    return Err("invalid parental listening reservation".into());
                }
                if let Media::Podcast { feed } = &value.media {
                    super::canonical_feed(feed)?;
                }
            }
        }
        if intervals > 65_536 || text_bytes > 2 * 1024 * 1024 {
            return Err("parental store exceeds its global budget".into());
        }
        Ok(())
    }
}
impl Owner {
    pub(super) fn intervals(&self) -> Vec<Interval> {
        union(
            self.settled
                .iter()
                .copied()
                .chain(self.reservations.iter().map(|value| value.interval))
                .collect(),
        )
    }
    pub(super) fn retire(&mut self, now_ms: u64) {
        let mut held = Vec::new();
        for reservation in self.reservations.drain(..) {
            if reservation.interval.end_ms <= now_ms {
                self.settled.push(reservation.interval);
            } else {
                held.push(reservation);
            }
        }
        self.reservations = held;
        let cutoff = now_ms.saturating_sub(RETAIN_MS);
        self.settled = union(
            self.settled
                .iter()
                .filter_map(|value| {
                    (value.end_ms > cutoff).then_some(Interval {
                        start_ms: value.start_ms.max(cutoff),
                        end_ms: value.end_ms,
                    })
                })
                .collect(),
        );
    }
}
struct Limited(Vec<u8>);
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_BYTES {
            return Err(std::io::Error::other("parental store byte budget exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;

fn unique_owners<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Owner>, D::Error> {
    struct Owners;
    impl<'de> serde::de::Visitor<'de> for Owners {
        type Value = BTreeMap<String, Owner>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 256 unique parental owners")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut input: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some((owner, value)) = input.next_entry::<String, Owner>()? {
                if values.len() >= 256 || values.insert(owner, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate or excessive parental owners",
                    ));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Owners)
}
