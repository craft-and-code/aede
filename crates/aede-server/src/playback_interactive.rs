//! Opt-in native transport controls and private, acknowledged resume checkpoints.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;
use aede_core::user::{PlaybackEntry, PlaybackSettings, PlaybackSource, PlaybackState};

#[path = "playback_interactive_store.rs"]
mod checkpoint;
use checkpoint::{ProfileLease, Snapshot, Writer, persist_snapshot, read_profile};

const MAX_ACTIONS: usize = 128;
const MAX_LISTENS: usize = 256;
const MAX_POSITION_MS: u64 = 24 * 60 * 60 * 1_000;
const MAX_EXACT_INTEGER: u64 = (1u64 << 53) - 1;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Frame {
    Ack {
        epoch: u64,
        frames: u64,
    },
    Seek {
        epoch: u64,
        frames: u64,
        position_ms: u64,
    },
    EditQueue {
        epoch: u64,
        frames: u64,
        revision: u64,
        items: Vec<Item>,
    },
    Stop {
        epoch: u64,
        frames: u64,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Item {
    occurrence: Option<u64>,
    track: Option<String>,
}

pub(super) enum Control {
    Seek(u64),
    Edit { revision: u64, items: Vec<Item> },
    Stop,
}

pub(super) enum Incoming {
    Continue,
    Closed,
    Control(Control),
}

pub(super) struct Epoch {
    number: u64,
    entries: Vec<PlaybackEntry>,
    base: PlaybackState,
    sender: Option<mpsc::Sender<Snapshot>>,
    failed: Arc<AtomicBool>,
}

impl Epoch {
    pub(super) fn checkpoint_failed(&self) -> bool {
        self.failed.load(AtomicOrdering::Acquire)
    }

    fn snapshot(&self, timeline: &ListeningTimeline, consumed: u64) -> PlaybackState {
        let mut snapshot = self.base.clone();
        if let Some((index, position, at_end)) = timeline.cursor(consumed) {
            let next = index + usize::from(at_end);
            snapshot.current_occurrence = self.entries.get(next).map(|entry| entry.occurrence);
            snapshot.position_ms = if at_end { 0 } else { position };
        }
        snapshot.settings.sample_rate = timeline.output_rate().or(snapshot.settings.sample_rate);
        snapshot.updated_at = clock::now_seconds();
        snapshot
    }

    pub(super) fn checkpoint(&self, timeline: &ListeningTimeline, consumed: u64) {
        if let Some(sender) = &self.sender {
            // One pending periodic write is enough. A final/control write is
            // awaited separately and always follows any queued periodic write.
            let _ = sender.try_send(Snapshot {
                state: self.snapshot(timeline, consumed),
                expected_revision: self.base.revision,
                reply: None,
            });
        }
    }
}

pub(super) async fn send_stream_json(
    socket: &mut WebSocket,
    value: &impl Serialize,
    epoch: Option<&Epoch>,
) -> Result<(), ()> {
    let Some(epoch) = epoch else {
        return send_json(socket, value).await;
    };
    let mut value = serde_json::to_value(value).map_err(|_| ())?;
    let object = value.as_object_mut().ok_or(())?;
    object.insert("epoch".into(), epoch.number.into());
    object.insert("revision".into(), epoch.base.revision.into());
    if let Some(index) = object.get("index").and_then(serde_json::Value::as_u64) {
        let entry = usize::try_from(index)
            .ok()
            .and_then(|index| epoch.entries.get(index))
            .ok_or(())?;
        object.insert("occurrence".into(), entry.occurrence.into());
    }
    send_json(socket, value).await
}

pub(super) async fn receive(
    socket: &mut WebSocket,
    acknowledgements: &mut Acknowledgements,
    epoch: Option<&Epoch>,
) -> Result<Incoming, StreamFailure> {
    let Some(epoch) = epoch else {
        return receive_ack(socket, acknowledgements).await.map(|open| {
            if open {
                Incoming::Continue
            } else {
                Incoming::Closed
            }
        });
    };
    let Some(message) = socket.recv().await else {
        return Ok(Incoming::Closed);
    };
    match message.map_err(|_| StreamFailure::STREAM_FAILED)? {
        Message::Text(input) => {
            let frame: Frame =
                serde_json::from_str(&input).map_err(|_| StreamFailure::INVALID_CONTROL)?;
            let (number, frames, control) = match frame {
                Frame::Ack { epoch, frames } => (epoch, frames, None),
                Frame::Seek {
                    epoch,
                    frames,
                    position_ms,
                } if position_ms <= MAX_POSITION_MS => {
                    (epoch, frames, Some(Control::Seek(position_ms)))
                }
                Frame::Seek { .. } => return Err(StreamFailure::INVALID_SEEK),
                Frame::EditQueue {
                    epoch,
                    frames,
                    revision,
                    items,
                } if items.len() <= MAX_QUEUE_TRACKS => {
                    (epoch, frames, Some(Control::Edit { revision, items }))
                }
                Frame::EditQueue { .. } => return Err(StreamFailure::INVALID_CONTROL),
                Frame::Stop { epoch, frames } => (epoch, frames, Some(Control::Stop)),
            };
            if number != epoch.number {
                return Err(StreamFailure::INVALID_CONTROL);
            }
            acknowledgements.acknowledge(frames, Instant::now())?;
            Ok(control.map_or(Incoming::Continue, Incoming::Control))
        }
        Message::Ping(payload) => {
            events::send_message(socket, Message::Pong(payload))
                .await
                .map_err(|_| StreamFailure::STREAM_FAILED)?;
            Ok(Incoming::Continue)
        }
        Message::Pong(_) => Ok(Incoming::Continue),
        Message::Close(_) => Ok(Incoming::Closed),
        Message::Binary(_) => Err(StreamFailure::INVALID_CONTROL),
    }
}

fn entry(source: &TrackSource, occurrence: u64) -> PlaybackEntry {
    PlaybackEntry {
        occurrence,
        track: source.reference.clone(),
        source: PlaybackSource {
            size: source.file.size,
            mtime: source.file.mtime,
            mtime_ns: Some(source.mtime_subseconds),
        },
    }
}

fn sources_match(entries: &[PlaybackEntry], sources: &[TrackSource]) -> bool {
    entries.len() == sources.len()
        && entries.iter().zip(sources).all(|(entry, source)| {
            entry.track == source.reference
                && entry.source.size == source.file.size
                && entry.source.mtime == source.file.mtime
                && entry.source.mtime_ns == Some(source.mtime_subseconds)
        })
}

async fn initial_state(
    state: &ApiState,
    captured: &auth::Principal,
    start: &Start,
) -> Result<PlaybackState, StreamFailure> {
    if start
        .profile
        .as_deref()
        .is_some_and(|profile| !user::valid_playback_profile(profile))
    {
        return Err(StreamFailure::INVALID_START);
    }
    let previous = if let Some(profile) = &start.profile {
        let state = state.clone();
        let captured = captured.clone();
        let profile = profile.clone();
        tokio::task::spawn_blocking(move || read_profile(&state, &captured, &profile))
            .await
            .map_err(|_| StreamFailure::STATE_FAILED)??
    } else {
        None
    };
    let references = if start.resume {
        previous
            .as_ref()
            .ok_or(StreamFailure::STATE_FAILED)?
            .entries
            .iter()
            .map(|entry| entry.track.clone())
            .collect()
    } else {
        start.references.clone()
    };
    let (sources, _) = current_sources(state, &references).await?;
    let sources = tokio::task::spawn_blocking(move || {
        sources.iter().try_for_each(validate_source)?;
        Ok::<_, StreamFailure>(sources)
    })
    .await
    .map_err(|_| StreamFailure::STATE_FAILED)??;
    let mut snapshot = if start.resume {
        let previous = previous.clone().ok_or(StreamFailure::STATE_FAILED)?;
        if !sources_match(&previous.entries, &sources) {
            return Err(StreamFailure::SOURCE_CHANGED);
        }
        previous
    } else {
        PlaybackState {
            owner: captured.owner.clone(),
            profile: start.profile.clone().unwrap_or_else(|| "volatile".into()),
            session_id: String::new(),
            revision: 1,
            entries: sources
                .iter()
                .enumerate()
                .map(|(index, source)| entry(source, index as u64 + 1))
                .collect(),
            current_occurrence: Some(1),
            position_ms: 0,
            settings: PlaybackSettings {
                normalize: start.normalize,
                sample_rate: start.output_rate,
                bass_db: start.tone.bass_db(),
                treble_db: start.tone.treble_db(),
            },
            updated_at: clock::now_seconds(),
        }
    };
    if snapshot.position_ms > MAX_POSITION_MS {
        return Err(StreamFailure::INVALID_SEEK);
    }
    snapshot.revision = previous
        .as_ref()
        .map_or(Some(1), |state| state.revision.checked_add(1))
        .filter(|revision| *revision <= MAX_EXACT_INTEGER)
        .ok_or(StreamFailure::STATE_FAILED)?;
    snapshot.session_id =
        aede_core::accounts::random_token().map_err(|_| StreamFailure::STATE_FAILED)?;
    snapshot.updated_at = clock::now_seconds();
    if start.profile.is_some() {
        let state = state.clone();
        let captured = captured.clone();
        let initial = snapshot.clone();
        tokio::task::spawn_blocking(move || {
            persist_snapshot(
                &state,
                &captured,
                initial,
                previous
                    .as_ref()
                    .map(|state| (state.session_id.as_str(), state.revision)),
            )
        })
        .await
        .map_err(|_| StreamFailure::STATE_FAILED)??;
    }
    Ok(snapshot)
}

async fn reset_ack(
    socket: &mut WebSocket,
    state: &ApiState,
    captured: &auth::Principal,
    shutdown: &mut broadcast::Receiver<()>,
    epoch: u64,
) -> Result<(), StreamFailure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ResetAck {
        #[serde(rename = "type")]
        kind: String,
        epoch: u64,
    }
    let deadline = tokio::time::sleep(START_TIMEOUT);
    tokio::pin!(deadline);
    let mut authorization = tokio::time::interval(AUTH_RECHECK_INTERVAL);
    loop {
        tokio::select! {
            _ = &mut deadline => return Err(StreamFailure::ACK_TIMEOUT),
            _ = shutdown.recv() => return Err(StreamFailure::SERVER_SHUTDOWN),
            _ = authorization.tick() => {
                if !authorized(state,captured,false).await { return Err(StreamFailure::AUTHENTICATION_EXPIRED) }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(input))) => {
                    let ack: ResetAck = serde_json::from_str(&input).map_err(|_| StreamFailure::INVALID_CONTROL)?;
                    if ack.kind != "reset_ack" || ack.epoch != epoch { return Err(StreamFailure::INVALID_CONTROL) }
                    if !authorized(state,captured,true).await { return Err(StreamFailure::AUTHENTICATION_EXPIRED) }
                    return Ok(());
                }
                Some(Ok(Message::Ping(payload))) => { events::send_message(socket,Message::Pong(payload)).await.map_err(|_|StreamFailure::STREAM_FAILED)?; }
                Some(Ok(Message::Pong(_))) => {}
                _ => return Err(StreamFailure::STREAM_FAILED),
            }
        }
    }
}

async fn announce(
    socket: &mut WebSocket,
    snapshot: &PlaybackState,
    epoch: u64,
    reset: bool,
) -> Result<(), StreamFailure> {
    let items: Vec<_> = snapshot.entries.iter().map(|entry| serde_json::json!({"occurrence":entry.occurrence,"track":entry.track.to_token()})).collect();
    send_json(socket, serde_json::json!({
        "type":if reset {"reset"} else {"queue"}, "epoch":epoch,"revision":snapshot.revision,
        "items":items,"current_occurrence":snapshot.current_occurrence,"position_ms":snapshot.position_ms
    })).await.map_err(|_| StreamFailure::STREAM_FAILED)
}

fn selected_index(snapshot: &PlaybackState) -> Option<usize> {
    snapshot.current_occurrence.and_then(|id| {
        snapshot
            .entries
            .iter()
            .position(|entry| entry.occurrence == id)
    })
}

async fn edit(
    state: &ApiState,
    snapshot: &mut PlaybackState,
    revision: u64,
    items: Vec<Item>,
    next_id: &mut u64,
) -> Result<(), StreamFailure> {
    if revision != snapshot.revision {
        return Err(StreamFailure::INVALID_CONTROL);
    }
    let prefix = selected_index(snapshot).map_or(snapshot.entries.len(), |index| index + 1);
    if prefix + items.len() > MAX_QUEUE_TRACKS {
        return Err(StreamFailure::INVALID_CONTROL);
    }
    let references = items
        .iter()
        .filter_map(|item| item.track.as_deref())
        .map(|track| {
            EntityRef::parse_token(track)
                .filter(|reference| reference.kind == EntityKind::Track)
                .ok_or(StreamFailure::INVALID_CONTROL)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (new_sources, _) = current_sources(state, &references).await?;
    let mut new_sources = new_sources.into_iter();
    let mut seen = BTreeSet::new();
    let mut future = Vec::new();
    let mut next = *next_id;
    for item in items {
        match (item.occurrence, item.track) {
            (Some(id), None) if seen.insert(id) => {
                let entry = snapshot.entries[prefix..]
                    .iter()
                    .find(|entry| entry.occurrence == id)
                    .ok_or(StreamFailure::INVALID_CONTROL)?;
                future.push(entry.clone());
            }
            (None, Some(_)) => {
                let source = new_sources.next().ok_or(StreamFailure::TRACK_NOT_FOUND)?;
                next = next
                    .checked_add(1)
                    .filter(|id| *id <= MAX_EXACT_INTEGER)
                    .ok_or(StreamFailure::INVALID_CONTROL)?;
                future.push(entry(&source, next));
            }
            _ => return Err(StreamFailure::INVALID_CONTROL),
        }
    }
    if snapshot.current_occurrence.is_none() {
        snapshot.current_occurrence = future.first().map(|entry| entry.occurrence);
        snapshot.position_ms = 0;
    }
    snapshot.entries.truncate(prefix);
    snapshot.entries.extend(future);
    *next_id = next;
    Ok(())
}

struct Visit {
    order: usize,
    frames: u64,
    sample_rate: u32,
    occurrence: u64,
    listen: history::Listened,
    moved: bool,
}

fn accumulate(
    visits: &mut BTreeMap<u64, Visit>,
    epoch: &Epoch,
    timeline: &ListeningTimeline,
    consumed: u64,
    sources: &[TrackSource],
    moved: &BTreeSet<u64>,
) -> Result<(), StreamFailure> {
    for (listen, frames, sample_rate) in timeline.acknowledged(consumed, sources) {
        let id = epoch
            .entries
            .get(listen.index)
            .ok_or(StreamFailure::STREAM_FAILED)?
            .occurrence;
        if let Some(visit) = visits.get_mut(&id) {
            if visit.sample_rate != sample_rate {
                return Err(StreamFailure::STREAM_FAILED);
            }
            visit.frames = visit
                .frames
                .checked_add(frames)
                .ok_or(StreamFailure::STREAM_FAILED)?;
            visit.listen.ms_played = frames_to_milliseconds(visit.frames, sample_rate);
            visit.moved |= moved.contains(&id);
            // An edit restart continues one visit from its consumed cursor.
            visit.listen.completed = !visit.moved && listen.completed;
        } else {
            if visits.len() >= MAX_LISTENS {
                return Err(StreamFailure::INVALID_CONTROL);
            }
            let mut listen = listen;
            let was_moved = moved.contains(&id) || epoch.base.position_ms > 0 && listen.index == 0;
            listen.completed &= !was_moved;
            visits.insert(
                id,
                Visit {
                    order: visits.len(),
                    frames,
                    sample_rate,
                    occurrence: id,
                    listen,
                    moved: was_moved,
                },
            );
        }
    }
    Ok(())
}

pub(super) async fn run(
    mut socket: WebSocket,
    state: ApiState,
    captured: auth::Principal,
    permit: tokio::sync::OwnedSemaphorePermit,
    mut shutdown: broadcast::Receiver<()>,
    mut start: Start,
) {
    start.queue = true;
    let lease = match start.profile.as_deref() {
        Some(profile) => match ProfileLease::acquire(&state, &captured.owner, profile) {
            Ok(lease) => Some(lease),
            Err(failure) => {
                send_failure(&mut socket, failure).await;
                return;
            }
        },
        None => None,
    };
    let mut snapshot = match initial_state(&state, &captured, &start).await {
        Ok(initial) => initial,
        Err(failure) => {
            send_failure(&mut socket, failure).await;
            return;
        }
    };
    let writer = start
        .profile
        .as_ref()
        .map(|_| Writer::spawn(state.clone(), captured.clone()));
    let mut number = 0;
    let mut next_id = snapshot
        .entries
        .iter()
        .map(|entry| entry.occurrence)
        .max()
        .unwrap_or(0);
    let mut visits = BTreeMap::new();
    let mut moved = BTreeSet::new();
    let mut failure = announce(&mut socket, &snapshot, number, false).await.err();
    let mut final_worker = None;
    let mut completed = false;
    let mut actions = 0;
    let mut seek_backup: Option<PlaybackState> = None;
    while failure.is_none() {
        let Some(index) = selected_index(&snapshot) else {
            completed = true;
            break;
        };
        let entries = snapshot.entries[index..].to_vec();
        let references: Vec<_> = entries.iter().map(|entry| entry.track.clone()).collect();
        let (sources, catalog) = match current_sources(&state, &references).await {
            Ok(value) => value,
            Err(error) => {
                failure = Some(error);
                break;
            }
        };
        if !sources_match(&entries, &sources) {
            failure = Some(StreamFailure::SOURCE_CHANGED);
            break;
        }
        start.normalize = snapshot.settings.normalize;
        start.output_rate = snapshot.settings.sample_rate;
        start.tone = match ToneControls::new(snapshot.settings.bass_db, snapshot.settings.treble_db)
        {
            Ok(tone) => tone,
            Err(_) => {
                failure = Some(StreamFailure::STATE_FAILED);
                break;
            }
        };
        let mut settings = settings(catalog, state.data_dir.clone(), &start);
        settings.first_position_ms = snapshot.position_ms;
        let epoch = Epoch {
            number,
            entries,
            base: snapshot.clone(),
            sender: writer.as_ref().map(|writer| writer.sender.clone()),
            failed: writer.as_ref().map_or_else(
                || Arc::new(AtomicBool::new(false)),
                |writer| writer.failed.clone(),
            ),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let (mut receiver, mut worker) =
            spawn_producer(sources.clone(), settings, cancelled.clone());
        let mut timeline = ListeningTimeline::default();
        let end = drive(
            &mut socket,
            &state,
            &captured,
            &mut receiver,
            &mut shutdown,
            &sources,
            DriveMode {
                queue: true,
                interactive: Some(&epoch),
            },
            &mut timeline,
        )
        .await;
        cancelled.store(true, AtomicOrdering::Release);
        drop(receiver);
        let worker_done = worker_finished(&mut worker).await;
        if let Err(error) = accumulate(
            &mut visits,
            &epoch,
            &timeline,
            end.acknowledged_frames,
            &sources,
            &moved,
        ) {
            failure = Some(error)
        }
        snapshot = if end.sample_rate.is_none() {
            seek_backup
                .take()
                .unwrap_or_else(|| epoch.snapshot(&timeline, end.acknowledged_frames))
        } else {
            seek_backup = None;
            epoch.snapshot(&timeline, end.acknowledged_frames)
        };
        snapshot.settings.sample_rate = end.sample_rate.or(snapshot.settings.sample_rate);
        if end.completed {
            snapshot.current_occurrence = None;
            snapshot.position_ms = 0;
            completed = true
        }
        if !worker_done {
            final_worker = Some(worker);
            failure = Some(StreamFailure::STREAM_FAILED);
            break;
        }
        if failure.is_some() {
            break;
        }
        if let Some(control) = end.control {
            actions += 1;
            if actions > MAX_ACTIONS {
                failure = Some(StreamFailure::INVALID_CONTROL);
                break;
            }
            let previous_revision = snapshot.revision;
            match control {
                Control::Stop => break,
                Control::Seek(position) => {
                    let Some(id) = snapshot.current_occurrence else {
                        failure = Some(StreamFailure::INVALID_SEEK);
                        break;
                    };
                    moved.insert(id);
                    if let Some(visit) = visits.get_mut(&id) {
                        visit.moved = true;
                        visit.listen.completed = false
                    }
                    seek_backup = Some(snapshot.clone());
                    snapshot.position_ms = position;
                }
                Control::Edit { revision, items } => {
                    if let Err(error) =
                        edit(&state, &mut snapshot, revision, items, &mut next_id).await
                    {
                        failure = Some(error);
                        break;
                    }
                }
            }
            snapshot.revision = match snapshot
                .revision
                .checked_add(1)
                .filter(|revision| *revision <= MAX_EXACT_INTEGER)
            {
                Some(revision) => revision,
                None => {
                    failure = Some(StreamFailure::INVALID_CONTROL);
                    break;
                }
            };
            if let Some(writer) = &writer {
                let mut published = seek_backup.clone().unwrap_or_else(|| snapshot.clone());
                published.revision = snapshot.revision;
                if let Some(backup) = &mut seek_backup {
                    backup.revision = snapshot.revision;
                }
                if let Err(error) = writer.save(published, previous_revision).await {
                    failure = Some(error);
                    break;
                }
            }
            number += 1;
            if let Err(error) = announce(&mut socket, &snapshot, number, true).await {
                failure = Some(error);
                break;
            }
            if let Err(error) =
                reset_ack(&mut socket, &state, &captured, &mut shutdown, number).await
            {
                failure = Some(error);
                break;
            }
        } else {
            failure = end.failure;
            break;
        }
    }
    if let Some(confirmed) = seek_backup.take() {
        snapshot = confirmed;
    }
    // Saving a resume point is independent of listening history and never
    // fills in frames that the client did not acknowledge.
    if let Some(writer) = &writer
        && let Err(error) = writer.save(snapshot.clone(), snapshot.revision).await
        && failure.is_none()
    {
        failure = Some(error);
    }
    let mut ordered_visits: Vec<_> = visits
        .values()
        .filter(|visit| visit.listen.ms_played > 0)
        .collect();
    ordered_visits.sort_by_key(|visit| visit.order);
    let listens: Vec<_> = ordered_visits
        .iter()
        .enumerate()
        .map(|(index, visit)| {
            let mut listen = visit.listen.clone();
            listen.index = index;
            listen
        })
        .collect();
    let record = (!listens.is_empty()).then(|| ListenRecord {
        state: state.clone(),
        captured: captured.clone(),
        listens: listens.clone(),
    });
    let mut writer_worker = writer.map(|writer| {
        drop(writer.sender);
        writer.worker
    });
    let writer_done = if let Some(worker) = &mut writer_worker {
        worker_finished(worker).await
    } else {
        true
    };
    let (recorded, permit) = if final_worker.is_some() || !writer_done {
        let deferred_worker = final_worker;
        let deferred_writer = writer_worker;
        tokio::spawn(async move {
            let _permit = permit;
            let _lease = lease;
            if let Some(worker) = deferred_worker {
                let _ = worker.await;
            }
            if let Some(worker) = deferred_writer {
                let _ = worker.await;
            }
            if let Some(record) = record {
                let _ = tokio::task::spawn_blocking(move || record.persist()).await;
            }
        });
        (Err(StreamFailure::HISTORY_FAILED), None)
    } else {
        drop(lease);
        match record {
            Some(record) => record_listen(record, permit).await,
            None => (Ok(HistoryOutcome::default()), Some(permit)),
        }
    };
    match recorded {
        Ok(outcome) => {
            for (_, visit) in ordered_visits
                .iter()
                .enumerate()
                .filter(|(index, _)| outcome.saved.contains(index))
            {
                if send_json(&mut socket,serde_json::json!({"type":"recorded","occurrence":visit.occurrence,"track":visit.listen.source.reference.to_token(),"ms_played":visit.listen.ms_played,"completed":visit.listen.completed})).await.is_err(){break}
            }
            if outcome.failed && failure.is_none() {
                failure = Some(StreamFailure::HISTORY_FAILED)
            }
        }
        Err(error) => {
            if failure.is_none() {
                failure = Some(error)
            }
        }
    }
    if let Some(failure) = failure {
        send_failure(&mut socket, failure).await;
    } else {
        let _=send_json(&mut socket,serde_json::json!({"type":"saved","profile":start.profile,"revision":snapshot.revision,"completed":completed})).await;
        send_close(&mut socket).await;
    }
    drop(permit);
}

#[cfg(test)]
#[path = "playback_interactive_tests.rs"]
mod tests;
