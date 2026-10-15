//! RSS address identity normalization without network access.
/// Normalize an RSS identity without fetching it or interpreting its contents.
pub fn canonical_feed(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 2048
        || !value.is_ascii()
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
        || value.contains('\\')
    {
        return Err("invalid podcast feed address".into());
    }
    let (scheme, rest) = value
        .split_once("://")
        .ok_or("invalid podcast feed address")?;
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return Err("invalid podcast feed address".into());
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.contains('@') {
        return Err("invalid podcast feed authority".into());
    }
    let (host, port) = if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or("invalid podcast feed authority")?;
        authority[1..end]
            .parse::<std::net::Ipv6Addr>()
            .map_err(|_| "invalid podcast feed authority")?;
        let tail = &authority[end + 1..];
        (
            &authority[..=end],
            if tail.is_empty() {
                None
            } else {
                Some(tail.strip_prefix(':').ok_or("invalid podcast feed port")?)
            },
        )
    } else {
        let (host, port) = authority
            .rsplit_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)));
        let host = host.strip_suffix('.').unwrap_or(host);
        if host.len() > 253
            || host.split('.').any(|part| {
                part.is_empty()
                    || part.len() > 63
                    || part.starts_with('-')
                    || part.ends_with('-')
                    || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err("invalid podcast feed authority".into());
        }
        (host, port)
    };
    let port = port
        .map(|port| {
            port.parse::<u16>()
                .ok()
                .filter(|port| *port > 0)
                .ok_or("invalid podcast feed port")
        })
        .transpose()?;
    let host = if host.starts_with('[') {
        format!(
            "[{}]",
            host[1..host.len() - 1]
                .parse::<std::net::Ipv6Addr>()
                .map_err(|_| "invalid podcast feed authority")?
        )
    } else {
        host.to_ascii_lowercase()
    };
    let mut normalized = format!("{scheme}://{host}");
    if let Some(port) =
        port.filter(|port| !matches!((scheme.as_str(), port), ("http", 80) | ("https", 443)))
    {
        normalized.push_str(&format!(":{port}"));
    }
    let suffix = rest[authority_end..].split('#').next().unwrap_or("");
    let (path, query) = suffix
        .split_once('?')
        .map_or((suffix, None), |(path, query)| (path, Some(query)));
    normalized.push_str(&normalize_path(path)?);
    if let Some(query) = query {
        normalized.push('?');
        normalized.push_str(&normalize_percent(query)?);
    }
    Ok(normalized)
}
fn normalize_percent(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            output.push(bytes[index] as char);
            index += 1;
            continue;
        }
        let pair = bytes
            .get(index + 1..index + 3)
            .ok_or("invalid podcast percent encoding")?;
        let high = (pair[0] as char)
            .to_digit(16)
            .ok_or("invalid podcast percent encoding")?;
        let low = (pair[1] as char)
            .to_digit(16)
            .ok_or("invalid podcast percent encoding")?;
        let byte = (high * 16 + low) as u8;
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            output.push(byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
        index += 3;
    }
    Ok(output)
}
fn normalize_path(value: &str) -> Result<String, String> {
    let value = normalize_percent(value)?;
    let trailing = value.ends_with('/') || value.ends_with("/.") || value.ends_with("/..");
    let mut segments = Vec::new();
    for segment in value.strip_prefix('/').unwrap_or(&value).split('/') {
        match segment {
            "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    let mut output = format!("/{}", segments.join("/"));
    if trailing && !output.ends_with('/') {
        output.push('/');
    }
    Ok(output)
}
#[cfg(test)]
#[path = "feeds_tests.rs"]
mod tests;
