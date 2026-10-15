//! Bound JSON expansion in the filesystem worker before publishing any response.

use super::*;
use std::io::{self, Write};

const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
#[derive(Default)]
struct Limited(Vec<u8>);
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_RESPONSE_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("M3U response limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn serialize(answer: &impl Serialize) -> Result<Vec<u8>, ApiError> {
    let mut bytes = Limited::default();
    serde_json::to_writer(&mut bytes, answer)
        .map_err(|_| failure(library_files::Error::TooLarge))?;
    Ok(bytes.0)
}

pub(super) fn json(bytes: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], bytes).into_response()
}
