//! Terminal playback through the decoder, DSP and local audio output.

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use aede_core::model::{Catalog, Id, TitleMatch};
use aede_core::playback::Repeat;
use aede_core::playback::gain_plan::{self, GainPlan, ReadyNormalization};
use aede_core::playback::normalization::Mode as NormalizationMode;
use aede_core::playback::stream::PcmTrack;
use aede_core::store;
use aede_core::store_lock::StoreLock;
use aede_core::user::{self, EntityRef, LOCAL_USER, Play};
use aede_core::{clock, model::EntityKind, query, tags};
use aede_dsp::{OutputMeter, OutputMeterError, ToneControls, gain_with_headroom_db};

use super::{Res, data_dir};
use crate::args::{Args, PlaybackShuffle};

#[path = "play_visualizer.rs"]
mod visualizer;
use visualizer::TerminalVisualizer;

#[path = "play_controls.rs"]
mod controls;
use controls::{Action, Controls};

#[path = "play_output.rs"]
mod output;
use output::LocalOutput;

#[path = "play_session.rs"]
mod session;

#[path = "play_order.rs"]
mod order;
use order::PlaybackOrder;

pub fn play(args: &Args) -> Res {
    let options = args.playback_options()?;
    let requested_normalization = normalization_mode(args)?;
    let tone = tone_controls(args)?;
    let raw = args.positionals.join(" ");
    if raw.trim().is_empty() {
        return Err(
            "give a file, folder, M3U, collection, artist, album or title: aede play <selection>"
                .into(),
        );
    }
    let catalog = store::load(&store::catalog_path(&data_dir(args)))?;
    let selection = resolve(&raw, catalog.as_ref(), Some(args))?;
    let normalization_mode = selection.normalization_mode(requested_normalization);
    let paths = selection.paths;
    let mut order = PlaybackOrder::new(&paths, catalog.as_ref(), &options)?;
    let mut normalization = ReadyNormalization::new(
        &paths,
        selection.is_album,
        catalog.as_ref(),
        &data_dir(args),
        normalization_mode,
    )?;
    let controls = Controls::start()?;
    if controls.is_some() {
        println!(
            "Controls: Space pause/resume · n/→ next · p/← previous · [/] seek 10s · r repeat · z shuffle · q/Ctrl-C stop"
        );
    }
    let mut output = LocalOutput::new()?;
    // Repeat may run indefinitely. Slow storage must back-pressure the producer
    // rather than accumulate an unbounded number of private listening events.
    let (history_send, history_receive) = mpsc::sync_channel::<PlaybackRecord>(64);
    let history_args = args.clone();
    let history_worker = std::thread::spawn(move || -> Result<(), String> {
        for item in history_receive {
            match item {
                PlaybackRecord::History(item) => record_play(
                    &history_args,
                    &item.path,
                    item.started,
                    item.played_ms,
                    item.completed,
                )
                .map_err(|error| error.to_string())?,
                PlaybackRecord::Loudness(update) => {
                    if let Err(error) = update.save(&data_dir(&history_args)) {
                        eprintln!("Warning: could not cache playback loudness: {error}");
                    }
                }
            }
        }
        Ok(())
    });
    let playback_result = session::play_selection(
        &paths,
        catalog.as_ref(),
        &mut normalization,
        controls.as_ref(),
        &mut output,
        &history_send,
        session::SelectionSettings {
            normalization_mode,
            tone,
            order: &mut order,
            options,
        },
    );
    drop(history_send);
    let history_result = history_worker
        .join()
        .map_err(|_| "listening history worker panicked")?;
    playback_result?;
    Ok(history_result?)
}

fn normalization_mode(args: &Args) -> Result<Option<NormalizationMode>, Box<dyn Error>> {
    match args.value("normalize") {
        None => Ok(None),
        Some("off") => Ok(Some(NormalizationMode::Off)),
        Some("track") => Ok(Some(NormalizationMode::Track)),
        Some("album") => Ok(Some(NormalizationMode::Album)),
        other => Err(format!("normalization must be off, track or album; got {other:?}").into()),
    }
}

fn tone_controls(args: &Args) -> Result<ToneControls, Box<dyn Error>> {
    fn level(args: &Args, name: &str) -> Result<f32, Box<dyn Error>> {
        if !args.has(name) {
            return Ok(0.0);
        }
        let raw = args
            .value(name)
            .ok_or_else(|| format!("--{name} needs a dB value"))?;
        Ok(raw
            .parse::<f32>()
            .map_err(|_| format!("--{name} needs a numeric dB value"))?)
    }
    let bass = level(args, "bass")?;
    let treble = level(args, "treble")?;
    ToneControls::new(bass, treble).map_err(|error| error.into())
}

struct PlaybackSelection {
    paths: Vec<PathBuf>,
    is_album: bool,
}

impl PlaybackSelection {
    fn mixed(paths: Vec<PathBuf>) -> Self {
        Self {
            paths,
            is_album: false,
        }
    }

    fn album(paths: Vec<PathBuf>) -> Self {
        Self {
            paths,
            is_album: true,
        }
    }

    fn normalization_mode(&self, requested: Option<NormalizationMode>) -> NormalizationMode {
        requested.unwrap_or(if self.is_album {
            NormalizationMode::Album
        } else {
            NormalizationMode::Track
        })
    }
}

struct HistoryItem {
    path: PathBuf,
    started: u64,
    played_ms: u64,
    completed: bool,
}

enum PlaybackRecord {
    History(HistoryItem),
    Loudness(gain_plan::LoudnessUpdate),
}

struct PlaybackSettings {
    normalization_mode: NormalizationMode,
    selected_gain: Option<GainPlan>,
    tone: ToneControls,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaybackEnd {
    Natural,
    Stop,
    Next,
    Previous,
    SeekRelative(i64),
}

struct PlaybackClock {
    started: Instant,
    paused_since: Option<Instant>,
    paused_duration: Duration,
    repeat: Repeat,
    shuffle: PlaybackShuffle,
    shuffle_revision: u64,
    smart_available: bool,
}

impl PlaybackClock {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            paused_since: None,
            paused_duration: Duration::ZERO,
            repeat: Repeat::Off,
            shuffle: PlaybackShuffle::Off,
            shuffle_revision: 0,
            smart_available: false,
        }
    }

    fn reset_elapsed(&mut self) {
        self.started = Instant::now();
        if self.paused_since.is_some() {
            self.paused_since = Some(self.started);
        }
        self.paused_duration = Duration::ZERO;
    }

    fn active_ms(&self) -> u64 {
        let now = self.paused_since.unwrap_or_else(Instant::now);
        let elapsed = now
            .duration_since(self.started)
            .saturating_sub(self.paused_duration);
        u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
    }

    fn toggle_pause(&mut self, output: &impl PlaybackOutput) -> Res {
        if let Some(paused_since) = self.paused_since.take() {
            output.resume()?;
            self.paused_duration += paused_since.elapsed();
        } else {
            output.pause()?;
            self.paused_since = Some(Instant::now());
        }
        Ok(())
    }
}

fn control_action(
    controls: Option<&Controls>,
    output: &impl PlaybackOutput,
    clock: &mut PlaybackClock,
) -> Result<Option<PlaybackEnd>, Box<dyn Error>> {
    let Some(controls) = controls else {
        return Ok(None);
    };
    loop {
        let action = if clock.paused_since.is_some() {
            controls.wait()
        } else {
            controls.poll()
        };
        match action {
            Some(Action::Pause) => clock.toggle_pause(output)?,
            Some(Action::Stop) => return Ok(Some(PlaybackEnd::Stop)),
            Some(Action::Next) => return Ok(Some(PlaybackEnd::Next)),
            Some(Action::Previous) => return Ok(Some(PlaybackEnd::Previous)),
            Some(Action::SeekRelative(delta)) => return Ok(Some(PlaybackEnd::SeekRelative(delta))),
            Some(Action::CycleRepeat) => {
                clock.repeat = match clock.repeat {
                    Repeat::Off => Repeat::One,
                    Repeat::One => Repeat::All,
                    Repeat::All => Repeat::Off,
                };
                println!(
                    "Repeat: {}",
                    match clock.repeat {
                        Repeat::Off => "off",
                        Repeat::One => "one",
                        Repeat::All => "all",
                    }
                );
            }
            Some(Action::CycleShuffle) => {
                clock.shuffle = match clock.shuffle {
                    PlaybackShuffle::Off => PlaybackShuffle::Random,
                    PlaybackShuffle::Random if clock.smart_available => PlaybackShuffle::Smart,
                    PlaybackShuffle::Random => {
                        eprintln!("Smart shuffle unavailable without a scanned catalog");
                        PlaybackShuffle::Off
                    }
                    PlaybackShuffle::Smart => PlaybackShuffle::Off,
                };
                clock.shuffle_revision = clock.shuffle_revision.wrapping_add(1);
                println!(
                    "Shuffle: {} (future entries)",
                    match clock.shuffle {
                        PlaybackShuffle::Off => "off",
                        PlaybackShuffle::Random => "random",
                        PlaybackShuffle::Smart => "smart",
                    }
                );
            }
            None if clock.paused_since.is_some() => clock.toggle_pause(output)?,
            None => return Ok(None),
        }
    }
}

trait PlaybackOutput: Write {
    /// Accept PCM from its original samples or encoded transport bytes. The
    /// byte offset and return value retain arbitrary partial-write semantics.
    fn write_pcm(
        &mut self,
        _samples: &[f32],
        f32le: &[u8],
        byte_offset: usize,
    ) -> std::io::Result<usize> {
        self.write(&f32le[byte_offset..])
    }

    fn pause(&self) -> Res;
    fn resume(&self) -> Res;
}

impl PlaybackOutput for LocalOutput {
    fn write_pcm(
        &mut self,
        samples: &[f32],
        f32le: &[u8],
        byte_offset: usize,
    ) -> std::io::Result<usize> {
        self.write_pcm(samples, f32le, byte_offset)
    }

    fn pause(&self) -> Res {
        self.pause()
    }
    fn resume(&self) -> Res {
        self.resume()
    }
}

impl PlaybackOutput for Vec<u8> {
    fn pause(&self) -> Res {
        Ok(())
    }
    fn resume(&self) -> Res {
        Ok(())
    }
}

fn resolve(
    raw: &str,
    catalog: Option<&Catalog>,
    args: Option<&Args>,
) -> Result<PlaybackSelection, Box<dyn Error>> {
    let path = Path::new(raw);
    if path.is_file() {
        return if is_m3u(path) {
            read_m3u(path).map(PlaybackSelection::mixed)
        } else {
            Ok(PlaybackSelection::mixed(vec![super::canonical(path)]))
        };
    }
    if path.is_dir() {
        let mut paths = Vec::new();
        collect_audio(path, &mut paths)?;
        if paths.is_empty() {
            return Err(format!("no audio files in {}", path.display()).into());
        }
        return Ok(PlaybackSelection::mixed(paths));
    }
    if path.components().count() > 1 || path.is_absolute() {
        return Err(format!("no file or folder at {}", path.display()).into());
    }
    let catalog = catalog.ok_or("no catalog for name lookup; run aede scan <folder> first")?;
    if let Some(name) = raw.strip_prefix("collection:") {
        return collection_paths(
            args.ok_or("collection lookup needs a data folder")?,
            catalog,
            name,
        )
        .map(PlaybackSelection::mixed);
    }
    let (artists, artist_match) = catalog.find_artists(raw);
    let (releases, release_match) = catalog.find_releases(raw);
    let (tracks, track_match) = catalog.find_tracks(raw);
    if !artists.is_empty() && artist_match == TitleMatch::Exact {
        return paths_for_artists(catalog, &artists, raw).map(PlaybackSelection::mixed);
    }
    if !releases.is_empty() && release_match == TitleMatch::Exact {
        return paths_for_releases(catalog, releases.iter().map(|r| r.id).collect(), raw)
            .map(PlaybackSelection::album);
    }
    if !tracks.is_empty() && track_match == TitleMatch::Exact {
        return paths_for_tracks(catalog, &tracks).map(PlaybackSelection::mixed);
    }
    if !artists.is_empty() {
        return paths_for_artists(catalog, &artists, raw).map(PlaybackSelection::mixed);
    }
    if !releases.is_empty() {
        return paths_for_releases(catalog, releases.iter().map(|r| r.id).collect(), raw)
            .map(PlaybackSelection::album);
    }
    if !tracks.is_empty() {
        return paths_for_tracks(catalog, &tracks).map(PlaybackSelection::mixed);
    }
    if let Some(args) = args {
        let data = super::user_data(args, catalog)?;
        if data.collection(LOCAL_USER, raw).is_some() {
            return collection_paths(args, catalog, raw).map(PlaybackSelection::mixed);
        }
    }
    Err(format!("no artist, album, track or collection matches \"{raw}\"").into())
}

fn is_m3u(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("m3u") || ext.eq_ignore_ascii_case("m3u8"))
}

fn read_m3u(path: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let contents = fs::read_to_string(path)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tracks = Vec::new();
    for (index, raw_line) in contents.trim_start_matches('\u{feff}').lines().enumerate() {
        let entry = raw_line.trim().trim_start_matches('\u{feff}');
        if entry.is_empty() || entry.starts_with('#') {
            continue;
        }
        if entry.contains("://") {
            return Err(format!(
                "{}:{}: remote M3U entries are not supported: {entry}",
                path.display(),
                index + 1
            )
            .into());
        }
        let named = Path::new(entry);
        let file = if named.is_absolute() {
            named.to_path_buf()
        } else {
            parent.join(named)
        };
        if !file.is_file() || !tags::is_audio_path(&file) {
            return Err(format!(
                "{}:{}: no playable audio file at {}",
                path.display(),
                index + 1,
                file.display()
            )
            .into());
        }
        tracks.push(super::canonical(&file));
    }
    if tracks.is_empty() {
        return Err(format!("no audio files in M3U {}", path.display()).into());
    }
    Ok(tracks)
}

fn collection_paths(
    args: &Args,
    catalog: &Catalog,
    name: &str,
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let data = super::user_data(args, catalog)?;
    let saved = data
        .collection(LOCAL_USER, name)
        .ok_or_else(|| format!("no collection is called \"{name}\""))?;
    let parsed = query::parse(&saved.expression)?;
    let held = super::sources_held(args)?;
    let context = query::Context::new(catalog, &data, LOCAL_USER).with_sources(&held);
    super::ensure_query_values(&parsed, &context)?;
    let ids = query::run(&parsed, &context);
    let mut paths = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(file) = catalog
            .track(id)
            .and_then(|track| catalog.file(track.file_id))
        else {
            return Err(
                format!("collection \"{name}\" contains a track without an audio file").into(),
            );
        };
        paths.push(PathBuf::from(&file.path));
    }
    if paths.is_empty() {
        return Err(format!("collection \"{name}\" has no tracks to play").into());
    }
    Ok(paths)
}

fn playing_label(path: &Path, catalog: Option<&Catalog>) -> String {
    let album = catalog
        .and_then(|catalog| {
            catalog.tracks.iter().find_map(|track| {
                (catalog.file(track.file_id)?.path == path.to_string_lossy())
                    .then(|| track.release_id.and_then(|id| catalog.release(id)))
                    .flatten()
            })
        })
        .map(|release| release.title.as_str())
        .filter(|title| !title.is_empty())
        .or_else(|| {
            path.parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
        })
        .unwrap_or("Unknown album");
    let title = path.file_stem().unwrap_or_default().to_string_lossy();
    crate::ui::literal(&format!("{album} — {title}")).replace(['\n', '\t'], " ")
}

fn paths_for_artists(
    catalog: &Catalog,
    artists: &[&aede_core::model::Artist],
    raw: &str,
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    if artists.len() > 1 {
        return Err(format!("\"{raw}\" matches several artists; use a fuller name").into());
    }
    if let Some(artist) = artists.first() {
        let ids = catalog.releases_as_album_artist(artist.id);
        return paths_for_releases(catalog, ids, raw);
    }
    Err(format!("no artist matches \"{raw}\"").into())
}

fn paths_for_tracks(
    catalog: &Catalog,
    tracks: &[&aede_core::model::Track],
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut paths = tracks
        .iter()
        .filter_map(|track| {
            catalog
                .file(track.file_id)
                .map(|file| PathBuf::from(&file.path))
        })
        .collect::<Vec<_>>();
    paths.sort();
    if paths.is_empty() {
        return Err("matching tracks have no audio files".into());
    }
    Ok(paths)
}

fn paths_for_releases(
    catalog: &Catalog,
    mut ids: Vec<Id>,
    name: &str,
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    ids.sort_by(|a, b| {
        let left = catalog.release(*a);
        let right = catalog.release(*b);
        left.map(|r| (r.year.unwrap_or(u32::MAX), &r.folder, &r.title))
            .cmp(&right.map(|r| (r.year.unwrap_or(u32::MAX), &r.folder, &r.title)))
    });
    let mut paths = Vec::new();
    for id in ids {
        if let Some(release) = catalog.release(id) {
            for track_id in &release.track_ids {
                if let Some(path) = catalog
                    .track(*track_id)
                    .and_then(|track| catalog.file(track.file_id))
                    .map(|file| PathBuf::from(&file.path))
                {
                    paths.push(path);
                }
            }
        }
    }
    if paths.is_empty() {
        return Err(format!("no playable tracks for \"{name}\"").into());
    }
    Ok(paths)
}

fn collect_audio(folder: &Path, paths: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    let mut entries = fs::read_dir(folder)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            collect_audio(&path, paths)?;
        } else if kind.is_file() && tags::is_audio_path(&path) {
            paths.push(super::canonical(&path));
        }
    }
    Ok(())
}

fn record_play(args: &Args, path: &Path, started: u64, played_ms: u64, completed: bool) -> Res {
    let directory = data_dir(args);
    let _lock = StoreLock::acquire(&directory)?;
    let user_path = user::user_path(&directory);
    let mut data = user::load(&user_path)?.unwrap_or_default();
    data.record_play(Play {
        owner: LOCAL_USER.to_string(),
        track: EntityRef::new(EntityKind::Track, path.to_string_lossy().into_owned()),
        at: started,
        ms_played: played_ms,
        completed,
    });
    user::save(&data, &user_path)?;
    Ok(())
}

fn peak_db(peak: f32) -> String {
    if peak == 0.0 {
        "−∞".to_string()
    } else {
        format!("{:+.2}", 20.0 * peak.log10())
    }
}

#[cfg(test)]
#[path = "play_tests.rs"]
mod tests;
