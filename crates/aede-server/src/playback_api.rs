//! Authenticated, bounded PCM playback for remote clients.
//!
//! This module transports processed PCM only. It deliberately does not own a
//! device, a queue, seek state, or a second audio pipeline: decoding, stereo
//! downmix, normalization, tone controls, rate conversion, and the final
//! output guard come from `aede-core` and `aede-dsp`, just as they do for local
//! playback. The client acknowledges cumulative output frames, so listening
//! history represents audio it consumed rather than audio the server merely
//! wrote to a socket buffer.

use std::convert::Infallible;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use aede_core::clock;
use aede_core::model::{AudioFile, EntityKind};
use aede_core::playback::gain_plan::ReadyNormalization;
use aede_core::playback::normalization::Mode;
use aede_core::playback::session::PcmSession;
use aede_core::playback::stream::PcmTrack;
use aede_core::store;
use aede_core::store_lock::StoreLock;
use aede_core::user::{self, EntityRef, Play, UserData};
use aede_dsp::{ToneControls, gain_with_headroom_db};
use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::*;

const MAX_CONTROL_MESSAGE_BYTES: usize = 4 * 1024;
const MAX_AUDIO_BYTES: usize = 32 * 1024;
const PRODUCER_EVENTS: usize = 4;
const MIN_SAMPLE_RATE: u32 = 8_000;
const MAX_SAMPLE_RATE: u32 = 192_000;
const MAX_UNACKNOWLEDGED_MILLISECONDS: u64 = 1_000;
const START_TIMEOUT: Duration = Duration::from_secs(10);
const SOURCE_READY_TIMEOUT: Duration = Duration::from_secs(10);
const ACK_TIMEOUT: Duration = Duration::from_secs(10);
const PRODUCER_PROGRESS_TIMEOUT: Duration = Duration::from_secs(10);
const AUTH_RECHECK_INTERVAL: Duration = Duration::from_secs(1);
const HISTORY_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

/// Registers the account-scoped remote playback transport.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/me/v1/playback", get(playback))
}

async fn playback(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
    request: Request,
) -> Result<Response, ApiError> {
    personal::no_query(&request)?;
    let principal = auth::request_principal(&request)?;
    // Playback creates private listening history, so the explicit read-only
    // role must be refused before the protocol is upgraded.
    auth::require_mutation(&principal)?;
    let permit = state
        .playback_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "playback_busy",
                "too many active playback streams; retry shortly",
            )
        })?;
    let shutdown = state.shutdown.subscribe();
    Ok(ws
        .max_message_size(MAX_CONTROL_MESSAGE_BYTES)
        .max_frame_size(MAX_CONTROL_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            playback_stream(socket, state, principal, permit, shutdown).await;
        }))
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum InitialFrame {
    Start {
        track: String,
        #[serde(default)]
        normalize: Normalize,
        sample_rate: Option<u32>,
        bass: Option<f32>,
        treble: Option<f32>,
    },
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Normalize {
    Off,
    #[default]
    Track,
    Album,
}

impl Normalize {
    fn mode(self) -> Mode {
        match self {
            Self::Off => Mode::Off,
            Self::Track => Mode::Track,
            Self::Album => Mode::Album,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ClientFrame {
    Ack { frames: u64 },
}

struct Start {
    reference: EntityRef,
    normalize: Mode,
    output_rate: Option<u32>,
    tone: ToneControls,
}

impl Start {
    fn parse(input: &str) -> Result<Self, StreamFailure> {
        let InitialFrame::Start {
            track,
            normalize,
            sample_rate,
            bass,
            treble,
        } = serde_json::from_str(input).map_err(|_| StreamFailure::INVALID_START)?;
        let reference = EntityRef::parse_token(&track)
            .filter(|reference| reference.kind == EntityKind::Track && !reference.key.is_empty())
            .ok_or(StreamFailure::INVALID_START)?;
        if sample_rate.is_some_and(|rate| !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&rate)) {
            return Err(StreamFailure::INVALID_START);
        }
        let bass = bass.unwrap_or(0.0);
        let treble = treble.unwrap_or(0.0);
        let tone = ToneControls::new(bass, treble).map_err(|_| StreamFailure::INVALID_START)?;
        Ok(Self {
            reference,
            normalize: normalize.mode(),
            output_rate: sample_rate,
            tone,
        })
    }
}

#[derive(Clone)]
pub(super) struct TrackSource {
    pub(super) reference: EntityRef,
    pub(super) path: PathBuf,
    pub(super) file: AudioFile,
    pub(super) mtime_subseconds: u32,
}

pub(super) fn source_from_catalog(
    catalog: &Catalog,
    requested: &EntityRef,
) -> Result<TrackSource, StreamFailure> {
    if requested.kind != EntityKind::Track {
        return Err(StreamFailure::TRACK_NOT_FOUND);
    }
    let track_id = requested
        .resolve(catalog)
        .ok_or(StreamFailure::TRACK_NOT_FOUND)?;
    let track = catalog
        .track(track_id)
        .ok_or(StreamFailure::TRACK_NOT_FOUND)?;
    let file = catalog
        .file(track.file_id)
        .ok_or(StreamFailure::TRACK_NOT_FOUND)?;
    let mtime_subseconds = catalog
        .file_mtime_subseconds
        .get(&file.path)
        .copied()
        // A legacy whole-second identity is not precise enough to safely
        // attribute a remote listen. A normal scan upgrades it.
        .ok_or(StreamFailure::SOURCE_CHANGED)?;
    let reference = EntityRef::of(catalog, EntityKind::Track, track_id)
        .ok_or(StreamFailure::TRACK_NOT_FOUND)?;
    Ok(TrackSource {
        reference,
        path: PathBuf::from(&file.path),
        file: file.clone(),
        mtime_subseconds,
    })
}

async fn current_source(
    state: &ApiState,
    requested: &EntityRef,
) -> Result<(TrackSource, Catalog), StreamFailure> {
    let catalog = state.catalog.read().await;
    let catalog = catalog.as_ref().ok_or(StreamFailure::CATALOG_UNAVAILABLE)?;
    let source = source_from_catalog(catalog, requested)?;
    // ReadyNormalization needs catalog analyses only for a one-track stream.
    // Retaining the full graph here would copy a potentially huge library per
    // active client, so keep just analyses that can describe this exact file.
    let normalization_catalog = Catalog {
        analyses: catalog
            .analyses
            .iter()
            .filter(|analysis| analysis.path == source.file.path)
            .cloned()
            .collect(),
        ..Catalog::default()
    };
    Ok((source, normalization_catalog))
}

/// File identity is checked around opening and decoding. This cannot make a
/// pathname-based decoder immune to every operating-system race, but it
/// refuses links and special files, never accepts a caller-supplied path, and
/// detects a replacement before it can become an acknowledged listen.
pub(super) fn validate_source(source: &TrackSource) -> Result<(), StreamFailure> {
    let metadata = fs::symlink_metadata(&source.path).map_err(|failure| {
        if failure.kind() == ErrorKind::NotFound {
            StreamFailure::SOURCE_UNAVAILABLE
        } else {
            StreamFailure::SOURCE_CHANGED
        }
    })?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() != source.file.size
        || clock::mtime_seconds(&metadata) != source.file.mtime
        || clock::mtime_subseconds(&metadata) != source.mtime_subseconds
    {
        return Err(StreamFailure::SOURCE_CHANGED);
    }
    Ok(())
}

struct ProducerSettings {
    output_rate: Option<u32>,
    tone: ToneControls,
    normalize: Mode,
    normalization_catalog: Catalog,
    data_dir: PathBuf,
}

fn settings(normalization_catalog: Catalog, data_dir: PathBuf, start: &Start) -> ProducerSettings {
    ProducerSettings {
        output_rate: start.output_rate,
        tone: start.tone,
        normalize: start.normalize,
        normalization_catalog,
        data_dir,
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct StreamFailure {
    code: &'static str,
    message: &'static str,
}

impl StreamFailure {
    const INVALID_START: Self = Self {
        code: "invalid_start",
        message: "the first message must be a valid playback start request",
    };
    const INVALID_ACK: Self = Self {
        code: "invalid_ack",
        message: "an acknowledgement must be monotonic and cannot exceed sent frames",
    };
    const TRACK_NOT_FOUND: Self = Self {
        code: "track_not_found",
        message: "the requested catalog track is unavailable",
    };
    const CATALOG_UNAVAILABLE: Self = Self {
        code: "catalog_unavailable",
        message: "the catalog is not available for playback",
    };
    const SOURCE_CHANGED: Self = Self {
        code: "source_changed",
        message: "the catalogued audio source changed; scan it again before streaming",
    };
    const SOURCE_UNAVAILABLE: Self = Self {
        code: "source_unavailable",
        message: "the catalogued audio source is unavailable",
    };
    const DECODE_FAILED: Self = Self {
        code: "decode_failed",
        message: "the catalogued audio source could not be decoded",
    };
    const PROCESSING_FAILED: Self = Self {
        code: "processing_failed",
        message: "the audio stream could not be processed safely",
    };
    const AUTHENTICATION_EXPIRED: Self = Self {
        code: "authentication_expired",
        message: "the account session expired, was revoked, or no longer permits playback",
    };
    const ACK_TIMEOUT: Self = Self {
        code: "ack_timeout",
        message: "the client did not acknowledge audio consumption in time",
    };
    const HISTORY_FAILED: Self = Self {
        code: "history_failed",
        message: "the listening history could not be saved",
    };
    const SERVER_SHUTDOWN: Self = Self {
        code: "server_shutdown",
        message: "the server is shutting down",
    };
    const STREAM_FAILED: Self = Self {
        code: "stream_failed",
        message: "the playback worker stopped unexpectedly",
    };
}

#[derive(Serialize)]
struct FormatFrame {
    #[serde(rename = "type")]
    kind: &'static str,
    track: String,
    encoding: &'static str,
    sample_rate: u32,
    channels: u16,
    duration_ms: Option<u64>,
    max_unacknowledged_frames: u64,
}

#[derive(Serialize)]
struct EofFrame {
    #[serde(rename = "type")]
    kind: &'static str,
    frames: u64,
}

#[derive(Serialize)]
struct ErrorFrame {
    #[serde(rename = "type")]
    kind: &'static str,
    code: &'static str,
    message: &'static str,
}

#[derive(Serialize)]
struct RecordedFrame {
    #[serde(rename = "type")]
    kind: &'static str,
    ms_played: u64,
    completed: bool,
}

enum ProducerEvent {
    Format(FormatFrame),
    Audio { bytes: Vec<u8>, frames: u64 },
    Eof { frames: u64 },
    Error(StreamFailure),
}

enum ProducerStop {
    Cancelled,
    Failed(StreamFailure),
}

fn emit(
    sender: &mpsc::Sender<ProducerEvent>,
    cancelled: &AtomicBool,
    event: ProducerEvent,
) -> Result<(), ProducerStop> {
    if cancelled.load(AtomicOrdering::Acquire) {
        return Err(ProducerStop::Cancelled);
    }
    sender
        .blocking_send(event)
        .map_err(|_| ProducerStop::Cancelled)
}

fn emit_block(
    sender: &mpsc::Sender<ProducerEvent>,
    cancelled: &AtomicBool,
    bytes: &[u8],
    channels: u16,
    max_pending: u64,
    sent: &mut u64,
) -> Result<(), ProducerStop> {
    let frame_bytes = usize::from(channels)
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
    if frame_bytes == 0 || !bytes.len().is_multiple_of(frame_bytes) {
        return Err(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED));
    }
    let max_frames = (MAX_AUDIO_BYTES / frame_bytes).min(
        usize::try_from(max_pending)
            .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?,
    );
    if max_frames == 0 {
        return Err(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED));
    }
    for chunk in bytes.chunks(max_frames * frame_bytes) {
        if cancelled.load(AtomicOrdering::Acquire) {
            return Err(ProducerStop::Cancelled);
        }
        let frames = u64::try_from(chunk.len() / frame_bytes)
            .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
        *sent = sent
            .checked_add(frames)
            .ok_or(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
        emit(
            sender,
            cancelled,
            ProducerEvent::Audio {
                bytes: chunk.to_vec(),
                frames,
            },
        )?;
    }
    Ok(())
}

fn produce(
    source: TrackSource,
    settings: ProducerSettings,
    sender: mpsc::Sender<ProducerEvent>,
    cancelled: std::sync::Arc<AtomicBool>,
) -> Result<(), ProducerStop> {
    validate_source(&source).map_err(ProducerStop::Failed)?;
    let mut track = PcmTrack::open_stereo(&source.path)
        .map_err(|_| ProducerStop::Failed(StreamFailure::DECODE_FAILED))?;
    validate_source(&source).map_err(ProducerStop::Failed)?;
    // Reuse the local player's policy for tags, imported analyses and cached
    // measurements. This server stream intentionally never observes or saves
    // a new measurement: a single requested track is not a complete album
    // programme and a disconnected client cannot prove full source playback.
    let normalization_paths = vec![source.path.clone()];
    let mut normalization = ReadyNormalization::new(
        &normalization_paths,
        false,
        Some(&settings.normalization_catalog),
        &settings.data_dir,
        settings.normalize,
    )
    .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
    let selected_gain = normalization
        .prepare(0)
        .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
    validate_source(&source).map_err(ProducerStop::Failed)?;
    let requested_gain =
        selected_gain.map_or(0.0, |gain| gain.gain_db) + settings.tone.safe_preamp_db();
    let gain_db = gain_with_headroom_db(
        requested_gain,
        selected_gain.and_then(|gain| gain.source_peak),
    )
    .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
    let output_rate = settings
        .output_rate
        .unwrap_or_else(|| track.format().sample_rate().min(MAX_SAMPLE_RATE));
    let format = track.format();
    let mut session = PcmSession::new(format, output_rate, settings.tone)
        .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
    session
        .begin_track(0, gain_db)
        .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
    let output = session.output_format();
    let max_unacknowledged_frames =
        u64::from(output.sample_rate()).saturating_mul(MAX_UNACKNOWLEDGED_MILLISECONDS) / 1_000;
    emit(
        &sender,
        &cancelled,
        ProducerEvent::Format(FormatFrame {
            kind: "format",
            track: source.reference.to_token(),
            encoding: "f32le",
            sample_rate: output.sample_rate(),
            channels: output.channels(),
            duration_ms: source.file.properties.duration_ms,
            max_unacknowledged_frames,
        }),
    )?;
    let mut sent = 0_u64;
    loop {
        if cancelled.load(AtomicOrdering::Acquire) {
            return Err(ProducerStop::Cancelled);
        }
        let raw = track
            .read_block(|_| Ok::<(), Infallible>(()))
            .map_err(|_| ProducerStop::Failed(StreamFailure::DECODE_FAILED))?;
        let Some(raw) = raw else {
            let ending = session
                .end_track()
                .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
            emit_block(
                &sender,
                &cancelled,
                ending.f32le,
                output.channels(),
                max_unacknowledged_frames,
                &mut sent,
            )?;
            let tail = session
                .finish()
                .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
            emit_block(
                &sender,
                &cancelled,
                tail.f32le,
                output.channels(),
                max_unacknowledged_frames,
                &mut sent,
            )?;
            validate_source(&source).map_err(ProducerStop::Failed)?;
            emit(&sender, &cancelled, ProducerEvent::Eof { frames: sent })?;
            return Ok(());
        };
        let processed = session
            .push_source(raw.samples)
            .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
        emit_block(
            &sender,
            &cancelled,
            processed.f32le,
            output.channels(),
            max_unacknowledged_frames,
            &mut sent,
        )?;
    }
}

fn spawn_producer(
    source: TrackSource,
    settings: ProducerSettings,
    cancelled: std::sync::Arc<AtomicBool>,
) -> (mpsc::Receiver<ProducerEvent>, tokio::task::JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel(PRODUCER_EVENTS);
    let worker = tokio::task::spawn_blocking(move || {
        match produce(source, settings, sender.clone(), cancelled) {
            Ok(()) | Err(ProducerStop::Cancelled) => {}
            Err(ProducerStop::Failed(failure)) => {
                let _ = sender.blocking_send(ProducerEvent::Error(failure));
            }
        }
    });
    (receiver, worker)
}

#[derive(Default)]
struct Acknowledgements {
    sent: u64,
    consumed: u64,
    max_pending: u64,
    waiting_since: Option<Instant>,
}

impl Acknowledgements {
    fn with_window(max_pending: u64) -> Self {
        Self {
            max_pending,
            ..Self::default()
        }
    }

    fn available(&self) -> u64 {
        self.max_pending
            .saturating_sub(self.sent.saturating_sub(self.consumed))
    }

    fn sent(&mut self, frames: u64, now: Instant) -> Result<(), StreamFailure> {
        self.sent = self
            .sent
            .checked_add(frames)
            .ok_or(StreamFailure::PROCESSING_FAILED)?;
        if self.sent > self.consumed && self.waiting_since.is_none() {
            self.waiting_since = Some(now);
        }
        Ok(())
    }

    fn acknowledge(&mut self, frames: u64, now: Instant) -> Result<bool, StreamFailure> {
        if frames < self.consumed || frames > self.sent {
            return Err(StreamFailure::INVALID_ACK);
        }
        if frames > self.consumed {
            self.consumed = frames;
            self.waiting_since = (self.sent > self.consumed).then_some(now);
            return Ok(true);
        }
        Ok(false)
    }

    fn deadline(&self) -> Option<Instant> {
        self.waiting_since.map(|waiting| waiting + ACK_TIMEOUT)
    }
}

struct DriveEnd {
    acknowledged_frames: u64,
    sample_rate: Option<u32>,
    completed: bool,
    failure: Option<StreamFailure>,
    notify: bool,
}

impl DriveEnd {
    fn stopped(
        acknowledgements: &Acknowledgements,
        sample_rate: Option<u32>,
        failure: Option<StreamFailure>,
        notify: bool,
    ) -> Self {
        Self {
            acknowledged_frames: acknowledgements.consumed,
            sample_rate,
            completed: false,
            failure,
            notify,
        }
    }
}

async fn receive_start(
    socket: &mut WebSocket,
    state: &ApiState,
    captured: &auth::Principal,
    shutdown: &mut broadcast::Receiver<()>,
) -> Result<Start, Option<StreamFailure>> {
    let mut authorization = tokio::time::interval(AUTH_RECHECK_INTERVAL);
    authorization.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let deadline = tokio::time::sleep(START_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => return Err(Some(StreamFailure::INVALID_START)),
            _ = shutdown.recv() => return Err(Some(StreamFailure::SERVER_SHUTDOWN)),
            _ = authorization.tick() => {
                if !authorized(state, captured, false).await {
                    return Err(Some(StreamFailure::AUTHENTICATION_EXPIRED));
                }
            }
            incoming = socket.recv() => {
                let Some(incoming) = incoming else {
                    return Err(None);
                };
                let incoming = match incoming {
                    Ok(incoming) => incoming,
                    Err(_) => return Err(None),
                };
                match incoming {
                    Message::Text(input) => return Start::parse(&input).map_err(Some),
                    Message::Ping(payload) => {
                        if events::send_message(socket, Message::Pong(payload)).await.is_err() {
                            return Err(None);
                        }
                    }
                    Message::Pong(_) => {}
                    Message::Close(_) => return Err(None),
                    Message::Binary(_) => return Err(Some(StreamFailure::INVALID_START)),
                }
            }
        }
    }
}

async fn wait_for_format(socket: &mut WebSocket) -> Result<bool, StreamFailure> {
    let Some(incoming) = socket.recv().await else {
        return Ok(false);
    };
    let incoming = incoming.map_err(|_| StreamFailure::STREAM_FAILED)?;
    match incoming {
        Message::Ping(payload) => {
            events::send_message(socket, Message::Pong(payload))
                .await
                .map_err(|_| StreamFailure::STREAM_FAILED)?;
            Ok(true)
        }
        Message::Pong(_) => Ok(true),
        Message::Close(_) => Ok(false),
        Message::Text(_) | Message::Binary(_) => Err(StreamFailure::INVALID_ACK),
    }
}

async fn authorized(state: &ApiState, captured: &auth::Principal, touch: bool) -> bool {
    let Ok(Some(accounts)) = auth::current_accounts(state).await else {
        return false;
    };
    let Ok(principal) = auth::principal(
        state,
        &accounts,
        auth::session_token(captured),
        touch,
        Instant::now(),
    ) else {
        return false;
    };
    auth::require_mutation(&principal).is_ok()
}

async fn send_json(socket: &mut WebSocket, value: impl Serialize) -> Result<(), ()> {
    let payload = serde_json::to_string(&value).map_err(|_| ())?;
    events::send_message(socket, Message::Text(payload)).await
}

async fn send_failure(socket: &mut WebSocket, failure: StreamFailure) {
    let _ = send_json(
        socket,
        ErrorFrame {
            kind: "error",
            code: failure.code,
            message: failure.message,
        },
    )
    .await;
    let _ = events::send_message(
        socket,
        Message::Close(Some(CloseFrame {
            code: close_code::POLICY,
            reason: "playback stream stopped".into(),
        })),
    )
    .await;
}

async fn send_close(socket: &mut WebSocket) {
    let _ = events::send_message(socket, Message::Close(None)).await;
}

async fn keep_authorized(
    state: &ApiState,
    captured: &auth::Principal,
    last_check: &mut Instant,
    force: bool,
) -> bool {
    if force || last_check.elapsed() >= AUTH_RECHECK_INTERVAL {
        if !authorized(state, captured, false).await {
            return false;
        }
        *last_check = Instant::now();
    }
    true
}

async fn receive_ack(
    socket: &mut WebSocket,
    acknowledgements: &mut Acknowledgements,
) -> Result<bool, StreamFailure> {
    let Some(incoming) = socket.recv().await else {
        return Ok(false);
    };
    let incoming = incoming.map_err(|_| StreamFailure::STREAM_FAILED)?;
    match incoming {
        Message::Text(input) => {
            let ClientFrame::Ack { frames } =
                serde_json::from_str(&input).map_err(|_| StreamFailure::INVALID_ACK)?;
            acknowledgements.acknowledge(frames, Instant::now())?;
            Ok(true)
        }
        Message::Ping(payload) => {
            events::send_message(socket, Message::Pong(payload))
                .await
                .map_err(|_| StreamFailure::STREAM_FAILED)?;
            Ok(true)
        }
        Message::Pong(_) => Ok(true),
        Message::Close(_) => Ok(false),
        Message::Binary(_) => Err(StreamFailure::INVALID_ACK),
    }
}

async fn drive(
    socket: &mut WebSocket,
    state: &ApiState,
    captured: &auth::Principal,
    receiver: &mut mpsc::Receiver<ProducerEvent>,
    shutdown: &mut broadcast::Receiver<()>,
) -> DriveEnd {
    let mut authorization = tokio::time::interval(AUTH_RECHECK_INTERVAL);
    authorization.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_authentication = Instant::now();
    let source_deadline = tokio::time::sleep(SOURCE_READY_TIMEOUT);
    tokio::pin!(source_deadline);
    let initial = loop {
        tokio::select! {
            event = receiver.recv() => match event {
                Some(event) => break event,
                None => return DriveEnd {
                    acknowledged_frames: 0,
                    sample_rate: None,
                    completed: false,
                    failure: Some(StreamFailure::STREAM_FAILED),
                    notify: true,
                },
            },
            _ = authorization.tick() => {
                if !keep_authorized(state, captured, &mut last_authentication, true).await {
                    return DriveEnd {
                        acknowledged_frames: 0,
                        sample_rate: None,
                        completed: false,
                        failure: Some(StreamFailure::AUTHENTICATION_EXPIRED),
                        notify: true,
                    };
                }
            }
            _ = shutdown.recv() => return DriveEnd {
                acknowledged_frames: 0,
                sample_rate: None,
                completed: false,
                failure: Some(StreamFailure::SERVER_SHUTDOWN),
                notify: true,
            },
            incoming = wait_for_format(socket) => match incoming {
                Ok(true) => {}
                Ok(false) => return DriveEnd {
                    acknowledged_frames: 0,
                    sample_rate: None,
                    completed: false,
                    failure: None,
                    notify: false,
                },
                Err(failure) => return DriveEnd {
                    acknowledged_frames: 0,
                    sample_rate: None,
                    completed: false,
                    failure: Some(failure),
                    notify: true,
                },
            },
            _ = &mut source_deadline => return DriveEnd {
                acknowledged_frames: 0,
                sample_rate: None,
                completed: false,
                failure: Some(StreamFailure::SOURCE_UNAVAILABLE),
                notify: true,
            },
        }
    };
    let format = match initial {
        ProducerEvent::Format(format) => format,
        ProducerEvent::Error(failure) => {
            return DriveEnd {
                acknowledged_frames: 0,
                sample_rate: None,
                completed: false,
                failure: Some(failure),
                notify: true,
            };
        }
        ProducerEvent::Audio { .. } | ProducerEvent::Eof { .. } => {
            return DriveEnd {
                acknowledged_frames: 0,
                sample_rate: None,
                completed: false,
                failure: Some(StreamFailure::STREAM_FAILED),
                notify: true,
            };
        }
    };
    let mut acknowledgements = Acknowledgements::with_window(format.max_unacknowledged_frames);
    if !keep_authorized(state, captured, &mut last_authentication, true).await {
        return DriveEnd::stopped(
            &acknowledgements,
            Some(format.sample_rate),
            Some(StreamFailure::AUTHENTICATION_EXPIRED),
            true,
        );
    }
    if send_json(socket, &format).await.is_err() {
        return DriveEnd::stopped(&acknowledgements, Some(format.sample_rate), None, false);
    }
    let mut pending = None;
    let mut eof = None;
    let mut producer_progress = Instant::now();
    loop {
        if let Some(event) = pending.take() {
            match event {
                ProducerEvent::Audio {
                    mut bytes,
                    mut frames,
                } => {
                    let accepted = frames.min(acknowledgements.available());
                    if accepted == 0 {
                        // The window is full. Requeue this exact producer
                        // block and fall through to `select!` so an ACK,
                        // shutdown, revocation or timeout can make progress.
                        pending = Some(ProducerEvent::Audio { bytes, frames });
                    } else {
                        if accepted < frames {
                            let frame_count =
                                usize::try_from(frames).ok().filter(|count| *count > 0);
                            let frame_bytes = frame_count
                                .and_then(|count| {
                                    bytes.len().checked_div(count).map(|width| (count, width))
                                })
                                .filter(|(count, width)| {
                                    *width > 0
                                        && count
                                            .checked_mul(*width)
                                            .is_some_and(|length| length == bytes.len())
                                })
                                .map(|(_, width)| width)
                                .ok_or(StreamFailure::PROCESSING_FAILED);
                            let Ok(frame_bytes) = frame_bytes else {
                                return DriveEnd::stopped(
                                    &acknowledgements,
                                    Some(format.sample_rate),
                                    Some(StreamFailure::PROCESSING_FAILED),
                                    true,
                                );
                            };
                            let split = usize::try_from(accepted)
                                .ok()
                                .and_then(|count| count.checked_mul(frame_bytes));
                            let Some(split) = split.filter(|split| *split <= bytes.len()) else {
                                return DriveEnd::stopped(
                                    &acknowledgements,
                                    Some(format.sample_rate),
                                    Some(StreamFailure::PROCESSING_FAILED),
                                    true,
                                );
                            };
                            let remaining = bytes.split_off(split);
                            frames -= accepted;
                            pending = Some(ProducerEvent::Audio {
                                bytes: remaining,
                                frames,
                            });
                            frames = accepted;
                        }
                        if !keep_authorized(state, captured, &mut last_authentication, false).await
                        {
                            return DriveEnd::stopped(
                                &acknowledgements,
                                Some(format.sample_rate),
                                Some(StreamFailure::AUTHENTICATION_EXPIRED),
                                true,
                            );
                        }
                        if events::send_message(socket, Message::Binary(bytes))
                            .await
                            .is_err()
                        {
                            return DriveEnd::stopped(
                                &acknowledgements,
                                Some(format.sample_rate),
                                None,
                                false,
                            );
                        }
                        if acknowledgements.sent(frames, Instant::now()).is_err() {
                            return DriveEnd::stopped(
                                &acknowledgements,
                                Some(format.sample_rate),
                                Some(StreamFailure::PROCESSING_FAILED),
                                true,
                            );
                        }
                        // Releasing this buffered item lets the decoder continue;
                        // give it a fresh bounded interval to produce the next one.
                        producer_progress = Instant::now();
                        continue;
                    }
                }
                ProducerEvent::Eof { frames } => {
                    if frames != acknowledgements.sent {
                        return DriveEnd::stopped(
                            &acknowledgements,
                            Some(format.sample_rate),
                            Some(StreamFailure::STREAM_FAILED),
                            true,
                        );
                    }
                    if !keep_authorized(state, captured, &mut last_authentication, false).await {
                        return DriveEnd::stopped(
                            &acknowledgements,
                            Some(format.sample_rate),
                            Some(StreamFailure::AUTHENTICATION_EXPIRED),
                            true,
                        );
                    }
                    if send_json(
                        socket,
                        EofFrame {
                            kind: "eof",
                            frames,
                        },
                    )
                    .await
                    .is_err()
                    {
                        return DriveEnd::stopped(
                            &acknowledgements,
                            Some(format.sample_rate),
                            None,
                            false,
                        );
                    }
                    eof = Some(frames);
                    continue;
                }
                ProducerEvent::Format(_) => {
                    return DriveEnd::stopped(
                        &acknowledgements,
                        Some(format.sample_rate),
                        Some(StreamFailure::STREAM_FAILED),
                        true,
                    );
                }
                ProducerEvent::Error(failure) => {
                    return DriveEnd::stopped(
                        &acknowledgements,
                        Some(format.sample_rate),
                        Some(failure),
                        true,
                    );
                }
            }
        }
        if eof.is_some_and(|frames| frames == acknowledgements.consumed) {
            return DriveEnd {
                acknowledged_frames: acknowledgements.consumed,
                sample_rate: Some(format.sample_rate),
                completed: true,
                failure: None,
                notify: false,
            };
        }
        let deadline = acknowledgements
            .deadline()
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(24 * 60 * 60));
        let acknowledgement_timeout =
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
        tokio::pin!(acknowledgement_timeout);
        let progress_timeout = tokio::time::sleep_until(tokio::time::Instant::from_std(
            producer_progress + PRODUCER_PROGRESS_TIMEOUT,
        ));
        tokio::pin!(progress_timeout);
        let consumed_before_receive = acknowledgements.consumed;
        tokio::select! {
            _ = &mut acknowledgement_timeout, if acknowledgements.deadline().is_some() => {
                return DriveEnd::stopped(
                    &acknowledgements,
                    Some(format.sample_rate),
                    Some(StreamFailure::ACK_TIMEOUT),
                    true,
                );
            }
            _ = &mut progress_timeout,
                if pending.is_none()
                    && eof.is_none()
                    && acknowledgements.sent == acknowledgements.consumed =>
            {
                return DriveEnd::stopped(
                    &acknowledgements,
                    Some(format.sample_rate),
                    Some(StreamFailure::SOURCE_UNAVAILABLE),
                    true,
                );
            }
            _ = authorization.tick() => {
                if !keep_authorized(state, captured, &mut last_authentication, true).await {
                    return DriveEnd::stopped(
                        &acknowledgements,
                        Some(format.sample_rate),
                        Some(StreamFailure::AUTHENTICATION_EXPIRED),
                        true,
                    );
                }
            }
            _ = shutdown.recv() => {
                return DriveEnd::stopped(
                    &acknowledgements,
                    Some(format.sample_rate),
                    Some(StreamFailure::SERVER_SHUTDOWN),
                    true,
                );
            }
            incoming = receive_ack(socket, &mut acknowledgements) => match incoming {
                Ok(true) => {
                    // A growing ACK can reopen a fresh producer wait after a
                    // long client pause. Pings and duplicate ACKs leave this
                    // deadline untouched.
                    if acknowledgements.consumed > consumed_before_receive {
                        // The ACK is committed only by the selected branch.
                        // Awaiting this read inside `receive_ack` would let
                        // another ready producer event cancel it after the
                        // socket message and frame count were already consumed.
                        if !authorized(state, captured, true).await {
                            return DriveEnd::stopped(
                                &acknowledgements,
                                Some(format.sample_rate),
                                Some(StreamFailure::AUTHENTICATION_EXPIRED),
                                true,
                            );
                        }
                        producer_progress = Instant::now();
                    }
                }
                Ok(false) => return DriveEnd::stopped(
                    &acknowledgements,
                    Some(format.sample_rate),
                    None,
                    false,
                ),
                Err(failure) => return DriveEnd::stopped(
                    &acknowledgements,
                    Some(format.sample_rate),
                    Some(failure),
                    true,
                ),
            },
            event = receiver.recv(), if pending.is_none() && eof.is_none() => match event {
                Some(event) => {
                    producer_progress = Instant::now();
                    pending = Some(event);
                }
                None => return DriveEnd::stopped(
                    &acknowledgements,
                    Some(format.sample_rate),
                    Some(StreamFailure::STREAM_FAILED),
                    true,
                ),
            },
        }
    }
}

fn frames_to_milliseconds(frames: u64, sample_rate: u32) -> u64 {
    let milliseconds = u128::from(frames).saturating_mul(1_000) / u128::from(sample_rate);
    u64::try_from(milliseconds).unwrap_or(u64::MAX)
}

fn history_error() -> ApiError {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "history_failed",
        "the listening history could not be saved",
    )
}

fn lock_for_history(data_dir: &Path) -> Result<StoreLock, ApiError> {
    // A catalog watcher may briefly own the shared store lock immediately
    // after the stream ends. Keep the once-per-socket history record pending
    // for a short, bounded interval instead of silently losing a valid listen.
    let deadline = Instant::now() + HISTORY_WRITE_TIMEOUT;
    loop {
        match StoreLock::try_acquire(data_dir) {
            Ok(lock) => return Ok(lock),
            Err(failure)
                if failure.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => return Err(history_error()),
        }
    }
}

fn load_history(data_dir: &Path, catalog: &Catalog) -> Result<UserData, ApiError> {
    let mut data = user::load(&user::user_path(data_dir))
        .map_err(|failure| {
            eprintln!("API playback history read failed: {failure}");
            history_error()
        })?
        .unwrap_or_default();
    user::reconcile(&mut data, catalog);
    Ok(data)
}

fn persist_listen(
    state: &ApiState,
    captured: &auth::Principal,
    original: &TrackSource,
    started_at: u64,
    ms_played: u64,
    completed: bool,
) -> Result<(), ApiError> {
    let _guard = lock_for_history(&state.data_dir)?;
    // Recheck under the writer lock. A disablement, role downgrade, session
    // revocation, or unreadable credentials wins over a late history update.
    let accounts = auth::load_accounts(state)?.ok_or_else(auth::unauthorized)?;
    let principal = auth::principal(
        state,
        &accounts,
        auth::session_token(captured),
        false,
        Instant::now(),
    )?;
    auth::require_mutation(&principal)?;
    let catalog = store::load(&store::catalog_path(&state.data_dir))
        .map_err(|failure| {
            eprintln!("API playback catalog read failed: {failure}");
            history_error()
        })?
        .ok_or_else(unavailable)?;
    let current =
        source_from_catalog(&catalog, &original.reference).map_err(|_| history_error())?;
    if current.reference != original.reference
        || current.path != original.path
        || current.file.size != original.file.size
        || current.file.mtime != original.file.mtime
        || current.mtime_subseconds != original.mtime_subseconds
    {
        return Err(history_error());
    }
    validate_source(original).map_err(|_| history_error())?;
    let mut data = load_history(&state.data_dir, &catalog)?;
    data.record_play(Play {
        owner: principal.owner,
        track: original.reference.clone(),
        at: started_at,
        ms_played,
        completed,
    });
    user::save(&data, &user::user_path(&state.data_dir)).map_err(|failure| {
        eprintln!("API playback history write failed: {failure}");
        history_error()
    })
}

#[derive(Clone)]
struct ListenRecord {
    state: ApiState,
    captured: auth::Principal,
    source: TrackSource,
    started_at: u64,
    ms_played: u64,
    completed: bool,
}

impl ListenRecord {
    fn persist(&self) -> Result<(), ApiError> {
        persist_listen(
            &self.state,
            &self.captured,
            &self.source,
            self.started_at,
            self.ms_played,
            self.completed,
        )
    }
}

async fn record_listen(
    record: ListenRecord,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> (
    Result<(), StreamFailure>,
    Option<tokio::sync::OwnedSemaphorePermit>,
) {
    let mut task = tokio::task::spawn_blocking(move || record.persist());
    tokio::select! {
        outcome = &mut task => match outcome {
            Ok(Ok(())) => (Ok(()), Some(permit)),
            Ok(Err(failure)) => {
                eprintln!("API playback history update failed: {}", failure.message);
                (Err(StreamFailure::HISTORY_FAILED), Some(permit))
            }
            Err(_) => (Err(StreamFailure::HISTORY_FAILED), Some(permit)),
        },
        _ = tokio::time::sleep(HISTORY_WRITE_TIMEOUT) => {
            tokio::spawn(async move {
                let _permit = permit;
                match task.await {
                    Ok(Err(failure)) => {
                        eprintln!(
                            "API playback deferred history update failed: {}",
                            failure.code
                        );
                    }
                    Err(_) => eprintln!("API playback deferred history task stopped"),
                    Ok(Ok(())) => {}
                }
            });
            (Err(StreamFailure::HISTORY_FAILED), None)
        }
    }
}

async fn worker_finished(worker: &mut tokio::task::JoinHandle<()>) -> bool {
    tokio::select! {
        _ = worker => true,
        _ = tokio::time::sleep(WORKER_DRAIN_TIMEOUT) => false,
    }
}

fn retain_worker(
    permit: tokio::sync::OwnedSemaphorePermit,
    worker: tokio::task::JoinHandle<()>,
    record: Option<ListenRecord>,
) {
    tokio::spawn(async move {
        let _permit = permit;
        let _ = worker.await;
        if let Some(record) = record {
            match tokio::task::spawn_blocking(move || record.persist()).await {
                Ok(Err(failure)) => {
                    eprintln!(
                        "API playback deferred history update failed: {}",
                        failure.code
                    );
                }
                Err(_) => eprintln!("API playback deferred history task stopped"),
                Ok(Ok(())) => {}
            }
        }
    });
}

async fn playback_stream(
    mut socket: WebSocket,
    state: ApiState,
    captured: auth::Principal,
    permit: tokio::sync::OwnedSemaphorePermit,
    mut shutdown: broadcast::Receiver<()>,
) {
    let start = match receive_start(&mut socket, &state, &captured, &mut shutdown).await {
        Ok(start) => start,
        Err(Some(failure)) => {
            send_failure(&mut socket, failure).await;
            return;
        }
        Err(None) => return,
    };
    if !authorized(&state, &captured, false).await {
        send_failure(&mut socket, StreamFailure::AUTHENTICATION_EXPIRED).await;
        return;
    }
    let (source, catalog) = match current_source(&state, &start.reference).await {
        Ok(source) => source,
        Err(failure) => {
            send_failure(&mut socket, failure).await;
            return;
        }
    };
    let settings = settings(catalog, state.data_dir.clone(), &start);
    let started_at = clock::now_seconds();
    let cancelled = std::sync::Arc::new(AtomicBool::new(false));
    let (mut receiver, mut worker) = spawn_producer(source.clone(), settings, cancelled.clone());
    let end = drive(&mut socket, &state, &captured, &mut receiver, &mut shutdown).await;
    cancelled.store(true, AtomicOrdering::Release);
    // Closing the bounded receiver wakes a worker blocked on output. A
    // spawned blocking task cannot be forcibly cancelled, so its permit stays
    // reserved until it actually exits.
    drop(receiver);

    let ms_played = end
        .sample_rate
        .map(|sample_rate| frames_to_milliseconds(end.acknowledged_frames, sample_rate))
        .unwrap_or(0);
    let record = (ms_played > 0).then(|| ListenRecord {
        state: state.clone(),
        captured: captured.clone(),
        source: source.clone(),
        started_at,
        ms_played,
        completed: end.completed,
    });
    let needs_record = record.is_some();
    let worker_done = worker_finished(&mut worker).await;
    let (recorded, permit) = if worker_done {
        match record {
            Some(record) => record_listen(record, permit).await,
            None => (Ok(()), Some(permit)),
        }
    } else {
        retain_worker(permit, worker, record);
        (
            if needs_record {
                Err(StreamFailure::HISTORY_FAILED)
            } else {
                Ok(())
            },
            None,
        )
    };
    if end.completed {
        match recorded {
            Ok(()) if ms_played > 0 => {
                if send_json(
                    &mut socket,
                    RecordedFrame {
                        kind: "recorded",
                        ms_played,
                        completed: true,
                    },
                )
                .await
                .is_ok()
                {
                    send_close(&mut socket).await;
                }
            }
            Ok(()) => send_close(&mut socket).await,
            Err(failure) => send_failure(&mut socket, failure).await,
        }
    } else if end.notify {
        send_failure(
            &mut socket,
            end.failure.unwrap_or(StreamFailure::STREAM_FAILED),
        )
        .await;
    }
    // Keep the connection's playback permit until its final protocol message
    // has been attempted. A timed-out worker/history reaper owns it instead.
    drop(permit);
}

#[cfg(test)]
#[path = "playback_api_tests.rs"]
mod tests;
