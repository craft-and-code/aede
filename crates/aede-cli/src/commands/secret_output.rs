//! Output-only credential masking shared by direct CLI passes and administrative jobs.

pub(super) fn redact(text: String, secrets: &[String]) -> String {
    let variants = secret_variants(secrets);
    if variants.is_empty() {
        return text;
    }
    // Match only the original text, including overlapping occurrences. Merge
    // their spans so neither a secret suffix nor a reprocessed marker escapes.
    let mut output = String::with_capacity(text.len());
    let mut kept_from = 0;
    for (start, _) in text.char_indices() {
        let Some(length) = variants
            .iter()
            .filter(|secret| text[start..].starts_with(secret.as_str()))
            .map(String::len)
            .max()
        else {
            continue;
        };
        if kept_from == 0 || start > kept_from {
            output.push_str(&text[kept_from..start]);
            output.push_str("[redacted]");
        }
        kept_from = kept_from.max(start + length);
    }
    output.push_str(&text[kept_from..]);
    output
}

pub(super) fn secret_variants(secrets: &[String]) -> Vec<String> {
    let mut variants = Vec::new();
    for secret in secrets {
        for value in [secret.as_str(), secret.trim()] {
            if value.is_empty() {
                continue;
            }
            variants.push(value.to_string());
            // Decode reports serialize diagnostics before capture. Mask their
            // JSON-escaped spelling too, so parsing a report cannot recover a key.
            let quoted = aede_core::json::Json::Str(value.to_string()).to_string_compact();
            variants.push(quoted[1..quoted.len() - 1].to_string());
            // Service keys can also appear in an error's query string. The
            // source adapters trim configuration and percent-encode AcoustID keys.
            let mut encoded = String::new();
            let mut lower_encoded = String::new();
            for byte in value.bytes() {
                if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                    encoded.push(char::from(byte));
                    lower_encoded.push(char::from(byte));
                } else {
                    encoded.push_str(&format!("%{byte:02X}"));
                    lower_encoded.push_str(&format!("%{byte:02x}"));
                }
            }
            variants.push(encoded);
            variants.push(lower_encoded);
        }
    }
    variants.sort_unstable();
    variants.dedup();
    variants
}

#[cfg(test)]
#[path = "secret_output_tests.rs"]
mod tests;
