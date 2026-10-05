//! Bounded explicit DNS-SD query for _googlecast._tcp.local on one IPv4 interface.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::{Instant, timeout};

use super::DeviceDescription;

const SERVICE: &str = "_googlecast._tcp.local";

fn name(packet: &[u8], offset: &mut usize) -> Result<String, String> {
    let mut cursor = *offset;
    let mut jumped = false;
    let mut output = String::new();
    for _ in 0..128 {
        let size = *packet.get(cursor).ok_or("truncated DNS name")?;
        cursor += 1;
        if size & 0xc0 == 0xc0 {
            let low = *packet.get(cursor).ok_or("truncated DNS pointer")?;
            let target = (usize::from(size & 63) << 8) | usize::from(low);
            // RFC 1035 pointers refer backwards; this also refuses loops.
            if target >= cursor - 1 {
                return Err("invalid DNS compression pointer".into());
            }
            if !jumped {
                *offset = cursor + 1;
                jumped = true;
            }
            cursor = target;
            continue;
        }
        if size > 63 {
            return Err("invalid DNS label".into());
        }
        if size == 0 {
            if !jumped {
                *offset = cursor;
            }
            return Ok(output);
        }
        let end = cursor
            .checked_add(usize::from(size))
            .ok_or("DNS name too large")?;
        let label = packet.get(cursor..end).ok_or("truncated DNS label")?;
        if !label
            .iter()
            .all(|byte| byte.is_ascii_graphic() && *byte != b'.' && *byte != b'\\')
        {
            return Err("invalid DNS service label".into());
        }
        if !output.is_empty() {
            output.push('.');
        }
        output.push_str(std::str::from_utf8(label).map_err(|_| "invalid DNS label")?);
        if output.len() > 253 {
            return Err("DNS name exceeds its limit".into());
        }
        cursor = end;
    }
    Err("DNS name exceeds its pointer budget".into())
}

fn word(packet: &[u8], offset: &mut usize) -> Result<u16, String> {
    let end = offset.checked_add(2).ok_or("DNS offset overflow")?;
    let bytes: [u8; 2] = packet
        .get(*offset..end)
        .ok_or("truncated DNS word")?
        .try_into()
        .map_err(|_| "invalid DNS word")?;
    *offset = end;
    Ok(u16::from_be_bytes(bytes))
}

fn friendly(input: &[u8]) -> Result<Option<String>, String> {
    let mut offset = 0;
    let mut found = None;
    while offset < input.len() {
        let size = usize::from(input[offset]);
        offset += 1;
        let end = offset.checked_add(size).ok_or("DNS TXT overflow")?;
        let text = input.get(offset..end).ok_or("truncated DNS TXT")?;
        offset = end;
        if let Some(value) = text.strip_prefix(b"fn=") {
            if found.is_some() {
                return Err("duplicate Cast friendly name".into());
            }
            let value = std::str::from_utf8(value).map_err(|_| "invalid Cast friendly name")?;
            if value.is_empty() || value.chars().any(char::is_control) {
                return Err("invalid Cast friendly name".into());
            }
            found = Some(value.into());
        }
    }
    Ok(found)
}

fn parse(packet: &[u8], peer: SocketAddr) -> Result<Vec<DeviceDescription>, String> {
    let SocketAddr::V4(peer) = peer else {
        return Err("Cast discovery only supports IPv4".into());
    };
    super::validate_ip(*peer.ip())?;
    if peer.port() != 5353 || packet.len() > 8192 {
        return Err("invalid mDNS response source or size".into());
    }
    let mut offset = 0;
    if word(packet, &mut offset)? != 0 {
        return Err("unexpected mDNS transaction".into());
    }
    let flags = word(packet, &mut offset)?;
    if flags & 0xf80f != 0x8000 || flags & 0x0200 != 0 {
        return Err("invalid mDNS response flags".into());
    }
    let questions = word(packet, &mut offset)?;
    let records = usize::from(word(packet, &mut offset)?)
        + usize::from(word(packet, &mut offset)?)
        + usize::from(word(packet, &mut offset)?);
    if questions > 16 || records > 128 {
        return Err("mDNS record budget exceeded".into());
    }
    for _ in 0..questions {
        name(packet, &mut offset)?;
        word(packet, &mut offset)?;
        word(packet, &mut offset)?;
    }
    let mut instances = BTreeSet::new();
    let mut targets = BTreeMap::new();
    let mut addresses = BTreeSet::new();
    let mut names = BTreeMap::new();
    for _ in 0..records {
        let owner = name(packet, &mut offset)?.to_ascii_lowercase();
        let kind = word(packet, &mut offset)?;
        let class = word(packet, &mut offset)? & 0x7fff;
        let ttl_end = offset.checked_add(4).ok_or("DNS offset overflow")?;
        let ttl = packet.get(offset..ttl_end).ok_or("truncated DNS TTL")?;
        let live = ttl.iter().any(|byte| *byte != 0);
        offset = ttl_end;
        let size = usize::from(word(packet, &mut offset)?);
        let end = offset.checked_add(size).ok_or("DNS record overflow")?;
        let data = packet.get(offset..end).ok_or("truncated DNS record")?;
        if class == 1 && live {
            match kind {
                12 if owner == SERVICE => {
                    let mut at = offset;
                    let instance = name(packet, &mut at)?.to_ascii_lowercase();
                    if at != end || !instance.ends_with(&format!(".{SERVICE}")) {
                        return Err("invalid Cast PTR".into());
                    }
                    instances.insert(instance);
                }
                33 => {
                    let mut at = offset;
                    let priority = word(packet, &mut at)?;
                    word(packet, &mut at)?;
                    let port = word(packet, &mut at)?;
                    let target = name(packet, &mut at)?.to_ascii_lowercase();
                    if at != end || priority != 0 || port == 0 || !target.ends_with(".local") {
                        return Err("invalid Cast SRV".into());
                    }
                    if targets.insert(owner.clone(), (target, port)).is_some() {
                        return Err("duplicate Cast SRV".into());
                    }
                }
                1 if data == peer.ip().octets() => {
                    addresses.insert(owner.clone());
                }
                16 => {
                    if let Some(value) = friendly(data)?
                        && names.insert(owner.clone(), value).is_some()
                    {
                        return Err("duplicate Cast TXT".into());
                    }
                }
                _ => {}
            }
        }
        offset = end;
    }
    if offset != packet.len() {
        return Err("trailing mDNS data".into());
    }
    Ok(instances
        .into_iter()
        .filter_map(|instance| {
            let (target, port) = targets.get(&instance)?;
            if !addresses.contains(target) {
                return None;
            }
            Some(DeviceDescription {
                name: names.get(&instance).cloned().unwrap_or(instance),
                location: format!("{}:{port}", peer.ip()),
                protocols: vec!["googlecast".into()],
            })
        })
        .collect())
}

fn query() -> Vec<u8> {
    let mut output = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in SERVICE.split('.') {
        output.push(label.len() as u8);
        output.extend_from_slice(label.as_bytes());
    }
    output.extend_from_slice(&[0, 0, 12, 0x80, 1]); // PTR, IN with unicast response requested.
    output
}

pub(super) async fn discover(bind: Ipv4Addr) -> Result<Vec<DeviceDescription>, String> {
    let socket = UdpSocket::bind(SocketAddrV4::new(bind, 0))
        .await
        .map_err(|error| format!("Cast discovery: {error}"))?;
    socket
        .set_multicast_ttl_v4(255)
        .map_err(|error| error.to_string())?;
    socket
        .send_to(&query(), "224.0.0.251:5353")
        .await
        .map_err(|error| format!("Cast discovery: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut buffer = [0; 8193];
    let mut found = BTreeMap::new();
    for _ in 0..256 {
        let received = timeout(
            deadline.saturating_duration_since(Instant::now()),
            socket.recv_from(&mut buffer),
        )
        .await;
        let (size, peer) = match received {
            Err(_) => break,
            Ok(Err(error)) => return Err(format!("Cast discovery: {error}")),
            Ok(Ok(value)) => value,
        };
        if let Ok(devices) = parse(&buffer[..size], peer) {
            for device in devices {
                if found.len() >= 32 && !found.contains_key(&device.location) {
                    break;
                }
                found.insert(device.location.clone(), device);
            }
        }
        if found.len() >= 32 || Instant::now() >= deadline {
            break;
        }
    }
    Ok(found.into_values().collect())
}

#[cfg(test)]
#[path = "cast_discovery_tests.rs"]
mod tests;
