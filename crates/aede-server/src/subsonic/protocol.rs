//! JSON/XML envelopes, safe XML text and stable opaque catalog identifiers.

use std::fmt::Write;

use super::*;

#[derive(Clone, Copy)]
pub(super) enum Format {
    Xml,
    Json,
}

#[derive(Debug)]
pub(crate) struct ProtocolError {
    pub(crate) code: u32,
    pub(crate) message: String,
}

impl ProtocolError {
    pub(crate) fn new(code: u32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub(super) fn response(format: Format, result: Result<Value, ProtocolError>) -> Response {
    let (status, payload) = match result {
        Ok(payload) => ("ok", payload),
        Err(failure) => (
            "failed",
            json!({"error": {"code": failure.code, "message": failure.message}}),
        ),
    };
    let mut envelope = json!({"status": status, "version": "1.16.1", "type": "Aede",
        "serverVersion": env!("CARGO_PKG_VERSION"), "openSubsonic": true});
    if let (Some(envelope), Some(payload)) = (envelope.as_object_mut(), payload.as_object()) {
        envelope.extend(
            payload
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
    }
    let mut response = match format {
        Format::Json => Json(json!({"subsonic-response": envelope})).into_response(),
        Format::Xml => {
            let mut output = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
            xml_element(&mut output, "subsonic-response", &envelope, true);
            (
                [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
                output,
            )
                .into_response()
        }
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        header::HeaderName::from_static("x-content-type-options"),
        axum::http::HeaderValue::from_static("nosniff"),
    );
    response
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

// Element/attribute names come only from our payload builders, never request
// or tag names. Strings are escaped and invalid XML 1.0 controls replaced.
fn escaped(output: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            '\t' => output.push_str("&#9;"),
            '\n' => output.push_str("&#10;"),
            '\r' => output.push_str("&#13;"),
            '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}' => {
                output.push(ch)
            }
            _ => output.push('\u{fffd}'),
        }
    }
}

fn xml_element(output: &mut String, name: &str, value: &Value, root: bool) {
    if let Value::Array(items) = value {
        for item in items {
            xml_element(output, name, item, false);
        }
        return;
    }
    output.push('<');
    output.push_str(name);
    if root {
        output.push_str(" xmlns=\"http://subsonic.org/restapi\"");
    }
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if key == "value" {
                continue;
            }
            if let Some(text) = scalar(value) {
                output.push(' ');
                output.push_str(key);
                output.push_str("=\"");
                escaped(output, &text);
                output.push('"');
            }
        }
    }
    output.push('>');
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if key == "value" {
                if let Some(text) = scalar(value) {
                    escaped(output, &text);
                }
            } else if value.is_object() || value.is_array() {
                xml_element(output, key, value, false);
            }
        }
    } else if let Some(text) = scalar(value) {
        escaped(output, &text);
    }
    output.push_str("</");
    output.push_str(name);
    output.push('>');
}

pub(super) fn sha256(text: &str) -> Result<String, ProtocolError> {
    // Rustls already exposes the approved provider's SHA-256 implementation.
    // Use the named suite so algorithm/provider defaults cannot change IDs.
    let suite = rustls::crypto::ring::cipher_suite::TLS13_AES_128_GCM_SHA256
        .tls13()
        .ok_or_else(|| ProtocolError::new(0, "SHA-256 provider unavailable"))?;
    let digest = suite.common.hash_provider.hash(text.as_bytes());
    let mut output = String::with_capacity(64);
    for byte in digest.as_ref() {
        write!(&mut output, "{byte:02x}")
            .map_err(|_| ProtocolError::new(0, "could not encode identifier"))?;
    }
    Ok(output)
}

pub(super) fn opaque_id(
    catalog: &Catalog,
    kind: EntityKind,
    id: Id,
) -> Result<String, ProtocolError> {
    let reference = EntityRef::of(catalog, kind, id)
        .ok_or_else(|| ProtocolError::new(70, "entity not found"))?;
    Ok(format!(
        "{}-{}",
        kind.as_str(),
        sha256(&reference.to_token())?
    ))
}

/// UTC snapshot timestamp; the catalog has no independent album-added date.
pub(super) fn utc_date(seconds: u64) -> Result<String, ProtocolError> {
    // Gregorian civil-date conversion from Unix days. The accepted range
    // limits intermediate arithmetic and yields XML Schema's four-digit year.
    if seconds > 253_402_300_799 {
        return Err(ProtocolError::new(0, "catalog timestamp is out of range"));
    }
    let days = seconds / 86_400;
    let z = days as i64 + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let time = seconds % 86_400;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time / 60 % 60,
        time % 60
    ))
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
