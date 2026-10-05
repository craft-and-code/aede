//! Explicit Cast V2 sender for a finite queue on the Default Media Receiver.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, timeout};

use super::{DeviceTrack, cast_tls, cast_wire};
use cast_wire::Message;

const CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
const HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
const RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
const MEDIA: &str = "urn:x-cast:com.google.cast.media";
const APPLICATION: &str = "CC1AD845";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

pub(super) fn endpoint(value: &str) -> Result<SocketAddrV4, String> {
    let address = if let Ok(ip) = value.parse::<Ipv4Addr>() {
        SocketAddrV4::new(ip, 8009)
    } else {
        value.parse::<SocketAddrV4>().map_err(
            |_| "Google Cast --device needs a literal IPv4 address, optionally followed by :PORT",
        )?
    };
    super::validate_ip(*address.ip())?;
    if address.port() == 0 {
        return Err("Google Cast needs a nonzero control port".into());
    }
    Ok(address)
}

pub(super) fn preflight(tracks: &[DeviceTrack]) -> Result<(), String> {
    for track in tracks {
        let p = &track.properties;
        let supported = matches!(
            (p.container.as_str(), p.codec.as_str()),
            ("flac", "flac")
                | ("mp3", "mp3")
                | ("ogg", "vorbis" | "opus")
                | ("opus", "opus")
                | ("wav" | "wave", "pcm")
                | ("mp4" | "m4a", "aac")
                | ("aac" | "adts", "aac")
        );
        let lossless = matches!(p.codec.as_str(), "flac" | "pcm");
        let maximum_rate = if lossless { 96_000 } else { 48_000 };
        if !supported
            || !matches!(p.channels, Some(1 | 2))
            || !p
                .sample_rate
                .is_some_and(|rate| rate > 0 && rate <= maximum_rate)
            || (lossless && !p.bit_depth.is_some_and(|bits| bits > 0 && bits <= 24))
        {
            return Err("selection exceeds the initial Google Cast mono/stereo codec, rate or precision profile; originals are never silently converted".into());
        }
    }
    Ok(())
}

struct ReaderTask(tokio::task::JoinHandle<()>);
impl Drop for ReaderTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct Controller {
    writer: mpsc::Sender<(Message, oneshot::Sender<Result<(), String>>)>,
    messages: mpsc::Receiver<Result<Message, String>>,
    sender: String,
    request: u32,
    application: Option<String>,
    session: Option<u64>,
    owns_application: bool,
    next_ping: Instant,
    last_pong: Instant,
    _reader: ReaderTask,
    _writer: ReaderTask,
}

impl Controller {
    fn new<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(stream: S, sender: String) -> Self {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let (outgoing, mut writes) =
            mpsc::channel::<(Message, oneshot::Sender<Result<(), String>>)>(16);
        let writer_task = tokio::spawn(async move {
            while let Some((message, done)) = writes.recv().await {
                let result = timeout(
                    Duration::from_secs(2),
                    cast_wire::write(&mut writer, &message),
                )
                .await
                .unwrap_or_else(|_| Err("Cast write timed out".into()));
                let failed = result.is_err();
                let _ = done.send(result);
                if failed {
                    break;
                }
            }
        });
        let (tx, messages) = mpsc::channel(16);
        let task = tokio::spawn(async move {
            let mut window = Instant::now();
            let mut count = 0;
            loop {
                // One reader owns partial frames even while timers and cancellation
                // fire in the controller; dropping read_exact would lose framing.
                let message =
                    match timeout(Duration::from_secs(30), cast_wire::read(&mut reader)).await {
                        Ok(result) => result,
                        Err(_) => Err("Cast frame read timed out".into()),
                    };
                if window.elapsed() >= Duration::from_secs(1) {
                    window = Instant::now();
                    count = 0;
                }
                count += 1;
                if count > 256 {
                    let _ = tx
                        .send(Err("Cast message rate exceeds its limit".into()))
                        .await;
                    break;
                }
                let failed = message.is_err();
                if tx.send(message).await.is_err() || failed {
                    break;
                }
            }
        });
        Self {
            writer: outgoing,
            messages,
            sender,
            request: 0,
            application: None,
            session: None,
            owns_application: false,
            next_ping: Instant::now(),
            last_pong: Instant::now(),
            _reader: ReaderTask(task),
            _writer: ReaderTask(writer_task),
        }
    }

    async fn send(
        &mut self,
        destination: &str,
        namespace: &str,
        payload: Value,
    ) -> Result<(), String> {
        let (done, result) = oneshot::channel();
        timeout(Duration::from_secs(3), async {
            self.writer
                .send((
                    Message {
                        source: self.sender.clone(),
                        destination: destination.into(),
                        namespace: namespace.into(),
                        payload,
                    },
                    done,
                ))
                .await
                .map_err(|_| "Cast writer disconnected")?;
            result.await.map_err(|_| "Cast writer disconnected")?
        })
        .await
        .map_err(|_| "Cast write timed out".to_owned())?
    }

    async fn request(
        &mut self,
        destination: &str,
        namespace: &str,
        mut payload: Value,
    ) -> Result<u32, String> {
        self.request = self
            .request
            .checked_add(1)
            .ok_or("Cast request budget exhausted")?;
        payload["requestId"] = self.request.into();
        self.send(destination, namespace, payload).await?;
        Ok(self.request)
    }

    async fn next(&mut self, deadline: Instant) -> Result<Option<Message>, String> {
        loop {
            if Instant::now() >= deadline {
                return Ok(None);
            }
            if self.last_pong.elapsed() > Duration::from_secs(20) {
                return Err("Cast heartbeat timed out".into());
            }
            if Instant::now() >= self.next_ping {
                self.send("receiver-0", HEARTBEAT, json!({"type":"PING"}))
                    .await?;
                self.next_ping = Instant::now() + Duration::from_secs(5);
            }
            let message = tokio::select! {
                message = self.messages.recv() => message.ok_or("Cast receiver disconnected")??,
                _ = tokio::time::sleep_until(self.next_ping.min(deadline)) => continue,
            };
            if message.destination != self.sender && message.destination != "*" {
                continue;
            }
            let platform = message.source == "receiver-0";
            let application = self.application.as_deref() == Some(message.source.as_str());
            if message.namespace == HEARTBEAT && platform {
                match message.payload["type"].as_str() {
                    Some("PING") => {
                        self.send("receiver-0", HEARTBEAT, json!({"type":"PONG"}))
                            .await?;
                    }
                    Some("PONG") => self.last_pong = Instant::now(),
                    _ => return Err("invalid Cast heartbeat".into()),
                }
                continue;
            }
            if message.namespace == CONNECTION
                && (platform || application)
                && message.payload["type"] == "CLOSE"
            {
                return Err("Cast receiver closed the channel".into());
            }
            if (message.namespace == RECEIVER && platform)
                || (message.namespace == MEDIA && application)
            {
                return Ok(Some(message));
            }
        }
    }

    async fn reply(
        &mut self,
        id: u32,
        namespace: &str,
        kind: &str,
        deadline: Instant,
    ) -> Result<Value, String> {
        for _ in 0..256 {
            let message = self.next(deadline).await?.ok_or("Cast status timed out")?;
            if message.namespace != namespace
                || message.payload["requestId"].as_u64() != Some(u64::from(id))
            {
                continue;
            }
            if message.payload["type"] != kind {
                return Err("Cast receiver rejected the requested operation".into());
            }
            return Ok(message.payload);
        }
        Err("Cast status message budget exceeded".into())
    }

    async fn play(
        &mut self,
        tracks: &[DeviceTrack],
        urls: &[String],
        replace: bool,
    ) -> Result<(), String> {
        self.send(
            "receiver-0",
            CONNECTION,
            json!({"type":"CONNECT", "origin":{}}),
        )
        .await?;
        let id = self
            .request("receiver-0", RECEIVER, json!({"type":"GET_STATUS"}))
            .await?;
        let status = self
            .reply(
                id,
                RECEIVER,
                "RECEIVER_STATUS",
                Instant::now() + REQUEST_TIMEOUT,
            )
            .await?;
        let apps = applications(&status)?;
        if !apps.is_empty() && !replace {
            return Err("Google Cast already has an application session; use --replace only to explicitly replace its playback".into());
        }
        let id = self
            .request(
                "receiver-0",
                RECEIVER,
                json!({"type":"LAUNCH", "appId": APPLICATION}),
            )
            .await?;
        // From this point LAUNCH may have taken effect even if its reply is lost.
        // Never STOP receiver-0 indiscriminately: it may now host somebody else.
        let status = self
            .reply(
                id,
                RECEIVER,
                "RECEIVER_STATUS",
                Instant::now() + Duration::from_secs(30),
            )
            .await?;
        let app = applications(&status)?
            .iter()
            .find(|app| app["appId"] == APPLICATION)
            .ok_or("Cast Default Media Receiver did not launch")?;
        let transport = routing_id(&app["transportId"])?;
        self.application = Some(transport.clone());
        self.send(
            &transport,
            CONNECTION,
            json!({"type":"CONNECT", "origin":{}}),
        )
        .await?;
        for (track, url) in tracks.iter().zip(urls) {
            self.session = None;
            self.owns_application = true;
            let id = self.request(&transport, MEDIA, json!({
                "type":"LOAD", "autoplay":true, "currentTime":0,
                "media": {"contentId":url, "contentType":track.mime, "streamType":"BUFFERED",
                    "metadata":{"metadataType":3, "title":track.title}}
            })).await?;
            let status = self
                .reply(
                    id,
                    MEDIA,
                    "MEDIA_STATUS",
                    Instant::now() + Duration::from_secs(30),
                )
                .await?;
            let first = media_status(&status)?;
            let session = first["mediaSessionId"]
                .as_u64()
                .filter(|id| *id > 0)
                .ok_or("Cast media session ID is missing")?;
            self.session = Some(session);
            check_content(first, url)?;
            let mut finished = playback_state(first)?;
            let mut next_poll = Instant::now() + Duration::from_secs(2);
            let mut last_status = Instant::now();
            while !finished {
                if Instant::now() >= next_poll {
                    self.request(
                        &transport,
                        MEDIA,
                        json!({"type":"GET_STATUS", "mediaSessionId":session}),
                    )
                    .await?;
                    next_poll = Instant::now() + Duration::from_secs(2);
                }
                let message = match self
                    .next(next_poll.min(last_status + REQUEST_TIMEOUT))
                    .await?
                {
                    Some(message) => message,
                    None if last_status.elapsed() >= REQUEST_TIMEOUT => {
                        return Err("Cast media status timed out".into());
                    }
                    None => continue,
                };
                if message.namespace == RECEIVER {
                    if !applications(&message.payload)?
                        .iter()
                        .any(|app| app["appId"] == APPLICATION && app["transportId"] == transport)
                    {
                        self.owns_application = false;
                        return Err("another controller changed the Cast application".into());
                    }
                    continue;
                }
                if message.payload["type"] != "MEDIA_STATUS" {
                    return Err("Cast receiver reported a media error".into());
                }
                let status = media_status(&message.payload)?;
                if status["mediaSessionId"].as_u64() != Some(session) {
                    self.owns_application = false;
                    return Err("another controller changed the Cast media session".into());
                }
                if check_content(status, url).is_err() {
                    self.owns_application = false;
                    return Err("another controller changed the Cast media".into());
                }
                last_status = Instant::now();
                finished = playback_state(status)?;
            }
        }
        self.owns_application = false;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), String> {
        if !self.owns_application {
            return Ok(());
        }
        if let (Some(transport), Some(session)) = (self.application.clone(), self.session) {
            let id = self
                .request(
                    &transport,
                    MEDIA,
                    json!({"type":"STOP", "mediaSessionId":session}),
                )
                .await?;
            let reply = self
                .reply(
                    id,
                    MEDIA,
                    "MEDIA_STATUS",
                    Instant::now() + Duration::from_secs(2),
                )
                .await?;
            let status = media_status(&reply)?;
            if status["mediaSessionId"].as_u64() != Some(session) || status["playerState"] != "IDLE"
            {
                return Err("Cast receiver did not confirm Stop for the owned session".into());
            }
        } else {
            return Err(
                "Cast load may have started, but no media session is available to confirm Stop"
                    .into(),
            );
        }
        Ok(())
    }
}

fn applications(payload: &Value) -> Result<&[Value], String> {
    let status = payload
        .get("status")
        .filter(|status| status.is_object())
        .ok_or("invalid Cast receiver status")?;
    match status.get("applications") {
        None => Ok(&[]),
        Some(value) => value
            .as_array()
            .filter(|apps| apps.len() <= 16)
            .map(Vec::as_slice)
            .ok_or_else(|| "invalid Cast application list".into()),
    }
}

fn routing_id(value: &Value) -> Result<String, String> {
    let text = value.as_str().ok_or("missing Cast application transport")?;
    if text.is_empty()
        || text.len() > 128
        || !text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || text == "receiver-0"
        || text == "*"
    {
        return Err("invalid Cast application transport".into());
    }
    Ok(text.into())
}

fn media_status(payload: &Value) -> Result<&Value, String> {
    let statuses = payload["status"]
        .as_array()
        .filter(|statuses| statuses.len() == 1)
        .ok_or("Cast media status must describe exactly one session")?;
    statuses
        .first()
        .filter(|status| status.is_object())
        .ok_or_else(|| "invalid Cast media status".into())
}

fn check_content(status: &Value, url: &str) -> Result<(), String> {
    if let Some(media) = status.get("media")
        && media["contentId"].as_str() != Some(url)
    {
        return Err("Cast media identity changed".into());
    }
    Ok(())
}

fn playback_state(status: &Value) -> Result<bool, String> {
    match status["playerState"].as_str() {
        Some("PLAYING" | "BUFFERING" | "PAUSED") => Ok(false),
        Some("IDLE") if status["idleReason"] == "FINISHED" => Ok(true),
        Some("IDLE") => Err("Cast playback ended without a FINISHED acknowledgement".into()),
        _ => Err("invalid Cast playback state".into()),
    }
}

async fn cancelled(signal: &mut watch::Receiver<bool>) {
    loop {
        if *signal.borrow() || signal.changed().await.is_err() {
            break;
        }
    }
}

pub(super) async fn run(
    bind: Ipv4Addr,
    target: &str,
    pin: &str,
    tracks: &[DeviceTrack],
    urls: &[String],
    replace: bool,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    preflight(tracks)?;
    let peer = endpoint(target)?;
    let stream = tokio::select! {
        biased;
        _ = cancelled(&mut shutdown) => return Ok(()),
        result = cast_tls::connect(bind, *peer.ip(), peer.port(), cast_tls::parse_pin(pin)?) => result?,
    };
    run_stream(stream, tracks, urls, replace, shutdown).await
}

async fn run_stream<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    stream: S,
    tracks: &[DeviceTrack],
    urls: &[String],
    replace: bool,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    if tracks.is_empty() || tracks.len() > 64 || tracks.len() != urls.len() {
        return Err("Cast playback needs one to 64 tracks and matching media URLs".into());
    }
    let sender = format!("sender-{}", aede_core::accounts::random_token()?);
    let mut controller = Controller::new(stream, sender);
    let result = tokio::select! {
        biased;
        _ = cancelled(&mut shutdown) => Ok(()),
        result = controller.play(tracks, urls, replace) => result,
    };
    match timeout(Duration::from_secs(3), controller.stop()).await {
        Ok(Ok(())) => result,
        stop => {
            let error = match stop {
                Ok(Err(error)) => error,
                _ => "Cast Stop timed out".into(),
            };
            match result {
                Ok(()) => Err(error),
                Err(playback) => Err(format!("{playback}; {error}")),
            }
        }
    }
}

#[cfg(test)]
#[path = "googlecast_tests.rs"]
mod tests;
