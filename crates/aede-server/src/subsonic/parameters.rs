//! Strict, bounded UTF-8 URL/form parsing without lossy percent decoding.

use std::collections::BTreeMap;

use super::*;

const MAX_BYTES: usize = 16 * 1024;
const COMMON: &[&str] = &["apiKey", "u", "p", "t", "s", "v", "c", "f", "callback"];

#[derive(Default)]
pub(super) struct Parameters {
    fields: BTreeMap<String, Vec<String>>,
    occurrences: usize,
    repeated: &'static [&'static str],
}

impl Parameters {
    pub(super) fn get(&self, name: &str) -> Option<&str> {
        self.values(name).first().map(String::as_str)
    }

    pub(super) fn values(&self, name: &str) -> &[String] {
        self.fields.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    fn for_method(method: &str) -> Self {
        let repeated: &'static [&'static str] = match method {
            "star" | "unstar" => &["id", "albumId", "artistId"],
            "createPlaylist" => &["songId"],
            "updatePlaylist" => &["songIdToAdd", "songIndexToRemove"],
            "scrobble" => &["id", "time"],
            _ => &[],
        };
        Self {
            repeated,
            ..Self::default()
        }
    }

    pub(super) fn required(&self, name: &str) -> Result<&str, ProtocolError> {
        self.get(name)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ProtocolError::new(10, format!("{name} is required")))
    }

    pub(super) fn number(
        &self,
        name: &str,
        default: usize,
        maximum: usize,
    ) -> Result<usize, ProtocolError> {
        match self.get(name) {
            None => Ok(default),
            Some(value) if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                value
                    .parse::<usize>()
                    .ok()
                    .filter(|number| *number <= maximum)
                    .ok_or_else(|| ProtocolError::new(10, format!("{name} is out of range")))
            }
            _ => Err(ProtocolError::new(
                10,
                format!("{name} must be a non-negative integer"),
            )),
        }
    }

    pub(super) fn allowed(&self, names: &[&str]) -> Result<(), ProtocolError> {
        if self
            .fields
            .keys()
            .any(|name| !COMMON.contains(&name.as_str()) && !names.contains(&name.as_str()))
        {
            return Err(ProtocolError::new(10, "unsupported parameter"));
        }
        Ok(())
    }

    pub(super) fn format(&self) -> Result<Format, ProtocolError> {
        if self.get("callback").is_some() {
            return Err(ProtocolError::new(0, "JSONP is not supported"));
        }
        match self.get("f").unwrap_or("xml") {
            "xml" => Ok(Format::Xml),
            "json" => Ok(Format::Json),
            _ => Err(ProtocolError::new(0, "response format must be xml or json")),
        }
    }

    pub(super) fn common(&self, discovery: bool) -> Result<(), ProtocolError> {
        if !discovery {
            self.required("c")?;
            let version = self.required("v")?;
            let parts: Vec<_> = version.split('.').collect();
            if parts.len() != 3
                || parts
                    .iter()
                    .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
            {
                return Err(ProtocolError::new(10, "v must be a three-part API version"));
            }
            let numbers: Option<Vec<u32>> = parts.iter().map(|part| part.parse().ok()).collect();
            let numbers = numbers.ok_or_else(|| ProtocolError::new(10, "invalid API version"))?;
            // Subsonic compatibility depends on major/minor only. The patch
            // component must still be a valid number, but does not gate access.
            if numbers[0] > 1 || numbers[0] == 1 && numbers[1] > 16 {
                return Err(ProtocolError::new(
                    30,
                    "client requires a newer Subsonic API",
                ));
            }
            if numbers[0] != 1 {
                return Err(ProtocolError::new(20, "client API version is unsupported"));
            }
        }
        Ok(())
    }

    fn append(&mut self, encoded: &str) -> Result<(), ProtocolError> {
        if encoded.is_empty() {
            return Ok(());
        }
        for part in encoded.split('&') {
            let (name, value) = part.split_once('=').unwrap_or((part, ""));
            let name = decode(name)?;
            let value = decode(value)?;
            if name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || value.len() > 2048
                || self.occurrences >= 256
                || (!self.fields.contains_key(&name) && self.fields.len() >= 32)
                || (self.fields.contains_key(&name) && !self.repeated.contains(&name.as_str()))
            {
                return Err(ProtocolError::new(
                    10,
                    "invalid, oversized or duplicate parameter",
                ));
            }
            self.fields.entry(name).or_default().push(value);
            self.occurrences += 1;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        let mut parameters = Self::default();
        for (name, value) in pairs {
            parameters
                .fields
                .entry(name.to_string())
                .or_default()
                .push(value.to_string());
        }
        parameters.occurrences = pairs.len();
        parameters
    }
}

pub(super) async fn read(request: Request, method: &str) -> Result<Parameters, ProtocolError> {
    let (parts, body) = request.into_parts();
    let query = parts.uri.query().unwrap_or("");
    if query.len() > MAX_BYTES {
        return Err(ProtocolError::new(10, "parameters exceed 16 KiB"));
    }
    let mut parameters = Parameters::for_method(method);
    parameters.append(query)?;
    let bytes = tokio::time::timeout(
        Duration::from_secs(1),
        to_bytes(body, MAX_BYTES - query.len()),
    )
    .await
    .map_err(|_| ProtocolError::new(10, "request body timed out"))?
    .map_err(|_| ProtocolError::new(10, "request body exceeds 16 KiB"))?;
    if parts.method == axum::http::Method::POST {
        let mut values = parts.headers.get_all(header::CONTENT_TYPE).iter();
        let media = values
            .next()
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if values.next().is_some()
            || !media.split(';').next().is_some_and(|value| {
                value
                    .trim()
                    .eq_ignore_ascii_case("application/x-www-form-urlencoded")
            })
        {
            return Err(ProtocolError::new(
                10,
                "POST requires application/x-www-form-urlencoded",
            ));
        }
        parameters.append(
            std::str::from_utf8(&bytes)
                .map_err(|_| ProtocolError::new(10, "form must be UTF-8"))?,
        )?;
    } else if !bytes.is_empty() {
        return Err(ProtocolError::new(10, "GET and HEAD do not accept a body"));
    }
    Ok(parameters)
}

fn decode(encoded: &str) -> Result<String, ProtocolError> {
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut input = encoded.bytes();
    while let Some(byte) = input.next() {
        match byte {
            b'+' => bytes.push(b' '),
            b'%' => {
                let high = input.next().and_then(|byte| (byte as char).to_digit(16));
                let low = input.next().and_then(|byte| (byte as char).to_digit(16));
                let (Some(high), Some(low)) = (high, low) else {
                    return Err(ProtocolError::new(10, "invalid percent encoding"));
                };
                bytes.push((high * 16 + low) as u8);
            }
            _ => bytes.push(byte),
        }
    }
    String::from_utf8(bytes).map_err(|_| ProtocolError::new(10, "parameters must be UTF-8"))
}

#[cfg(test)]
#[path = "parameters_tests.rs"]
mod tests;
