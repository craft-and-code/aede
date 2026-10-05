//! Explicit LAN discovery and original-file UPnP AV/OpenHome control.

use std::collections::BTreeSet;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::{Semaphore, watch};
use tokio::time::{Instant, timeout};

use super::{DeviceProtocol, DeviceTrack};

#[path = "upnp_xml.rs"]
mod xml;

const DESCRIPTION_NAMESPACE: &str = "urn:schemas-upnp-org:device-1-0";
const SOAP_NAMESPACE: &str = "http://schemas.xmlsoap.org/soap/envelope/";
const OPENHOME_PLAYLIST: &str = "urn:av-openhome-org:service:Playlist:1";
const MAX_BODY: usize = 512 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const POLL: Duration = Duration::from_millis(500);
const START_TIMEOUT: Duration = Duration::from_secs(30);

/// One LAN renderer's name, validated endpoint and advertised controls.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceDescription {
    /// Device-supplied friendly name.
    pub name: String,
    /// Same-peer IPv4 HTTP description URL, or a Cast IPv4:port endpoint.
    pub location: String,
    /// Implemented control adapters advertised by the device.
    pub protocols: Vec<String>,
}

#[derive(Clone, Debug)]
struct DeviceUrl {
    ip: Ipv4Addr,
    port: u16,
    path: String,
}

pub(super) fn device_ip(description_url: &str) -> Result<Ipv4Addr, String> {
    Ok(DeviceUrl::parse(description_url)?.ip)
}

fn local_ip(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback() || ip.is_link_local()
}

impl DeviceUrl {
    fn parse(value: &str) -> Result<Self, String> {
        if value.len() > 4096
            || value
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte == b' ' || byte == b'\\')
            || value.contains('#')
        {
            return Err("invalid device HTTP URL".into());
        }
        let rest = value
            .strip_prefix("http://")
            .ok_or("device URL must use HTTP and a literal local IPv4 address")?;
        let split = rest.find(['/', '?']).unwrap_or(rest.len());
        let authority = &rest[..split];
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (
                host,
                port.parse::<u16>()
                    .map_err(|_| "invalid device HTTP port")?,
            ),
            None => (authority, 80),
        };
        let ip = host
            .parse::<Ipv4Addr>()
            .map_err(|_| "device URL needs a literal local IPv4 address")?;
        if !local_ip(ip) || port == 0 || host != ip.to_string() {
            return Err("device URL needs a unicast private, link-local or loopback IPv4 address and nonzero port".into());
        }
        let path = if split == rest.len() {
            "/".into()
        } else if rest.as_bytes()[split] == b'?' {
            format!("/{}", &rest[split..])
        } else {
            rest[split..].into()
        };
        validate_path(&path)?;
        Ok(Self { ip, port, path })
    }

    fn resolve(&self, reference: &str) -> Result<Self, String> {
        let target = if reference.starts_with("http://") {
            Self::parse(reference)?
        } else {
            if reference.is_empty()
                || reference.starts_with("//")
                || reference.contains(':')
                || reference.contains('#')
                || reference.len() > 4096
            {
                return Err("invalid device control URL".into());
            }
            let path = if reference.starts_with('/') {
                reference.to_owned()
            } else {
                let base = self.path.split('?').next().ok_or("invalid base URL")?;
                let slash = base.rfind('/').ok_or("invalid base URL")?;
                format!("{}{reference}", &base[..slash + 1])
            };
            validate_path(&path)?;
            Self {
                ip: self.ip,
                port: self.port,
                path,
            }
        };
        if target.ip != self.ip || target.port != self.port {
            return Err(
                "device description cannot redirect controls to another host or port".into(),
            );
        }
        Ok(target)
    }

    fn location(&self) -> String {
        format!("http://{}:{}{}", self.ip, self.port, self.path)
    }
}

fn validate_path(value: &str) -> Result<(), String> {
    if !value.starts_with('/')
        || value.len() > 4096
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ' || byte == b'\\')
    {
        return Err("invalid device HTTP path".into());
    }
    let path = value.split('?').next().ok_or("invalid device HTTP path")?;
    let mut decoded = Vec::with_capacity(path.len());
    let mut bytes = path.bytes();
    while let Some(byte) = bytes.next() {
        let byte = if byte == b'%' {
            let high = bytes
                .next()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or("invalid device URL escape")?;
            let low = bytes
                .next()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or("invalid device URL escape")?;
            (high * 16 + low) as u8
        } else {
            byte
        };
        if byte.is_ascii_control() || matches!(byte, b'\\' | b'%') {
            return Err("invalid escaped device path".into());
        }
        decoded.push(byte);
    }
    if decoded
        .split(|byte| *byte == b'/')
        .any(|part| part == b".." || part == b".")
    {
        return Err("device URL traversal is forbidden".into());
    }
    Ok(())
}

#[derive(Clone)]
struct Service {
    kind: String,
    control: DeviceUrl,
    device_scope: usize,
}

struct Description {
    name: String,
    services: Vec<Service>,
}

fn description(body: &str, location: &DeviceUrl) -> Result<Description, String> {
    let root = xml::parse(body)?;
    if root.name != "root" || root.namespace != DESCRIPTION_NAMESPACE {
        return Err("not a UPnP device description".into());
    }
    let base = match root.optional("URLBase")? {
        Some(node) => {
            description_namespace(node)?;
            location.resolve(root.value("URLBase")?)?
        }
        None => location.clone(),
    };
    let device = root.child("device")?;
    let name = description_value(device, "friendlyName")?.to_owned();
    if name.len() > 512 {
        return Err("device name exceeds its limit".into());
    }
    let mut services = Vec::new();
    collect_services(device, &base, &mut services, &mut 0)?;
    Ok(Description { name, services })
}

fn collect_services(
    node: &xml::Node,
    base: &DeviceUrl,
    output: &mut Vec<Service>,
    next_scope: &mut usize,
) -> Result<(), String> {
    description_namespace(node)?;
    let device_scope = *next_scope;
    *next_scope += 1;
    if let Some(list) = node.optional("serviceList")? {
        description_namespace(list)?;
        for item in &list.children {
            if item.name != "service" {
                continue;
            }
            description_namespace(item)?;
            let kind = description_value(item, "serviceType")?;
            if service_version(kind, "AVTransport").is_some()
                || service_version(kind, "ConnectionManager").is_some()
                || kind == OPENHOME_PLAYLIST
            {
                if output.len() == 64 {
                    return Err("too many device services".into());
                }
                output.push(Service {
                    kind: kind.into(),
                    control: base.resolve(description_value(item, "controlURL")?)?,
                    device_scope,
                });
            }
        }
    }
    if let Some(list) = node.optional("deviceList")? {
        description_namespace(list)?;
        for item in &list.children {
            if item.name == "device" {
                collect_services(item, base, output, next_scope)?;
            }
        }
    }
    Ok(())
}

fn description_namespace(node: &xml::Node) -> Result<(), String> {
    if node.namespace != DESCRIPTION_NAMESPACE {
        return Err("invalid device description namespace".into());
    }
    Ok(())
}

fn description_value<'a>(node: &'a xml::Node, name: &str) -> Result<&'a str, String> {
    description_namespace(node.child(name)?)?;
    node.value(name)
}

fn service_version(kind: &str, name: &str) -> Option<u8> {
    let version = kind
        .strip_prefix(&format!("urn:schemas-upnp-org:service:{name}:"))?
        .parse::<u8>()
        .ok()?;
    (1..=3).contains(&version).then_some(version)
}

pub(super) async fn discover(bind: Ipv4Addr) -> Result<Vec<DeviceDescription>, String> {
    if !local_ip(bind) {
        return Err("device discovery needs an explicit local IPv4 interface".into());
    }
    let socket = UdpSocket::bind(SocketAddrV4::new(bind, 0))
        .await
        .map_err(|error| format!("device discovery: {error}"))?;
    for target in [
        "urn:schemas-upnp-org:device:MediaRenderer:1",
        OPENHOME_PLAYLIST,
    ] {
        let query = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: {target}\r\n\r\n"
        );
        socket
            .send_to(query.as_bytes(), "239.255.255.250:1900")
            .await
            .map_err(|error| format!("device discovery: {error}"))?;
    }
    let end = Instant::now() + Duration::from_secs(3);
    let mut locations = BTreeSet::new();
    let mut buffer = [0u8; 8193];
    let mut packets = 0;
    while locations.len() < 32 && packets < 256 && Instant::now() < end {
        let received = timeout(
            end.saturating_duration_since(Instant::now()),
            socket.recv_from(&mut buffer),
        )
        .await;
        let (length, peer) = match received {
            Err(_) => break,
            Ok(Err(error)) => return Err(format!("device discovery: {error}")),
            Ok(Ok(value)) => value,
        };
        packets += 1;
        if length > 8192 {
            continue;
        }
        if let Ok(location) = ssdp_location(&buffer[..length], peer) {
            locations.insert(location.location());
        }
    }
    let mut pending = tokio::task::JoinSet::new();
    let slots = Arc::new(Semaphore::new(8));
    for location in locations {
        let slots = slots.clone();
        pending.spawn(async move {
            let _permit = slots
                .acquire_owned()
                .await
                .map_err(|_| "device discovery closed".to_owned())?;
            let target = DeviceUrl::parse(&location)?;
            let body = timeout(Duration::from_secs(3), request(&target, "GET", None, None))
                .await
                .map_err(|_| "device description timed out".to_owned())??;
            let device = description(&body, &target)?;
            let mut protocols = Vec::new();
            if device
                .services
                .iter()
                .any(|service| service_version(&service.kind, "AVTransport").is_some())
            {
                protocols.push("upnp".into());
            }
            if device
                .services
                .iter()
                .any(|service| service.kind == OPENHOME_PLAYLIST)
            {
                protocols.push("openhome".into());
            }
            Ok::<_, String>(DeviceDescription {
                name: device.name,
                location,
                protocols,
            })
        });
    }
    let mut result = Vec::new();
    while let Some(reply) = pending.join_next().await {
        if let Ok(Ok(device)) = reply
            && !device.protocols.is_empty()
        {
            result.push(device);
        }
    }
    result.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.location.cmp(&right.location))
    });
    Ok(result)
}

fn ssdp_location(packet: &[u8], peer: SocketAddr) -> Result<DeviceUrl, String> {
    let SocketAddr::V4(peer) = peer else {
        return Err("IPv6 discovery is unsupported".into());
    };
    let packet = std::str::from_utf8(packet).map_err(|_| "invalid SSDP response")?;
    let mut lines = packet.split("\r\n");
    if lines.next() != Some("HTTP/1.1 200 OK") {
        return Err("invalid SSDP response status".into());
    }
    let mut location = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').ok_or("invalid SSDP header")?;
        if name.eq_ignore_ascii_case("location") {
            if location.is_some() {
                return Err("duplicate SSDP location".into());
            }
            location = Some(DeviceUrl::parse(value.trim())?);
        }
    }
    let location = location.ok_or("missing SSDP location")?;
    if location.ip != *peer.ip() {
        return Err("SSDP location does not match its response peer".into());
    }
    Ok(location)
}

async fn line(reader: &mut BufReader<TcpStream>, limit: usize) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    loop {
        let buffer = reader
            .fill_buf()
            .await
            .map_err(|error| format!("device HTTP read: {error}"))?;
        if buffer.is_empty() {
            return Err("truncated device HTTP line".into());
        }
        let length = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |position| position + 1);
        if result.len() + length > limit {
            return Err("device HTTP line exceeds its limit".into());
        }
        result.extend_from_slice(&buffer[..length]);
        reader.consume(length);
        if result.ends_with(b"\n") {
            if !result.ends_with(b"\r\n") {
                return Err("invalid device HTTP line ending".into());
            }
            result.truncate(result.len() - 2);
            return Ok(result);
        }
    }
}

async fn request(
    target: &DeviceUrl,
    method: &str,
    soap_action: Option<&str>,
    body: Option<&str>,
) -> Result<String, String> {
    timeout(REQUEST_TIMEOUT, exchange(target, method, soap_action, body))
        .await
        .map_err(|_| "device HTTP request timed out".to_owned())?
}

async fn exchange(
    target: &DeviceUrl,
    method: &str,
    soap_action: Option<&str>,
    body: Option<&str>,
) -> Result<String, String> {
    let mut socket = TcpStream::connect(SocketAddrV4::new(target.ip, target.port))
        .await
        .map_err(|error| format!("device HTTP connect: {error}"))?;
    let body = body.unwrap_or("");
    if body.len() > MAX_BODY {
        return Err("device SOAP request exceeds its limit".into());
    }
    let mut headers = format!(
        "{method} {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\nContent-Length: {}\r\n",
        target.path,
        target.ip,
        target.port,
        body.len()
    );
    if let Some(action) = soap_action {
        headers.push_str(&format!(
            "Content-Type: text/xml; charset=\"utf-8\"\r\nSOAPAction: \"{action}\"\r\n"
        ));
    }
    headers.push_str("\r\n");
    socket
        .write_all(headers.as_bytes())
        .await
        .map_err(|error| format!("device HTTP write: {error}"))?;
    socket
        .write_all(body.as_bytes())
        .await
        .map_err(|error| format!("device HTTP write: {error}"))?;
    let mut reader = BufReader::new(socket);
    let status = line(&mut reader, 8192).await?;
    let status = std::str::from_utf8(&status).map_err(|_| "invalid device HTTP status")?;
    let mut words = status.splitn(3, ' ');
    if !matches!(words.next(), Some("HTTP/1.1" | "HTTP/1.0")) {
        return Err("invalid device HTTP version".into());
    }
    let code = words
        .next()
        .ok_or("invalid device HTTP status")?
        .parse::<u16>()
        .map_err(|_| "invalid device HTTP status")?;
    let mut length = None;
    let mut chunked = false;
    let mut header_bytes = status.len();
    for index in 0..65 {
        let raw = line(&mut reader, 8192).await?;
        header_bytes += raw.len() + 2;
        if header_bytes > 32768 || index == 64 {
            return Err("device HTTP headers exceed their limit".into());
        }
        if raw.is_empty() {
            break;
        }
        let header = std::str::from_utf8(&raw).map_err(|_| "invalid device HTTP header")?;
        let (name, value) = header.split_once(':').ok_or("invalid device HTTP header")?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("duplicate device HTTP content length".into());
            }
            let count = value
                .parse::<usize>()
                .map_err(|_| "invalid device HTTP content length")?;
            if count > MAX_BODY {
                return Err("device HTTP body exceeds its limit".into());
            }
            length = Some(count);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            if chunked || !value.eq_ignore_ascii_case("chunked") {
                return Err("unsupported device HTTP transfer encoding".into());
            }
            chunked = true;
        } else if name.eq_ignore_ascii_case("content-encoding")
            && !value.eq_ignore_ascii_case("identity")
        {
            return Err("compressed device HTTP responses are unsupported".into());
        }
    }
    if chunked && length.is_some() {
        return Err("ambiguous device HTTP framing".into());
    }
    let mut output = Vec::new();
    if chunked {
        let mut chunks = 0usize;
        loop {
            chunks += 1;
            if chunks > 8192 {
                return Err("too many device HTTP chunks".into());
            }
            let raw = line(&mut reader, 1024).await?;
            let header = std::str::from_utf8(&raw).map_err(|_| "invalid device HTTP chunk")?;
            let size =
                usize::from_str_radix(header.split(';').next().ok_or("invalid device chunk")?, 16)
                    .map_err(|_| "invalid device HTTP chunk size")?;
            if size == 0 {
                let mut trailer_bytes = 0;
                for count in 0..65 {
                    let trailer = line(&mut reader, 8192).await?;
                    trailer_bytes += trailer.len();
                    if count == 64 || trailer_bytes > 32768 {
                        return Err("device HTTP trailers exceed their limit".into());
                    }
                    if trailer.is_empty() {
                        break;
                    }
                }
                break;
            }
            if size > MAX_BODY - output.len() {
                return Err("device HTTP body exceeds its limit".into());
            }
            let start = output.len();
            output.resize(start + size, 0);
            reader
                .read_exact(&mut output[start..])
                .await
                .map_err(|_| "truncated device HTTP chunk")?;
            if !line(&mut reader, 2).await?.is_empty() {
                return Err("invalid device HTTP chunk ending".into());
            }
        }
    } else if let Some(length) = length {
        output.resize(length, 0);
        reader
            .read_exact(&mut output)
            .await
            .map_err(|_| "truncated device HTTP body")?;
    } else {
        reader
            .take((MAX_BODY + 1) as u64)
            .read_to_end(&mut output)
            .await
            .map_err(|error| format!("device HTTP body: {error}"))?;
        if output.len() > MAX_BODY {
            return Err("device HTTP body exceeds its limit".into());
        }
    }
    if code != 200 {
        return Err(format!("device HTTP status {code}; redirects are refused"));
    }
    String::from_utf8(output).map_err(|_| "device XML must use UTF-8".into())
}

async fn soap(
    service: &Service,
    action: &str,
    arguments: &[(&str, &str)],
) -> Result<xml::Node, String> {
    let mut body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><s:Envelope xmlns:s=\"{SOAP_NAMESPACE}\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{}\">",
        xml::escape(&service.kind)?
    );
    for (name, value) in arguments {
        body.push_str(&format!("<{name}>{}</{name}>", xml::escape(value)?));
    }
    body.push_str(&format!("</u:{action}></s:Body></s:Envelope>"));
    let reply = request(
        &service.control,
        "POST",
        Some(&format!("{}#{action}", service.kind)),
        Some(&body),
    )
    .await?;
    let envelope = xml::parse(&reply)?;
    if envelope.name != "Envelope" || envelope.namespace != SOAP_NAMESPACE {
        return Err("invalid device SOAP envelope".into());
    }
    let body = envelope.child("Body")?;
    if body.namespace != SOAP_NAMESPACE || body.children.len() != 1 {
        return Err("invalid device SOAP body".into());
    }
    let mut children = body.children.iter();
    let response = children.next().ok_or("missing SOAP response")?;
    if response.name != format!("{action}Response") || response.namespace != service.kind {
        return Err(format!("invalid device response to {action}"));
    }
    if response
        .children
        .iter()
        .any(|node| !node.namespace.is_empty() && node.namespace != service.kind)
    {
        return Err("foreign namespace in device SOAP fields".into());
    }
    // Return ownership without retaining the parsed envelope's remaining fields.
    let mut envelope = envelope;
    let body = envelope
        .children
        .iter_mut()
        .find(|node| node.name == "Body")
        .ok_or("missing SOAP body")?;
    body.children
        .pop()
        .ok_or_else(|| "missing SOAP response".into())
}

fn normalized_mime(value: &str) -> &str {
    match value {
        "audio/x-flac" => "audio/flac",
        "audio/x-wav" | "audio/wave" | "audio/x-wave" => "audio/wav",
        "audio/x-m4a" | "audio/m4a" => "audio/mp4",
        "application/ogg" | "audio/x-ogg" => "audio/ogg",
        _ => value,
    }
}

fn advertised_mime<'a>(protocol_info: &'a str, mime: &'a str) -> Option<&'a str> {
    protocol_info.split(',').find_map(|item| {
        let mut fields = item.trim().splitn(4, ':');
        let protocol = fields.next()?;
        let network = fields.next()?;
        let advertised = fields.next()?;
        fields.next()?;
        if !protocol.eq_ignore_ascii_case("http-get") || network != "*" {
            return None;
        }
        if advertised == "*" {
            return Some(mime);
        }
        if normalized_mime(advertised).eq_ignore_ascii_case(normalized_mime(mime)) {
            Some(advertised)
        } else {
            None
        }
    })
}

fn metadata(track: &DeviceTrack, url: &str, mime: &str) -> Result<String, String> {
    Ok(format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\"><item id=\"0\" parentID=\"-1\" restricted=\"1\"><dc:title>{}</dc:title><upnp:class>object.item.audioItem.musicTrack</upnp:class><res protocolInfo=\"http-get:*:{}:*\">{}</res></item></DIDL-Lite>",
        xml::escape(&track.title)?,
        xml::escape(mime)?,
        xml::escape(url)?
    ))
}

fn encoded_ids(ids: &[u32]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = ids
        .iter()
        .flat_map(|id| id.to_be_bytes())
        .collect::<Vec<_>>();
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for part in bytes.chunks(3) {
        let first = part[0];
        let second = part.get(1).copied().unwrap_or(0);
        let third = part.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[usize::from(first >> 2)] as char);
        output.push(ALPHABET[usize::from((first & 3) << 4 | second >> 4)] as char);
        output.push(if part.len() > 1 {
            ALPHABET[usize::from((second & 15) << 2 | third >> 6)] as char
        } else {
            '='
        });
        output.push(if part.len() > 2 {
            ALPHABET[usize::from(third & 63)] as char
        } else {
            '='
        });
    }
    output
}

pub(super) async fn run(
    protocol: DeviceProtocol,
    description_url: &str,
    tracks: &[DeviceTrack],
    urls: &[String],
    replace: bool,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    if tracks.is_empty() || tracks.len() > 64 || tracks.len() != urls.len() {
        return Err("device playback needs one to 64 tracks and matching media URLs".into());
    }
    let target = DeviceUrl::parse(description_url)?;
    let device = description(&request(&target, "GET", None, None).await?, &target)?;
    let service = match protocol {
        DeviceProtocol::Upnp => device
            .services
            .iter()
            .find(|service| service_version(&service.kind, "AVTransport").is_some()),
        DeviceProtocol::Openhome => device
            .services
            .iter()
            .find(|service| service.kind == OPENHOME_PLAYLIST),
        DeviceProtocol::Slimproto | DeviceProtocol::Googlecast => {
            return Err("requested protocol does not use UPnP control".into());
        }
    }
    .ok_or("device does not advertise the requested protocol")?;
    let controlled = AtomicBool::new(false);
    let (result, was_cancelled) = {
        let playback = async {
            match protocol {
                DeviceProtocol::Upnp => {
                    upnp_play(&device, service, tracks, urls, &controlled).await
                }
                DeviceProtocol::Openhome => {
                    openhome_play(service, tracks, urls, replace, &controlled).await
                }
                DeviceProtocol::Slimproto | DeviceProtocol::Googlecast => {
                    Err("invalid device protocol".into())
                }
            }
        };
        tokio::pin!(playback);
        tokio::select! {
            biased;
            _ = cancelled(&mut shutdown) => (Ok(()), true),
            result = &mut playback => (result, false),
        }
    };
    if result.is_err() || was_cancelled {
        let error = result
            .as_ref()
            .err()
            .map(String::as_str)
            .unwrap_or("device playback cancelled");
        if !controlled.load(Ordering::Relaxed) {
            return result;
        }
        let arguments = if matches!(protocol, DeviceProtocol::Upnp) {
            vec![("InstanceID", "0")]
        } else {
            Vec::new()
        };
        match timeout(Duration::from_secs(2), soap(service, "Stop", &arguments)).await {
            Ok(Ok(_)) => {}
            Ok(Err(stop_error)) => {
                return Err(format!("{error}; stopping the device failed: {stop_error}"));
            }
            Err(_) => return Err(format!("{error}; stopping the device timed out")),
        }
    }
    result
}

async fn cancelled(receiver: &mut watch::Receiver<bool>) {
    loop {
        if *receiver.borrow() {
            return;
        }
        if receiver.changed().await.is_err() {
            return;
        }
    }
}

async fn upnp_play(
    device: &Description,
    service: &Service,
    tracks: &[DeviceTrack],
    urls: &[String],
    controlled: &AtomicBool,
) -> Result<(), String> {
    let manager = device
        .services
        .iter()
        .find(|candidate| {
            candidate.device_scope == service.device_scope
                && service_version(&candidate.kind, "ConnectionManager").is_some()
        })
        .ok_or("UPnP renderer needs ConnectionManager/GetProtocolInfo")?;
    let formats = soap(manager, "GetProtocolInfo", &[]).await?;
    let sink = formats.value("Sink")?;
    let mimes = tracks
        .iter()
        .map(|track| {
            advertised_mime(sink, track.mime)
                .ok_or_else(|| format!("renderer does not advertise original {} audio", track.mime))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let current = soap(service, "GetTransportInfo", &[("InstanceID", "0")]).await?;
    let state = current.value("CurrentTransportState")?;
    if !matches!(state, "STOPPED" | "NO_MEDIA_PRESENT") {
        return Err("renderer is active; stop it before casting".into());
    }
    controlled.store(true, Ordering::Relaxed);
    for ((track, url), mime) in tracks.iter().zip(urls).zip(mimes) {
        let metadata = metadata(track, url, mime)?;
        soap(
            service,
            "SetAVTransportURI",
            &[
                ("InstanceID", "0"),
                ("CurrentURI", url),
                ("CurrentURIMetaData", &metadata),
            ],
        )
        .await?;
        soap(service, "Play", &[("InstanceID", "0"), ("Speed", "1")]).await?;
        let start_deadline = Instant::now() + START_TIMEOUT;
        let mut playing = false;
        loop {
            let info = soap(service, "GetTransportInfo", &[("InstanceID", "0")]).await?;
            let position = soap(service, "GetPositionInfo", &[("InstanceID", "0")]).await?;
            let actual = position.value("TrackURI")?;
            if actual != url
                && (!actual.is_empty()
                    || playing
                    || info.value("CurrentTransportState")? == "PLAYING")
            {
                controlled.store(false, Ordering::Relaxed);
                return Err(
                    "renderer changed or cleared its URI; another controller may own it".into(),
                );
            }
            if info.value("CurrentTransportStatus")? != "OK" {
                return Err("renderer reported a transport error".into());
            }
            match info.value("CurrentTransportState")? {
                "PLAYING" => playing = true,
                "TRANSITIONING" | "PAUSED_PLAYBACK" => {}
                "STOPPED" if playing => break,
                "STOPPED" if Instant::now() < start_deadline => {}
                "STOPPED" | "NO_MEDIA_PRESENT" => {
                    return Err("renderer did not confirm playback before stopping".into());
                }
                _ => return Err("unsupported renderer transport state".into()),
            }
            if !playing && Instant::now() >= start_deadline {
                return Err("renderer did not enter PLAYING within 30 seconds".into());
            }
            tokio::time::sleep(POLL).await;
        }
    }
    Ok(())
}

async fn openhome_play(
    service: &Service,
    tracks: &[DeviceTrack],
    urls: &[String],
    replace: bool,
    controlled: &AtomicBool,
) -> Result<(), String> {
    let formats = soap(service, "ProtocolInfo", &[]).await?;
    let info = formats.value("Value")?;
    let mimes = tracks
        .iter()
        .map(|track| {
            advertised_mime(info, track.mime).ok_or_else(|| {
                format!(
                    "OpenHome renderer does not advertise original {} audio",
                    track.mime
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let maximum = soap(service, "TracksMax", &[])
        .await?
        .value("Value")?
        .parse::<usize>()
        .map_err(|_| "invalid OpenHome playlist capacity")?;
    if tracks.len() > maximum {
        return Err("selection exceeds the OpenHome playlist capacity".into());
    }
    let queue = soap(service, "IdArray", &[]).await?;
    if !queue.value("Array")?.is_empty() && !replace {
        return Err(
            "OpenHome playlist is not empty; clear it first or explicitly use --replace".into(),
        );
    }
    for action in ["Repeat", "Shuffle"] {
        match soap(service, action, &[]).await?.value("Value")? {
            "0" | "false" => {}
            "1" | "true" => {
                return Err(format!(
                    "disable OpenHome {action} before finite Aède casting"
                ));
            }
            _ => return Err(format!("invalid OpenHome {action} setting")),
        }
    }
    let state = soap(service, "TransportState", &[]).await?;
    if state.value("Value")? != "Stopped" && !replace {
        return Err(
            "OpenHome renderer is active; stop it first or explicitly use --replace".into(),
        );
    }
    controlled.store(true, Ordering::Relaxed);
    if replace {
        soap(service, "Stop", &[]).await?;
        soap(service, "DeleteAll", &[]).await?;
    }
    let mut ids = Vec::with_capacity(tracks.len());
    let mut after_id = 0;
    for ((track, url), mime) in tracks.iter().zip(urls).zip(mimes) {
        let after = after_id.to_string();
        let metadata = metadata(track, url, mime)?;
        let reply = soap(
            service,
            "Insert",
            &[("AfterId", &after), ("Uri", url), ("Metadata", &metadata)],
        )
        .await?;
        after_id = reply
            .value("NewId")?
            .parse::<u32>()
            .map_err(|_| "invalid OpenHome playlist ID")?;
        if after_id == 0 || ids.contains(&after_id) {
            return Err("invalid or duplicated OpenHome playlist ID".into());
        }
        ids.push(after_id);
    }
    let queue = soap(service, "IdArray", &[]).await?;
    let actual_ids = queue
        .value("Array")?
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if actual_ids != encoded_ids(&ids).as_bytes() {
        controlled.store(false, Ordering::Relaxed);
        return Err("OpenHome playlist changed while inserting Aède tracks".into());
    }
    let token = queue
        .value("Token")?
        .parse::<u32>()
        .map_err(|_| "invalid OpenHome playlist token")?;
    soap(service, "SeekIndex", &[("Value", "0")]).await?;
    soap(service, "Play", &[]).await?;
    let start_deadline = Instant::now() + START_TIMEOUT;
    let mut playing = false;
    loop {
        let changed = soap(service, "IdArrayChanged", &[("Token", &token.to_string())]).await?;
        if !matches!(changed.value("Value")?, "0" | "false") {
            controlled.store(false, Ordering::Relaxed);
            return Err("OpenHome playlist changed by another controller".into());
        }
        let state = soap(service, "TransportState", &[]).await?;
        let current_id = soap(service, "Id", &[])
            .await?
            .value("Value")?
            .parse::<u32>()
            .map_err(|_| "invalid OpenHome current track ID")?;
        if current_id != 0 && !ids.contains(&current_id) {
            controlled.store(false, Ordering::Relaxed);
            return Err("OpenHome selected a track outside the Aède queue".into());
        }
        match state.value("Value")? {
            "Playing" => playing = true,
            "Buffering" | "Paused" => {}
            "Stopped" if playing && ids.last() == Some(&current_id) => break,
            "Stopped" if playing => {
                return Err("OpenHome queue stopped before its final track".into());
            }
            "Stopped" if Instant::now() < start_deadline => {}
            "Stopped" => {
                return Err("OpenHome renderer did not confirm playback before stopping".into());
            }
            _ => return Err("invalid OpenHome transport state".into()),
        }
        if !playing && Instant::now() >= start_deadline {
            return Err("OpenHome renderer did not enter Playing within 30 seconds".into());
        }
        tokio::time::sleep(POLL).await;
    }
    // Retain the user's completed device playlist; the next command requires --replace.
    Ok(())
}

#[cfg(test)]
#[path = "upnp_tests.rs"]
mod tests;
