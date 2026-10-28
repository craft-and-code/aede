use super::*;
use md5::{Digest, Md5};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
/// Exact scanned source stamp; a cached descriptor is not fresh merely because its name matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputIdentity {
    /// Stable canonical catalogued file path.
    pub path: String,
    /// Original source byte length.
    pub size: u64,
    /// Original source modification time, whole seconds.
    pub modified: u64,
    /// Original source modification-time subsecond nanoseconds.
    pub nanos: u32,
}
/// Complete descriptor for one input. Personal labels never enter this shared acoustic artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// Exact original file identity.
    pub input: InputIdentity,
    /// Validated measurement profile.
    pub features: Features,
}
fn location(root: &Path, input: &InputIdentity) -> PathBuf {
    let token = format!(
        "{}\0{}\0{}\0{}\0{}",
        crate::PCM_DECODER_PROVENANCE,
        input.path,
        input.size,
        input.modified,
        input.nanos
    );
    let hash = Md5::digest(token.as_bytes());
    root.join(format!("acoustic-v{}", aede_moods::PROFILE))
        .join(format!("{hash:x}.json"))
}
fn safe_root(root: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(root).map_err(|_| "acoustic cache root unavailable")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("unsafe acoustic cache root".into());
    }
    Ok(())
}
fn valid_input(input: &InputIdentity) -> bool {
    Path::new(&input.path).is_absolute()
        && input.path.len() <= 4096
        && !input.path.contains('\0')
        && input.nanos < 1_000_000_000
}
/// Read at most eight KiB, refusing symlinks, malformed records and a mismatched source/profile.
pub fn load(root: &Path, input: &InputIdentity) -> Result<Option<Record>, String> {
    if !valid_input(input) {
        return Err("invalid acoustic input identity".into());
    }
    let path = location(root, input);
    let parent = path.parent().ok_or("invalid acoustic cache path")?;
    if !parent.exists() {
        return Ok(None);
    }
    safe_root(parent)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("acoustic cache unavailable".into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 8192 {
        return Err("invalid acoustic cache file".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(|_| "acoustic cache unavailable")?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| "acoustic cache read failed")?;
    if bytes.len() > 8192 {
        return Err("acoustic cache too large".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "acoustic cache unreadable")?;
    if value["path"].as_str() != Some(&input.path)
        || value["size"].as_u64() != Some(input.size)
        || value["modified"].as_u64() != Some(input.modified)
        || value["nanos"].as_u64() != Some(u64::from(input.nanos))
        || value.as_object().is_none_or(|o| o.len() != 5)
    {
        return Err("acoustic cache identity mismatch".into());
    }
    let features: Features = serde_json::from_value(value["features"].clone())
        .map_err(|_| "acoustic cache features invalid")?;
    if !features.valid() {
        return Err("acoustic cache profile invalid".into());
    }
    Ok(Some(Record {
        input: input.clone(),
        features,
    }))
}
/// Publish each completed source atomically. Interrupted jobs retain earlier source records.
pub fn save(root: &Path, record: &Record) -> Result<(), String> {
    if !valid_input(&record.input) || !record.features.valid() {
        return Err("invalid acoustic record".into());
    }
    safe_root(root)?;
    let path = location(root, &record.input);
    let parent = path.parent().ok_or("invalid acoustic cache path")?;
    match fs::create_dir(parent) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("acoustic cache directory unavailable".into()),
    };
    safe_root(parent)?;
    let value = serde_json::json!({"path":record.input.path,"size":record.input.size,"modified":record.input.modified,"nanos":record.input.nanos,"features":record.features});
    let bytes = serde_json::to_vec(&value).map_err(|_| "acoustic cache serialization failed")?;
    if bytes.len() > 8192 {
        return Err("acoustic cache too large".into());
    }
    crate::atomic_file::write(&path, &bytes).map_err(|_| "acoustic cache publication failed".into())
}
#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
