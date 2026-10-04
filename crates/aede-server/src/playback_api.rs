//! Authenticated, bounded PCM playback for remote clients.
//!
//! This module transports processed PCM only. It deliberately does not own a
//! device or a second audio pipeline. Opt-in controls manage source positioning,
//! queue revisions and private checkpoints; decoding, stereo
//! downmix, normalization, tone controls, rate conversion, and the final
//! output guard come from `aede-core` and `aede-dsp`, just as they do for local
//! playback. The client acknowledges cumulative output frames, so listening
//! history represents audio it consumed rather than audio the server merely
//! wrote to a socket buffer.

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use aede_core::clock;
use aede_core::model::{AudioFile, EntityKind};
use aede_core::playback::gain_plan::ReadyNormalization;
use aede_core::playback::normalization::Mode;
use aede_core::store;
use aede_core::store_lock::StoreLock;
use aede_core::user::{self, EntityRef, Play, UserData};
use aede_dsp::ToneControls;
use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::*;

const MAX_CONTROL_MESSAGE_BYTES: usize = 4 * 1024;
const MAX_QUEUE_TRACKS: usize = 64;
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
        #[serde(default, deserialize_with = "non_null_field")]
        track: Option<String>,
        #[serde(default, deserialize_with = "non_null_field")]
        tracks: Option<Vec<String>>,
        #[serde(default)]
        normalize: Normalize,
        sample_rate: Option<u32>,
        bass: Option<f32>,
        treble: Option<f32>,
        #[serde(default)]
        interactive: bool,
        profile: Option<String>,
    },
    Resume {
        profile: String,
    },
}

fn non_null_field<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
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
    references: Vec<EntityRef>,
    queue: bool,
    normalize: Mode,
    output_rate: Option<u32>,
    tone: ToneControls,
    interactive: bool,
    profile: Option<String>,
    resume: bool,
}

impl Start {
    fn parse(input: &str) -> Result<Self, StreamFailure> {
        let initial = serde_json::from_str(input).map_err(|_| StreamFailure::INVALID_START)?;
        let InitialFrame::Start {
            track,
            tracks,
            normalize,
            sample_rate,
            bass,
            treble,
            interactive,
            profile,
        } = initial
        else {
            let InitialFrame::Resume { profile } = initial else {
                return Err(StreamFailure::INVALID_START);
            };
            return Ok(Self {
                references: Vec::new(),
                queue: true,
                normalize: Mode::Track,
                output_rate: None,
                tone: ToneControls::FLAT,
                interactive: true,
                profile: Some(profile),
                resume: true,
            });
        };
        if profile.is_some() && !interactive {
            return Err(StreamFailure::INVALID_START);
        }
        let (tracks, queue) = match (track, tracks) {
            (Some(track), None) => (vec![track], false),
            (None, Some(tracks)) if !tracks.is_empty() && tracks.len() <= MAX_QUEUE_TRACKS => {
                (tracks, true)
            }
            _ => return Err(StreamFailure::INVALID_START),
        };
        let references = tracks
            .iter()
            .map(|track| {
                EntityRef::parse_token(track)
                    .filter(|reference| {
                        reference.kind == EntityKind::Track && !reference.key.is_empty()
                    })
                    .ok_or(StreamFailure::INVALID_START)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if sample_rate.is_some_and(|rate| !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&rate)) {
            return Err(StreamFailure::INVALID_START);
        }
        let bass = bass.unwrap_or(0.0);
        let treble = treble.unwrap_or(0.0);
        let tone = ToneControls::new(bass, treble).map_err(|_| StreamFailure::INVALID_START)?;
        Ok(Self {
            references,
            queue,
            normalize: normalize.mode(),
            output_rate: sample_rate,
            tone,
            interactive,
            profile,
            resume: false,
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
        // Raw lyrics/tags can be large. Repeated queue references need only
        // source identity and technical properties, never copies of those tags.
        file: AudioFile {
            id: file.id,
            path: file.path.clone(),
            size: file.size,
            mtime: file.mtime,
            properties: file.properties.clone(),
            ..AudioFile::default()
        },
        mtime_subseconds,
    })
}

async fn current_sources(
    state: &ApiState,
    requested: &[EntityRef],
) -> Result<(Vec<TrackSource>, Catalog), StreamFailure> {
    // Track references and selected analyses can each scan a large catalog.
    // The socket's playback permit bounds this work independently of HTTP
    // workers; keep it away from Tokio's network and acknowledgement threads.
    let state = state.clone();
    let requested = requested.to_vec();
    tokio::task::spawn_blocking(move || selected_sources(&state, &requested))
        .await
        .map_err(|_| StreamFailure::STREAM_FAILED)?
}

fn selected_sources(
    state: &ApiState,
    requested: &[EntityRef],
) -> Result<(Vec<TrackSource>, Catalog), StreamFailure> {
    let catalog = state.catalog.blocking_read();
    let catalog = catalog.as_ref().ok_or(StreamFailure::CATALOG_UNAVAILABLE)?;
    let sources = requested
        .iter()
        .map(|reference| source_from_catalog(catalog, reference))
        .collect::<Result<Vec<_>, _>>()?;
    let paths: BTreeSet<_> = sources
        .iter()
        .map(|source| source.file.path.as_str())
        .collect();
    // ReadyNormalization needs catalog analyses only for the selected files.
    // Retaining the full graph here would copy a potentially huge library per
    // active client, so keep just analyses that can describe this exact file.
    let normalization_catalog = Catalog {
        analyses: catalog
            .analyses
            .iter()
            .filter(|analysis| paths.contains(analysis.path.as_str()))
            .cloned()
            .collect(),
        ..Catalog::default()
    };
    Ok((sources, normalization_catalog))
}

/// File identity is checked around opening and decoding. This cannot make a
/// pathname-based decoder immune to every operating-system race, but it
/// refuses relative/traversing paths, links and special files, never accepts a
/// caller-supplied path, and detects a replacement before it can become an
/// acknowledged listen.
pub(super) fn validate_source(source: &TrackSource) -> Result<(), StreamFailure> {
    if !source.path.is_absolute()
        || source
            .path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(StreamFailure::SOURCE_CHANGED);
    }
    let metadata = fs::symlink_metadata(&source.path).map_err(|failure| {
        if failure.kind() == ErrorKind::NotFound {
            StreamFailure::SOURCE_UNAVAILABLE
        } else {
            StreamFailure::SOURCE_CHANGED
        }
    })?;
    let identity = user::PlaybackSource {
        size: source.file.size,
        mtime: source.file.mtime,
        mtime_ns: Some(source.mtime_subseconds),
    };
    if metadata.file_type().is_symlink() || !identity.matches_metadata(&metadata) {
        return Err(StreamFailure::SOURCE_CHANGED);
    }
    Ok(())
}

struct ProducerSettings {
    queue: bool,
    interactive: bool,
    first_position_ms: u64,
    output_rate: Option<u32>,
    tone: ToneControls,
    normalize: Mode,
    normalization_catalog: Catalog,
    data_dir: PathBuf,
}

fn settings(normalization_catalog: Catalog, data_dir: PathBuf, start: &Start) -> ProducerSettings {
    ProducerSettings {
        queue: start.queue,
        interactive: start.interactive,
        first_position_ms: 0,
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
    const INVALID_SEEK: Self = Self {
        code: "invalid_seek",
        message: "the requested source position is outside this track",
    };
    const INVALID_CONTROL: Self = Self {
        code: "invalid_control",
        message: "the interactive control, epoch or queue revision is invalid",
    };
    const STATE_FAILED: Self = Self {
        code: "state_failed",
        message: "the private playback checkpoint could not be saved or restored",
    };
    const STATE_CONFLICT: Self = Self {
        code: "state_conflict",
        message: "another playback session owns this saved profile",
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
    #[serde(skip_serializing_if = "Option::is_none")]
    position_ms: Option<u64>,
    max_unacknowledged_frames: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_frame: Option<u64>,
}

#[derive(Serialize)]
struct TrackFrame {
    #[serde(rename = "type")]
    kind: &'static str,
    index: usize,
    track: String,
    start_frame: u64,
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_ms: Option<u64>,
}

#[derive(Serialize)]
struct TrackEndFrame {
    #[serde(rename = "type")]
    kind: &'static str,
    index: usize,
    track: String,
    end_frame: u64,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    track: Option<String>,
}

enum ProducerEvent {
    Format(FormatFrame),
    Track(TrackFrame),
    TrackEnd(TrackEndFrame),
    Audio { bytes: Vec<u8>, frames: u64 },
    Eof { frames: u64 },
    Error(StreamFailure),
}

enum ProducerStop {
    Cancelled,
    Failed(StreamFailure),
}

#[path = "playback_api_producer.rs"]
mod producer;
use producer::spawn_producer;

#[path = "playback_api_history.rs"]
mod history;
use history::ListeningTimeline;

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
    control: Option<interactive::Control>,
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
            control: None,
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

struct DriveMode<'a> {
    queue: bool,
    interactive: Option<&'a interactive::Epoch>,
}

async fn drive(
    socket: &mut WebSocket,
    state: &ApiState,
    captured: &auth::Principal,
    receiver: &mut mpsc::Receiver<ProducerEvent>,
    shutdown: &mut broadcast::Receiver<()>,
    sources: &[TrackSource],
    mode: DriveMode<'_>,
    timeline: &mut ListeningTimeline,
) -> DriveEnd {
    let DriveMode { queue, interactive } = mode;
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
                    control: None,
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
                        control: None,
                    };
                }
            }
            _ = shutdown.recv() => return DriveEnd {
                acknowledged_frames: 0,
                sample_rate: None,
                completed: false,
                failure: Some(StreamFailure::SERVER_SHUTDOWN),
                notify: true,
                control: None,
            },
            incoming = wait_for_format(socket) => match incoming {
                Ok(true) => {}
                Ok(false) => return DriveEnd {
                    acknowledged_frames: 0,
                    sample_rate: None,
                    completed: false,
                    failure: None,
                    notify: false,
                    control: None,
                },
                Err(failure) => return DriveEnd {
                    acknowledged_frames: 0,
                    sample_rate: None,
                    completed: false,
                    failure: Some(failure),
                    notify: true,
                    control: None,
                },
            },
            _ = &mut source_deadline => return DriveEnd {
                acknowledged_frames: 0,
                sample_rate: None,
                completed: false,
                failure: Some(StreamFailure::SOURCE_UNAVAILABLE),
                notify: true,
                control: None,
            },
        }
    };
    let mut format = match initial {
        ProducerEvent::Format(format) => format,
        ProducerEvent::Error(failure) => {
            return DriveEnd {
                acknowledged_frames: 0,
                sample_rate: None,
                completed: false,
                failure: Some(failure),
                notify: true,
                control: None,
            };
        }
        ProducerEvent::Audio { .. }
        | ProducerEvent::Eof { .. }
        | ProducerEvent::Track(_)
        | ProducerEvent::TrackEnd(_) => {
            return DriveEnd {
                acknowledged_frames: 0,
                sample_rate: None,
                completed: false,
                failure: Some(StreamFailure::STREAM_FAILED),
                notify: true,
                control: None,
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
    if interactive::send_stream_json(socket, &format, interactive)
        .await
        .is_err()
    {
        return DriveEnd::stopped(&acknowledgements, Some(format.sample_rate), None, false);
    }
    let mut pending = None;
    let mut eof = None;
    let mut producer_progress = Instant::now();
    let mut checkpoint_at = Instant::now();
    loop {
        if interactive.is_some_and(|epoch| epoch.checkpoint_failed()) {
            return DriveEnd::stopped(
                &acknowledgements,
                Some(format.sample_rate),
                Some(StreamFailure::STATE_FAILED),
                true,
            );
        }
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
                    if interactive::send_stream_json(
                        socket,
                        &EofFrame {
                            kind: "eof",
                            frames,
                        },
                        interactive,
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
                ProducerEvent::Format(next) => {
                    if !queue
                        || next.start_frame != Some(acknowledgements.sent)
                        || next.sample_rate != format.sample_rate
                    {
                        return DriveEnd::stopped(
                            &acknowledgements,
                            Some(format.sample_rate),
                            Some(StreamFailure::STREAM_FAILED),
                            true,
                        );
                    }
                    if acknowledgements.sent != acknowledgements.consumed {
                        // Channel changes require the client to drain its old
                        // device format before any differently laid out bytes.
                        pending = Some(ProducerEvent::Format(next));
                    } else {
                        if !keep_authorized(state, captured, &mut last_authentication, false).await
                        {
                            return DriveEnd::stopped(
                                &acknowledgements,
                                Some(format.sample_rate),
                                Some(StreamFailure::AUTHENTICATION_EXPIRED),
                                true,
                            );
                        }
                        if interactive::send_stream_json(socket, &next, interactive)
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
                        acknowledgements.max_pending = next.max_unacknowledged_frames;
                        format = next;
                        producer_progress = Instant::now();
                        continue;
                    }
                }
                ProducerEvent::Track(track) => {
                    if let Err(failure) =
                        timeline.begin(&track, acknowledgements.sent, format.sample_rate, sources)
                    {
                        return DriveEnd::stopped(
                            &acknowledgements,
                            Some(format.sample_rate),
                            Some(failure),
                            true,
                        );
                    }
                    if queue
                        && interactive::send_stream_json(socket, &track, interactive)
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
                    continue;
                }
                ProducerEvent::TrackEnd(track) => {
                    if let Err(failure) = timeline.end(&track, acknowledgements.sent) {
                        return DriveEnd::stopped(
                            &acknowledgements,
                            Some(format.sample_rate),
                            Some(failure),
                            true,
                        );
                    }
                    if queue
                        && interactive::send_stream_json(socket, &track, interactive)
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
                    continue;
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
                control: None,
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
            incoming = interactive::receive(socket, &mut acknowledgements, interactive) => match incoming {
                Ok(interactive::Incoming::Control(control)) => {
                    if !authorized(state, captured, true).await {
                        return DriveEnd::stopped(&acknowledgements, Some(format.sample_rate),
                            Some(StreamFailure::AUTHENTICATION_EXPIRED), true);
                    }
                    let mut end = DriveEnd::stopped(&acknowledgements, Some(format.sample_rate), None, false);
                    end.control = Some(control);
                    return end;
                }
                Ok(interactive::Incoming::Continue) => {
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
                        if checkpoint_at.elapsed() >= Duration::from_secs(5) {
                            if let Some(epoch) = interactive {
                                epoch.checkpoint(timeline, acknowledgements.consumed);
                            }
                            checkpoint_at = Instant::now();
                        }
                        producer_progress = Instant::now();
                    }
                }
                Ok(interactive::Incoming::Closed) => return DriveEnd::stopped(
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

#[derive(Default)]
struct HistoryOutcome {
    saved: Vec<usize>,
    failed: bool,
}

#[derive(Clone)]
struct ListenRecord {
    state: ApiState,
    captured: auth::Principal,
    listens: Vec<history::Listened>,
}

impl ListenRecord {
    fn persist(&self) -> Result<HistoryOutcome, ApiError> {
        let _guard = lock_for_history(&self.state.data_dir)?;
        // Revocation wins over every pending occurrence in the batch. Source
        // invalidation is local to its occurrence and cannot erase another
        // acknowledged listen.
        let accounts = auth::load_accounts(&self.state)?.ok_or_else(auth::unauthorized)?;
        let principal = auth::principal(
            &self.state,
            &accounts,
            auth::session_token(&self.captured),
            false,
            Instant::now(),
        )?;
        auth::require_mutation(&principal)?;
        let catalog = store::load(&store::catalog_path(&self.state.data_dir))
            .map_err(|failure| {
                eprintln!("API playback catalog read failed: {failure}");
                history_error()
            })?
            .ok_or_else(unavailable)?;
        let mut data = load_history(&self.state.data_dir, &catalog)?;
        let mut outcome = HistoryOutcome::default();
        for listen in &self.listens {
            let original = &listen.source;
            let valid = source_from_catalog(&catalog, &original.reference).is_ok_and(|current| {
                current.reference == original.reference
                    && current.path == original.path
                    && current.file.size == original.file.size
                    && current.file.mtime == original.file.mtime
                    && current.mtime_subseconds == original.mtime_subseconds
            }) && validate_source(original).is_ok();
            if !valid {
                outcome.failed = true;
                continue;
            }
            data.record_play(Play {
                owner: principal.owner.clone(),
                track: original.reference.clone(),
                at: listen.started_at,
                ms_played: listen.ms_played,
                completed: listen.completed,
            });
            outcome.saved.push(listen.index);
        }
        if !outcome.saved.is_empty() {
            user::save(&data, &user::user_path(&self.state.data_dir)).map_err(|failure| {
                eprintln!("API playback history write failed: {failure}");
                history_error()
            })?;
        }
        Ok(outcome)
    }
}

async fn record_listen(
    record: ListenRecord,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> (
    Result<HistoryOutcome, StreamFailure>,
    Option<tokio::sync::OwnedSemaphorePermit>,
) {
    let mut task = tokio::task::spawn_blocking(move || record.persist());
    tokio::select! {
        outcome = &mut task => match outcome {
            Ok(Ok(outcome)) => (Ok(outcome), Some(permit)),
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
                    Ok(Err(failure)) => eprintln!("API playback deferred history update failed: {}", failure.code),
                    Err(_) => eprintln!("API playback deferred history task stopped"),
                    Ok(Ok(outcome)) if outcome.failed => eprintln!("API playback deferred history rejected changed sources"),
                    Ok(Ok(_)) => {}
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
                Ok(Ok(outcome)) if outcome.failed => {
                    eprintln!("API playback deferred history rejected changed sources")
                }
                Ok(Ok(_)) => {}
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
    if start.interactive {
        interactive::run(socket, state, captured, permit, shutdown, start).await;
        return;
    }
    let (sources, catalog) = match current_sources(&state, &start.references).await {
        Ok(sources) => sources,
        Err(failure) => {
            send_failure(&mut socket, failure).await;
            return;
        }
    };
    let settings = settings(catalog, state.data_dir.clone(), &start);
    let cancelled = std::sync::Arc::new(AtomicBool::new(false));
    let (mut receiver, mut worker) = spawn_producer(sources.clone(), settings, cancelled.clone());
    let mut timeline = ListeningTimeline::default();
    let end = drive(
        &mut socket,
        &state,
        &captured,
        &mut receiver,
        &mut shutdown,
        &sources,
        DriveMode {
            queue: start.queue,
            interactive: None,
        },
        &mut timeline,
    )
    .await;
    cancelled.store(true, AtomicOrdering::Release);
    // Closing the bounded receiver wakes a worker blocked on output. A
    // spawned blocking task cannot be forcibly cancelled, so its permit stays
    // reserved until it actually exits.
    drop(receiver);
    let mut listens = if end.sample_rate.is_some() {
        timeline.listens(end.acknowledged_frames, &sources)
    } else {
        Vec::new()
    };
    if !start.queue && !end.completed {
        // The original singleton contract completes only after the socket's
        // EOF handshake; additive queue markers must not change that rule.
        for listen in &mut listens {
            listen.completed = false;
        }
    }
    let record = (!listens.is_empty()).then(|| ListenRecord {
        state: state.clone(),
        captured: captured.clone(),
        listens: listens.clone(),
    });
    let needs_record = record.is_some();
    let worker_done = worker_finished(&mut worker).await;
    let (recorded, permit) = if worker_done {
        match record {
            Some(record) => record_listen(record, permit).await,
            None => (Ok(HistoryOutcome::default()), Some(permit)),
        }
    } else {
        retain_worker(permit, worker, record);
        (
            if needs_record {
                Err(StreamFailure::HISTORY_FAILED)
            } else {
                Ok(HistoryOutcome::default())
            },
            None,
        )
    };
    if end.completed {
        match recorded {
            Ok(outcome) => {
                for listen in listens
                    .iter()
                    .filter(|listen| outcome.saved.contains(&listen.index))
                {
                    if send_json(
                        &mut socket,
                        RecordedFrame {
                            kind: "recorded",
                            ms_played: listen.ms_played,
                            completed: listen.completed,
                            index: start.queue.then_some(listen.index),
                            track: start.queue.then(|| listen.source.reference.to_token()),
                        },
                    )
                    .await
                    .is_err()
                    {
                        return;
                    }
                }
                if outcome.failed {
                    send_failure(&mut socket, StreamFailure::HISTORY_FAILED).await;
                } else {
                    send_close(&mut socket).await;
                }
            }
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

#[path = "playback_interactive.rs"]
mod interactive;

#[cfg(test)]
#[path = "playback_api_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "playback_queue_tests.rs"]
mod queue_tests;

#[cfg(test)]
#[path = "playback_md5_tests.rs"]
mod md5_tests;
