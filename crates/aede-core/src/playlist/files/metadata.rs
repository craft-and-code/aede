use super::*;
use md5::{Digest, Md5};

pub(super) fn metadata(path: &Path) -> Result<File, Error> {
    let text = path
        .to_str()
        .filter(|text| text.len() <= 4096)
        .ok_or(Error::InvalidPath)?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(Error::InvalidPath)?;
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or(Error::InvalidPath)?;
    // Two domain-separated existing digests form a selector, never an access token.
    let digest = |domain: &[u8]| {
        let mut hash = Md5::new();
        hash.update(domain);
        hash.update(text.as_bytes());
        format!("{:x}", hash.finalize())
    };
    Ok(File {
        id: format!(
            "m3u-file-{}{}",
            digest(b"aede:m3u:path:1\0"),
            digest(b"aede:m3u:path:2\0")
        ),
        name: name.into(),
        filename: filename.into(),
        path: text.into(),
    })
}
