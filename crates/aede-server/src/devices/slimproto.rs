//! Explicit, single-player SlimProto control for original local audio.
//!
//! Packet layouts and event meanings follow the Squeezelite and Lyrion
//! implementations. Download EOF and decoder EOF are not audible completion:
//! this first profile waits for decoded EOF followed by a drained output.

use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, interval, timeout, timeout_at};

use super::DeviceTrack;

const MAX_PAYLOAD: usize = 4096;
const MAX_QUEUE: usize = 64;
const MAX_REFUSED_PEERS: usize = 16;
const MAX_PACKET_BURST: usize = 256;
const STRM_BYTES: usize = 24;
const STAT_BYTES: usize = 53;

#[derive(Clone, Copy)]
struct Limits {
    connection: Duration,
    hello: Duration,
    packet: Duration,
    write: Duration,
    heartbeat: Duration,
    completion_grace: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            connection: Duration::from_secs(60),
            hello: Duration::from_secs(5),
            packet: Duration::from_secs(30),
            write: Duration::from_secs(2),
            heartbeat: Duration::from_secs(5),
            completion_grace: Duration::from_secs(120),
        }
    }
}

struct SessionOptions {
    volume: Option<u8>,
    limits: Limits,
}

/// Runs one finite original-file queue for the explicitly selected IPv4 player.
/// Cancellation is cooperative so the player receives a bounded stop request.
/// No listening history is inferred from these unauthenticated device reports.
pub(super) async fn run(
    listener: TcpListener,
    expected_peer: Ipv4Addr,
    tracks: &[DeviceTrack],
    urls: &[String],
    volume: Option<u8>,
    shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    run_with_limits(
        listener,
        expected_peer,
        tracks,
        urls,
        shutdown,
        SessionOptions {
            volume,
            limits: Limits::default(),
        },
    )
    .await
}

async fn run_with_limits(
    listener: TcpListener,
    expected_peer: Ipv4Addr,
    tracks: &[DeviceTrack],
    urls: &[String],
    mut shutdown: watch::Receiver<bool>,
    options: SessionOptions,
) -> Result<(), String> {
    let SessionOptions { volume, limits } = options;
    if tracks.is_empty() || tracks.len() > MAX_QUEUE || tracks.len() != urls.len() {
        return Err("SlimProto requires 1 to 64 tracks and one media URL per occurrence".into());
    }
    if volume.is_some_and(|volume| volume > 100) {
        return Err("SlimProto device volume must be between 0 and 100 percent".into());
    }
    let Some(mut stream) = accept_player(&listener, expected_peer, &mut shutdown, limits).await?
    else {
        return Ok(());
    };
    stream
        .set_nodelay(true)
        .map_err(|error| format!("SlimProto TCP: {error}"))?;

    let hello = tokio::select! {
        result = timeout(limits.hello, read_packet(&mut stream)) => {
            result.map_err(|_| "SlimProto HELO timed out".to_string()).and_then(|packet| packet)
        }
        () = cancelled(&mut shutdown) => {
            stop(&mut stream, limits).await?;
            return Ok(());
        },
    };
    let (mut reader, mut writer) = stream.into_split();
    let prepared = hello.and_then(|packet| {
        let capabilities = Capabilities::parse(&packet)?;
        tracks
            .iter()
            .zip(urls)
            .map(|(track, url)| start_payload(track, url, &capabilities))
            .collect::<Result<Vec<_>, _>>()
    });
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            if let Err(stop_error) = stop(&mut writer, limits).await {
                return Err(format!("{error}; stop request also failed: {stop_error}"));
            }
            return Err(error);
        }
    };

    // A single reader preserves fragmented frames while the controller sends
    // heartbeats or handles cancellation. Cancelling read_exact would discard
    // its partial prefix and desynchronise the next frame.
    let (sender, mut packets) = mpsc::channel(8);
    let reader_task = tokio::spawn(async move {
        loop {
            let packet = timeout(limits.packet, read_packet(&mut reader))
                .await
                .map_err(|_| "SlimProto player stopped responding".to_string())
                .and_then(|packet| packet);
            let failed = packet.is_err();
            if sender.send(packet).await.is_err() || failed {
                break;
            }
        }
    });
    let result = play_queue(
        &mut writer,
        &mut packets,
        tracks,
        &prepared,
        volume,
        &mut shutdown,
        limits,
    )
    .await;
    let stopped = stop(&mut writer, limits).await;
    reader_task.abort();
    // Preserve the original transport/decoder diagnosis if stopping also fails.
    match (result, stopped) {
        (Err(error), Err(stop_error)) => {
            Err(format!("{error}; stop request also failed: {stop_error}"))
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(()), stopped) => stopped,
    }
}

async fn accept_player(
    listener: &TcpListener,
    expected: Ipv4Addr,
    shutdown: &mut watch::Receiver<bool>,
    limits: Limits,
) -> Result<Option<TcpStream>, String> {
    let deadline = Instant::now() + limits.connection;
    for _ in 0..MAX_REFUSED_PEERS {
        let accepted = tokio::select! {
            result = timeout_at(deadline, listener.accept()) => result
                .map_err(|_| "SlimProto player connection timed out".to_string())?
                .map_err(|error| format!("SlimProto accept: {error}"))?,
            () = cancelled(shutdown) => return Ok(None),
        };
        if accepted.1.ip() == IpAddr::V4(expected) {
            return Ok(Some(accepted.0));
        }
        // Other hosts cannot claim this explicitly selected receiver session.
        drop(accepted.0);
    }
    Err("SlimProto refused too many connections from another address".into())
}

async fn cancelled(shutdown: &mut watch::Receiver<bool>) {
    loop {
        if *shutdown.borrow_and_update() || shutdown.changed().await.is_err() {
            return;
        }
    }
}

#[derive(Debug)]
struct Packet {
    opcode: [u8; 4],
    payload: Vec<u8>,
}

async fn read_packet(reader: &mut (impl AsyncRead + Unpin)) -> Result<Packet, String> {
    let mut header = [0; 8];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|error| format!("SlimProto disconnected: {error}"))?;
    let length = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if length > MAX_PAYLOAD {
        return Err("SlimProto player packet exceeds 4096 bytes".into());
    }
    let mut payload = vec![0; length];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|error| format!("SlimProto truncated packet: {error}"))?;
    Ok(Packet {
        opcode: [header[0], header[1], header[2], header[3]],
        payload,
    })
}

struct Capabilities {
    flac: bool,
    mp3: bool,
    wav: bool,
    maximum_rate: u32,
}

impl Capabilities {
    fn parse(packet: &Packet) -> Result<Self, String> {
        if packet.opcode != *b"HELO" || packet.payload.len() < 36 {
            return Err("SlimProto requires a modern HELO with advertised codecs".into());
        }
        let text = std::str::from_utf8(&packet.payload[36..])
            .map_err(|_| "SlimProto capabilities are not UTF-8".to_string())?;
        if text.bytes().any(|byte| !(b' '..=b'~').contains(&byte)) {
            return Err("SlimProto capabilities contain control or non-ASCII bytes".into());
        }
        let mut capabilities = Self {
            flac: false,
            mp3: false,
            wav: false,
            maximum_rate: 48_000,
        };
        let mut has_rate = false;
        for item in text.split(',') {
            match item {
                "flc" => capabilities.flac = true,
                "mp3" => capabilities.mp3 = true,
                "wav" => capabilities.wav = true,
                _ => {
                    if let Some(rate) = item.strip_prefix("MaxSampleRate=") {
                        if has_rate {
                            return Err(
                                "SlimProto advertises duplicate maximum sample rates".into()
                            );
                        }
                        capabilities.maximum_rate = rate
                            .parse::<u32>()
                            .ok()
                            .filter(|rate| (8_000..=1_536_000).contains(rate))
                            .ok_or_else(|| {
                                "SlimProto maximum sample rate is invalid".to_string()
                            })?;
                        has_rate = true;
                    }
                }
            }
        }
        Ok(capabilities)
    }
}

fn start_payload(
    track: &DeviceTrack,
    url: &str,
    capabilities: &Capabilities,
) -> Result<Vec<u8>, String> {
    let properties = &track.properties;
    let rate = properties
        .sample_rate
        .filter(|rate| *rate > 0)
        .ok_or_else(|| "SlimProto needs a known source sample rate".to_string())?;
    if rate > capabilities.maximum_rate {
        return Err("SlimProto source sample rate exceeds the player's advertised limit".into());
    }
    if !matches!(properties.channels, Some(1 | 2)) {
        return Err("SlimProto's initial profile supports mono or stereo sources".into());
    }
    if properties.duration_ms.is_none_or(|duration| duration == 0) {
        return Err("SlimProto needs a positive known track duration".into());
    }
    let mut payload = control_payload(b's');
    payload[1] = b'1';
    payload[7] = 1; // Small sources must not wait for a large network threshold.
    let supported = match (
        properties.codec.as_str(),
        properties.container.as_str(),
        track.mime,
    ) {
        ("flac", "flac", "audio/flac") if capabilities.flac => {
            if !matches!(properties.bit_depth, Some(4..=24)) {
                return Err("SlimProto's initial FLAC profile requires a known precision of at most 24 bits".into());
            }
            payload[2] = b'f';
            true
        }
        ("mp3", "mp3", "audio/mpeg") if capabilities.mp3 => {
            payload[2] = b'm';
            true
        }
        ("pcm", "wav", "audio/wav") if capabilities.wav => {
            payload[2] = b'p';
            payload[3] = match properties.bit_depth {
                Some(8) => b'0',
                Some(16) => b'1',
                Some(24) => b'2',
                Some(32) => b'3',
                _ => return Err("SlimProto WAV requires 8, 16, 24 or 32-bit integer PCM".into()),
            };
            let rates = [
                11_025, 22_050, 32_000, 44_100, 48_000, 8_000, 12_000, 16_000, 24_000, 96_000,
                88_200, 176_400, 192_000,
            ];
            let index = rates
                .iter()
                .position(|candidate| *candidate == rate)
                .ok_or_else(|| {
                    "SlimProto WAV sample rate has no supported protocol code".to_string()
                })?;
            payload[4] = b'0' + index as u8;
            payload[5] = if properties.channels == Some(1) {
                b'1'
            } else {
                b'2'
            };
            payload[6] = b'1';
            true
        }
        _ => false,
    };
    if !supported {
        return Err(
            "SlimProto source codec/container is unsupported or not advertised by this player"
                .into(),
        );
    }
    let (address, path) = media_address(url)?;
    payload[18..20].copy_from_slice(&address.port().to_be_bytes());
    payload[20..24].copy_from_slice(&address.ip().octets());
    let request = format!("GET {path} HTTP/1.0\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    payload.extend_from_slice(request.as_bytes());
    Ok(payload)
}

fn media_address(url: &str) -> Result<(SocketAddrV4, &str), String> {
    let remainder = url
        .strip_prefix("http://")
        .ok_or_else(|| "SlimProto needs an explicit HTTP IPv4 media URL".to_string())?;
    let (authority, suffix) = remainder
        .split_once('/')
        .ok_or_else(|| "SlimProto media URL has no path".to_string())?;
    let address = authority
        .parse::<SocketAddrV4>()
        .map_err(|_| "SlimProto media URL requires an IPv4 address and port".to_string())?;
    let path = &remainder[authority.len()..];
    if address.port() == 0
        || suffix.is_empty()
        || path.len() > 2048
        || path
            .bytes()
            .any(|byte| !(b'!'..=b'~').contains(&byte) || byte == b'#')
    {
        return Err("SlimProto media URL path is invalid or too long".into());
    }
    Ok((address, path))
}

fn control_payload(command: u8) -> Vec<u8> {
    let mut payload = vec![0; STRM_BYTES];
    payload[0] = command;
    payload[2] = b'm';
    payload[3..7].fill(b'?');
    payload[10] = b'0'; // Disable fades; volume and replay gain remain untouched.
    payload
}

fn volume_payload(volume: u8) -> [u8; 18] {
    let gain = (u32::from(volume) * 65_536 + 50) / 100;
    let legacy_gain = (u32::from(volume) * 128 + 50) / 100;
    let mut payload = [0; 18];
    payload[..4].copy_from_slice(&legacy_gain.to_be_bytes());
    payload[4..8].copy_from_slice(&legacy_gain.to_be_bytes());
    payload[8] = 1;
    payload[10..14].copy_from_slice(&gain.to_be_bytes());
    payload[14..18].copy_from_slice(&gain.to_be_bytes());
    payload
}

async fn send_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    opcode: &[u8; 4],
    payload: &[u8],
    limits: Limits,
) -> Result<(), String> {
    let length = u16::try_from(payload.len() + 4)
        .map_err(|_| "SlimProto server packet is too long".to_string())?;
    let mut frame = Vec::with_capacity(usize::from(length) + 2);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(opcode);
    frame.extend_from_slice(payload);
    timeout(limits.write, writer.write_all(&frame))
        .await
        .map_err(|_| "SlimProto write timed out".to_string())?
        .map_err(|error| format!("SlimProto write: {error}"))
}

async fn stop(writer: &mut (impl AsyncWrite + Unpin), limits: Limits) -> Result<(), String> {
    send_frame(writer, b"strm", &control_payload(b'q'), limits).await
}

async fn play_queue(
    writer: &mut (impl AsyncWrite + Unpin),
    packets: &mut mpsc::Receiver<Result<Packet, String>>,
    tracks: &[DeviceTrack],
    prepared: &[Vec<u8>],
    volume: Option<u8>,
    shutdown: &mut watch::Receiver<bool>,
    limits: Limits,
) -> Result<(), String> {
    if *shutdown.borrow_and_update() {
        return Ok(());
    }
    // strm s preserves old output for LMS gapless joins. An explicit new cast
    // must first discard any samples left by a previous controller session.
    stop(writer, limits).await?;
    if let Some(volume) = volume {
        send_frame(writer, b"audg", &volume_payload(volume), limits).await?;
    }
    send_frame(writer, b"aude", &[1, 1], limits).await?;
    let mut heartbeat = interval(limits.heartbeat);
    // The first interval tick is immediate; strm s itself starts the heartbeat.
    heartbeat.tick().await;
    let mut burst_start = Instant::now();
    let mut burst = 0;
    for (track, request) in tracks.iter().zip(prepared) {
        if *shutdown.borrow_and_update() {
            return Ok(());
        }
        let budget = Duration::from_millis(track.properties.duration_ms.unwrap_or(0))
            .checked_add(limits.completion_grace)
            .ok_or_else(|| "SlimProto track duration is too large".to_string())?;
        let deadline = Instant::now()
            .checked_add(budget)
            .ok_or_else(|| "SlimProto track duration is too large".to_string())?;
        send_frame(writer, b"strm", request, limits).await?;
        let mut started = false;
        let mut decoded = false;
        let mut released = false;
        loop {
            let packet = tokio::select! {
                () = cancelled(shutdown) => return Ok(()),
                () = tokio::time::sleep_until(deadline) => return Err("SlimProto track did not finish within its playback budget".into()),
                _ = heartbeat.tick() => {
                    send_frame(writer, b"strm", &control_payload(b't'), limits).await?;
                    continue;
                },
                packet = packets.recv() => packet.ok_or_else(|| "SlimProto packet reader stopped".to_string())??,
            };
            if burst_start.elapsed() >= Duration::from_secs(5) {
                burst_start = Instant::now();
                burst = 0;
            }
            burst += 1;
            if burst > MAX_PACKET_BURST {
                return Err("SlimProto player sent too many control packets".into());
            }
            match packet.opcode {
                opcode if opcode == *b"STAT" => {
                    if packet.payload.len() != STAT_BYTES {
                        return Err("SlimProto STAT must contain 53 payload bytes".into());
                    }
                    match &packet.payload[..4] {
                        b"STMs" => started = true,
                        b"STMd" => {
                            decoded = true;
                            // A short source can finish decoding before the
                            // player's normal output threshold is reached.
                            if !started && !released {
                                send_frame(writer, b"strm", &control_payload(b'u'), limits).await?;
                                released = true;
                            }
                        }
                        b"STMl" if !started && !released => {
                            send_frame(writer, b"strm", &control_payload(b'u'), limits).await?;
                            released = true;
                        }
                        b"STMu" if started && decoded && packet.payload[33..37] == [0; 4] => break,
                        b"STMn" => return Err("SlimProto player could not decode the source".into()),
                        b"STMo" => return Err("SlimProto player ran out of audio while still downloading".into()),
                        b"STMp" => return Err("SlimProto device-side pause is not supported by this finite-queue profile".into()),
                        _ => {},
                    }
                }
                opcode if opcode == *b"DSCO" => {
                    if packet.payload.len() != 1 || packet.payload[0] > 1 {
                        return Err("SlimProto player could not retrieve the source".into());
                    }
                    // Normal download EOF or decoder-triggered disconnect
                    // leaves decoded samples in the player's output buffer.
                }
                opcode if opcode == *b"RESP" => {
                    let successful = packet.payload.starts_with(b"HTTP/1.0 200 ")
                        || packet.payload.starts_with(b"HTTP/1.1 200 ");
                    if !successful {
                        return Err(
                            "SlimProto player received an unsuccessful media HTTP response".into(),
                        );
                    }
                }
                opcode
                    if opcode == *b"SETD"
                        || opcode == *b"IR  "
                        || opcode == *b"BUTN"
                        || opcode == *b"KNOB"
                        || opcode == *b"ANIC" => {}
                _ => return Err("SlimProto player sent an unsupported control packet".into()),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "slimproto_tests.rs"]
mod tests;
