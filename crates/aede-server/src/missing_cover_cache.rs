//! Separate archive cache for unowned release groups; music folders are never targets.
use crate::subsonic::artwork::{Source, read_image};
use aede_core::coverart::{self, RenderedImage, Size};
use aede_core::json::Json;
use std::path::{Path, PathBuf};

pub(super) fn valid_group(group: &str) -> bool {
    group.len() == 36
        && group.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn folder(data: &Path, group: &str) -> Result<PathBuf, String> {
    if !valid_group(group) {
        return Err("invalid release-group UUID".into());
    }
    let root = data.join("missing-coverartarchive");
    let target = root.join(group.to_ascii_lowercase());
    for directory in [&root, &target] {
        match std::fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err("unavailable archive cache directory".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(target.clone()),
            Err(_) => return Err("unreadable archive cache directory".into()),
        }
    }
    Ok(target)
}
pub(super) fn cached(data: &Path, group: &str) -> Result<Option<RenderedImage>, String> {
    let folder = folder(data, group)?;
    for extension in ["jpg", "png"] {
        let path = folder.join(format!("cover.{extension}"));
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {
                return read_image(Source { path, folder }, Some(512))
                    .map(Some)
                    .map_err(|_| "invalid cached archive image".into());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("unreadable cached archive image".into()),
        }
    }
    Ok(None)
}
trait Archive {
    fn direct_front(&mut self, _group: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
    fn index(&mut self, group: &str) -> Result<Option<Json>, String>;
    fn image(&mut self, url: &str) -> Result<Vec<u8>, String>;
}
fn archive_image(url: &str) -> bool {
    // Only archive-selected HTTPS media authorities are accepted, never client URLs.
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = rest.split_once('/') else {
        return false;
    };
    (host == "coverartarchive.org" || host == "archive.org" || host.ends_with(".archive.org"))
        && !host.contains(['@', ':', '?', '#'])
        && !path.is_empty()
}
fn obtain(
    data: &Path,
    group: &str,
    archive: &mut dyn Archive,
) -> Result<Option<RenderedImage>, String> {
    if let Some(image) = cached(data, group)? {
        return Ok(Some(image));
    }
    let bytes = match archive.direct_front(group)? {
        Some(bytes) => bytes,
        None => {
            let Some(index) = archive.index(group)? else {
                return Ok(None);
            };
            coverart::validate_index(&index)?;
            let Some(front) = coverart::front(&index, Size::Thumbnail(500)) else {
                return Ok(None);
            };
            let url = front
                .url
                .strip_prefix("http://")
                .map(|rest| format!("https://{rest}"))
                .unwrap_or(front.url);
            if !archive_image(&url) {
                return Err("unsupported archive image authority".into());
            }
            archive.image(&url)?
        }
    };
    let rendered = coverart::render_image(&bytes, Some(512))?;
    let target = folder(data, group)?;
    std::fs::create_dir_all(&target).map_err(|_| "archive cache cannot be created")?;
    folder(data, group)?;
    coverart::write_beside(&target, &bytes)?;
    Ok(Some(rendered))
}
#[derive(Default)]
struct Network {
    #[cfg(feature = "fetch")]
    client: Option<aede_core::http::Client>,
    absent: std::collections::BTreeMap<String, u64>,
}
impl Network {
    #[cfg(feature = "fetch")]
    fn client(&mut self) -> &mut aede_core::http::Client {
        self.client.get_or_insert_with(|| {
            aede_core::http::Client::new(
                aede_core::http::Client::identify(
                    "aede",
                    env!("CARGO_PKG_VERSION"),
                    env!("CARGO_PKG_REPOSITORY"),
                ),
                std::time::Duration::from_secs(1),
            )
        })
    }
}
impl Archive for Network {
    fn direct_front(&mut self, group: &str) -> Result<Option<Vec<u8>>, String> {
        #[cfg(feature = "fetch")]
        {
            let base = coverart::index_url(group);
            // Curated front endpoints avoid an extra JSON dependency; absent thumbnails may use the original.
            for suffix in ["/front-500", "/front"] {
                match self.client().get_bytes(&format!("{base}{suffix}")) {
                    Ok(bytes) => return Ok(Some(bytes)),
                    Err(aede_core::http::Error::Status(404 | 500)) if suffix == "/front-500" => {
                        continue;
                    }
                    Err(aede_core::http::Error::Status(404)) => return Ok(None),
                    Err(error) => return Err(error.to_string()),
                }
            }
            Ok(None)
        }
        #[cfg(not(feature = "fetch"))]
        {
            let _ = group;
            Err("archive fetching is disabled in this build".into())
        }
    }

    fn index(&mut self, group: &str) -> Result<Option<Json>, String> {
        #[cfg(feature = "fetch")]
        {
            let client = self.client();
            match client.get_json(&coverart::index_url(group)) {
                Ok(value) => Ok(Some(value)),
                Err(aede_core::http::Error::Status(404)) => Ok(None),
                Err(error) => Err(error.to_string()),
            }
        }
        #[cfg(not(feature = "fetch"))]
        {
            let _ = group;
            Err("archive fetching is disabled in this build".into())
        }
    }
    fn image(&mut self, url: &str) -> Result<Vec<u8>, String> {
        #[cfg(feature = "fetch")]
        {
            self.client
                .as_mut()
                .ok_or("archive client is unavailable")?
                .get_bytes(url)
                .map_err(|error| error.to_string())
        }
        #[cfg(not(feature = "fetch"))]
        {
            let _ = url;
            Err("archive fetching is disabled in this build".into())
        }
    }
}
/// One rate-limited client per server; the caller also holds a bounded worker permit.
#[derive(Default)]
pub(super) struct CoverService(std::sync::Mutex<Network>);
impl CoverService {
    pub(super) fn fetch(&self, data: &Path, group: &str) -> Result<Option<RenderedImage>, String> {
        if let Some(image) = cached(data, group)? {
            return Ok(Some(image));
        }
        let mut network = self
            .0
            .try_lock()
            .map_err(|_| "archive downloading is busy; retry later")?;
        let now = aede_core::clock::now_seconds();
        if network.absent.get(group).is_some_and(|until| *until > now) {
            return Ok(None);
        }
        let result = obtain(data, group, &mut *network)?;
        if result.is_none() {
            network.absent.retain(|_, until| *until > now);
            if network.absent.len() < 4096 {
                network
                    .absent
                    .insert(group.into(), now.saturating_add(86400));
            }
        }
        Ok(result)
    }
}
#[cfg(test)]
#[path = "missing_cover_cache_tests.rs"]
mod tests;
