//! Shared policy for unchanged encoded originals, independent of Subsonic.

use aede_core::tags::AudioProperties;
use hyper::http::{HeaderMap, header};

/// The MIME type of original audio, independent of its private filesystem path.
pub fn content_type(properties: &AudioProperties) -> &'static str {
    match suffix(properties) {
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "aiff" => "audio/aiff",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "wv" => "audio/wavpack",
        _ => "application/octet-stream",
    }
}

/// A conservative filename suffix for the original encoded container.
pub fn suffix(properties: &AudioProperties) -> &'static str {
    let container = if properties.container.is_empty() {
        properties.codec.as_str()
    } else {
        properties.container.as_str()
    };
    match container {
        "flac" => "flac",
        "mp3" => "mp3",
        "ogg" => "ogg",
        "opus" => "opus",
        "wav" | "wave" => "wav",
        "aiff" | "aif" => "aiff",
        "mp4" | "m4a" | "m4b" => "m4a",
        "aac" | "adts" => "aac",
        "wavpack" | "wv" => "wv",
        "ape" => "ape",
        "dsf" => "dsf",
        "dff" => "dff",
        _ => "bin",
    }
}

/// A validated selection within one unchanged original file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteRange {
    /// First included byte in the unchanged original.
    pub start: u64,
    /// Number of included bytes, bounded by the original size.
    pub length: u64,
    /// Whether this selection requires an HTTP 206 response.
    pub partial: bool,
}

/// Select a single HTTP byte range; `None` means an unsatisfiable range.
///
/// Without a published strong validator, If-Range selects the whole file.
/// Duplicate Range or If-Range headers are errors rather than guessed ranges.
/// The caller maps those errors to its protocol and implements HEAD separately.
pub fn requested_range(headers: &HeaderMap, size: u64) -> Result<Option<ByteRange>, &'static str> {
    let mut ranges = headers.get_all(header::RANGE).iter();
    let range = ranges.next();
    if ranges.next().is_some() {
        return Err("Only one Range header is supported");
    }
    let mut conditions = headers.get_all(header::IF_RANGE).iter();
    let conditional = conditions.next().is_some();
    if conditions.next().is_some() {
        return Err("Only one If-Range header is supported");
    }
    // No strong entity validator is published. An unrecognized If-Range
    // cannot authorize appending bytes to a previously downloaded entity.
    if conditional || range.is_none() {
        return Ok(Some(ByteRange {
            start: 0,
            length: size,
            partial: false,
        }));
    }
    let value = range.and_then(|range| range.to_str().ok()).unwrap_or("");
    Ok(parse_range(value, size))
}

fn parse_range(value: &str, size: u64) -> Option<ByteRange> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    if size == 0 || value.contains(',') {
        return None;
    }
    let decimal = |value: &str| {
        (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| value.parse::<u64>().ok())
            .flatten()
    };
    let (start, end) = if start.is_empty() {
        let suffix = decimal(end)?.min(size);
        if suffix == 0 {
            return None;
        }
        (size - suffix, size - 1)
    } else {
        let start = decimal(start)?;
        if start >= size {
            return None;
        }
        let end = if end.is_empty() {
            size - 1
        } else {
            decimal(end)?.min(size - 1)
        };
        if end < start {
            return None;
        }
        (start, end)
    };
    Some(ByteRange {
        start,
        length: end.checked_sub(start)?.checked_add(1)?,
        partial: true,
    })
}

#[cfg(test)]
#[path = "original_tests.rs"]
mod tests;
