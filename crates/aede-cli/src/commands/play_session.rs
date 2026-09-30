//! Local transport around the shared, continuous PCM processing session.

use std::collections::BTreeMap;

use aede_core::playback::session::{PcmSession, SessionBlock, TrackSpan};
use aede_dsp::PcmFormat;

use super::*;

trait SessionOutput: PlaybackOutput {
    fn needs_reopen(&mut self, format: PcmFormat) -> Result<bool, Box<dyn Error>>;
    fn prepare(&mut self, format: PcmFormat) -> Result<PcmFormat, Box<dyn Error>>;
    fn close_input(&mut self);
    fn drain_progress(&mut self) -> Result<output::DrainProgress, Box<dyn Error>>;
    fn host_tail(&self) -> Duration;
    fn finish(&mut self) -> Res;
}

impl SessionOutput for LocalOutput {
    fn needs_reopen(&mut self, format: PcmFormat) -> Result<bool, Box<dyn Error>> {
        self.needs_reopen(format)
    }
    fn prepare(&mut self, format: PcmFormat) -> Result<PcmFormat, Box<dyn Error>> {
        self.prepare(format)
    }
    fn close_input(&mut self) {
        self.close_input();
    }
    fn drain_progress(&mut self) -> Result<output::DrainProgress, Box<dyn Error>> {
        self.drain_progress()
    }
    fn host_tail(&self) -> Duration {
        self.host_tail()
    }
    fn finish(&mut self) -> Res {
        self.finish()
    }
}

enum PreparedOutput {
    Ready(PcmFormat),
    Interrupted(PlaybackEnd),
}

struct PendingTrack {
    index: usize,
    path: PathBuf,
    started: u64,
    active_started_ms: u64,
    format: PcmFormat,
    frames: u64,
    submitted_bytes: u64,
    complete: bool,
    meter: OutputMeter,
    loudness: Option<gain_plan::LoudnessUpdate>,
}

impl PendingTrack {
    fn submitted_ms(&self) -> u64 {
        self.frames.saturating_mul(1000) / u64::from(self.format.sample_rate())
    }

    fn interrupted_ms(&self, clock: &PlaybackClock) -> u64 {
        self.submitted_ms()
            .min(clock.active_ms().saturating_sub(self.active_started_ms))
    }
}

struct PlaybackRecords<'a> {
    pending: BTreeMap<usize, PendingTrack>,
    sender: &'a mpsc::Sender<PlaybackRecord>,
    selection_len: usize,
    last_position: Option<(usize, u64)>,
    visualizer: Option<TerminalVisualizer>,
}

impl<'a> PlaybackRecords<'a> {
    fn new(sender: &'a mpsc::Sender<PlaybackRecord>, selection_len: usize) -> Self {
        Self {
            pending: BTreeMap::new(),
            sender,
            selection_len,
            last_position: None,
            visualizer: None,
        }
    }

    fn begin(
        &mut self,
        token: usize,
        index: usize,
        path: &Path,
        format: PcmFormat,
        clock: &PlaybackClock,
    ) {
        let meter = match OutputMeter::new(format) {
            Ok(meter) => meter,
            Err(error) => {
                eprintln!("True-peak meter unavailable: {error}");
                OutputMeter::sample_peak_only(format)
            }
        };
        self.pending.insert(
            token,
            PendingTrack {
                index,
                path: path.to_path_buf(),
                started: clock::now_seconds(),
                active_started_ms: clock.active_ms(),
                format,
                frames: 0,
                submitted_bytes: 0,
                complete: false,
                meter,
                loudness: None,
            },
        );
    }

    fn source_finished(&mut self, token: usize, update: Option<gain_plan::LoudnessUpdate>) -> Res {
        self.pending
            .get_mut(&token)
            .ok_or("playback track record is missing")?
            .loudness = update;
        Ok(())
    }

    fn submitted(&mut self, event: Submitted<'_>) -> Res {
        match event {
            Submitted::Bytes { token, count } => {
                let record = self
                    .pending
                    .get_mut(&token)
                    .ok_or("output refers to an unknown playback track")?;
                record.submitted_bytes = record.submitted_bytes.saturating_add(count as u64);
                record.frames = record.submitted_bytes / (u64::from(record.format.channels()) * 4);
                Ok(())
            }
            Submitted::Complete { span, samples } => self.accepted(span, samples),
        }
    }

    fn accepted(&mut self, span: &TrackSpan, samples: &[f32]) -> Res {
        let record = self
            .pending
            .get_mut(&span.token)
            .ok_or("output refers to an unknown playback track")?;
        if !samples.is_empty() {
            match record.meter.observe(samples, span.stats) {
                Ok(()) => {}
                Err(OutputMeterError::Meter(error)) => {
                    eprintln!("True-peak meter stopped: {error}")
                }
                Err(error) => return Err(error.into()),
            }
            if let Some(visualizer) = &mut self.visualizer
                && let Err(error) = visualizer.observe(samples)
            {
                eprintln!("visualizer stopped: {error}");
                visualizer.disable();
            }
        }
        record.complete |= span.complete;
        // The last listen stays pending until the output has actually drained,
        // preserving Stop/Next's incomplete-history behavior at the final tail.
        if record.complete && record.index + 1 < self.selection_len {
            self.publish(span.token, true, None)?;
        }
        Ok(())
    }

    fn position_ms(&self, index: usize, clock: &PlaybackClock) -> u64 {
        self.pending
            .values()
            .rev()
            .find(|record| record.index == index)
            .map(|record| record.interrupted_ms(clock))
            .or_else(|| {
                self.last_position
                    .filter(|(last, _)| *last == index)
                    .map(|(_, ms)| ms)
            })
            .unwrap_or(0)
    }

    fn publish(&mut self, token: usize, completed: bool, clock: Option<&PlaybackClock>) -> Res {
        let mut record = self
            .pending
            .remove(&token)
            .ok_or("playback track record is missing")?;
        let played_ms = clock.map_or_else(
            || record.submitted_ms(),
            |clock| record.interrupted_ms(clock),
        );
        self.last_position = Some((record.index, played_ms));
        let measured_frames = record.meter.sample_peak_snapshot().frames;
        report_meter(&mut record.meter);
        if measured_frames < record.frames {
            eprintln!(
                "Signal diagnostics: {} frames in a partially submitted output block have no complete peak/guard statistics",
                record.frames - measured_frames,
            );
        }
        if played_ms > 0 {
            self.sender
                .send(PlaybackRecord::History(HistoryItem {
                    path: record.path,
                    started: record.started,
                    played_ms,
                    completed,
                }))
                .map_err(|_| "listening history worker stopped")?;
        }
        if completed && let Some(update) = record.loudness {
            self.sender
                .send(PlaybackRecord::Loudness(update))
                .map_err(|_| "playback record worker stopped")?;
        }
        Ok(())
    }

    fn finish(&mut self, completed: bool, clock: &PlaybackClock) -> Res {
        let tokens = self.pending.keys().copied().collect::<Vec<_>>();
        for token in tokens {
            let natural = completed
                && self
                    .pending
                    .get(&token)
                    .is_some_and(|record| record.complete);
            self.publish(token, natural, (!natural).then_some(clock))?;
        }
        Ok(())
    }
}

pub(super) enum Submitted<'a> {
    // Byte progress preserves complete-frame history even if a later write
    // fails halfway through a span. Peak/guard stats require the entire span.
    Bytes {
        token: usize,
        count: usize,
    },
    Complete {
        span: &'a TrackSpan,
        samples: &'a [f32],
    },
}

/// Submit attributed portions, reporting byte progress immediately and only
/// publishing a span's aggregate meter statistics once it is fully accepted.
pub(super) fn submit_block(
    block: SessionBlock<'_>,
    output: &mut impl PlaybackOutput,
    controls: Option<&Controls>,
    clock: &mut PlaybackClock,
    mut submitted: impl FnMut(Submitted<'_>) -> Res,
) -> Result<PlaybackEnd, Box<dyn Error>> {
    for span in block.spans {
        let samples = &block.samples[span.samples.clone()];
        let bytes = &block.f32le[span.samples.start * 4..span.samples.end * 4];
        let mut byte_offset = 0;
        while byte_offset < bytes.len() {
            if let Some(action) = control_action(controls, output, clock)? {
                return Ok(action);
            }
            match output.write_pcm(samples, bytes, byte_offset) {
                Ok(0) => return Err("audio output closed before accepting PCM".into()),
                Ok(count) if count <= bytes.len() - byte_offset => {
                    submitted(Submitted::Bytes {
                        token: span.token,
                        count,
                    })?;
                    byte_offset += count;
                }
                Ok(_) => return Err("audio output reported an invalid write length".into()),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(format!("audio output closed: {error}").into()),
            }
        }
        submitted(Submitted::Complete { span, samples })?;
    }
    Ok(PlaybackEnd::Natural)
}

fn flush_group(
    processing: &mut Option<PcmSession>,
    records: &mut PlaybackRecords<'_>,
    output: &mut impl PlaybackOutput,
    controls: Option<&Controls>,
    clock: &mut PlaybackClock,
) -> Result<PlaybackEnd, Box<dyn Error>> {
    let Some(session) = processing.as_mut() else {
        return Ok(PlaybackEnd::Natural);
    };
    let result = submit_block(session.finish()?, output, controls, clock, |event| {
        records.submitted(event)
    })?;
    *processing = None;
    Ok(result)
}

fn drain_output(
    output: &mut LocalOutput,
    controls: Option<&Controls>,
    clock: &mut PlaybackClock,
) -> Result<PlaybackEnd, Box<dyn Error>> {
    drain_output_with(
        output,
        clock,
        |output, clock| control_action(controls, output, clock),
        PlaybackClock::active_ms,
        || std::thread::sleep(Duration::from_millis(25)),
    )
}

fn drain_output_with<O: SessionOutput>(
    output: &mut O,
    clock: &mut PlaybackClock,
    mut control: impl FnMut(&O, &mut PlaybackClock) -> Result<Option<PlaybackEnd>, Box<dyn Error>>,
    mut active_ms: impl FnMut(&PlaybackClock) -> u64,
    mut wait: impl FnMut(),
) -> Result<PlaybackEnd, Box<dyn Error>> {
    output.close_input();
    let mut drain = DrainWait::new(active_ms(clock));
    loop {
        if let Some(action) = control(output, clock)? {
            return Ok(action);
        }
        if drain.advance(
            active_ms(clock),
            output.drain_progress()?,
            output.host_tail(),
        )? {
            output.finish()?;
            return Ok(PlaybackEnd::Natural);
        }
        wait();
    }
}

// A stalled native callback must not trap noninteractive playback forever.
// This is a progress timeout, independent of the 500 ms queue capacity and
// deliberately tolerant of coarse host callbacks. Pauses use no active time.
const DRAIN_STALL_MILLISECONDS: u64 = 5_000;

struct DrainWait {
    last_consumed: Option<u64>,
    last_progress_ms: u64,
    tail_started_ms: Option<u64>,
}

impl DrainWait {
    fn new(active_ms: u64) -> Self {
        Self {
            last_consumed: None,
            last_progress_ms: active_ms,
            tail_started_ms: None,
        }
    }

    fn advance(
        &mut self,
        active_ms: u64,
        progress: output::DrainProgress,
        host_tail: Duration,
    ) -> Result<bool, Box<dyn Error>> {
        if progress.drained {
            let tail_started = *self.tail_started_ms.get_or_insert(active_ms);
            return Ok(u128::from(active_ms.saturating_sub(tail_started)) >= host_tail.as_millis());
        }
        self.tail_started_ms = None;
        // ffplay cannot report consumed frames: a timeout with no progress
        // evidence would wrongly assume its buffered audio has stopped.
        if let Some(consumed) = progress.consumed_frames {
            if self.last_consumed != Some(consumed) {
                self.last_consumed = Some(consumed);
                self.last_progress_ms = active_ms;
            } else if active_ms.saturating_sub(self.last_progress_ms) >= DRAIN_STALL_MILLISECONDS {
                return Err(
                    "native output drain stalled: no consumed-frame progress for 5 seconds".into(),
                );
            }
        }
        Ok(false)
    }
}

fn prepare_output_with<O: SessionOutput>(
    output: &mut O,
    format: PcmFormat,
    clock: &mut PlaybackClock,
    control: impl FnMut(&O, &mut PlaybackClock) -> Result<Option<PlaybackEnd>, Box<dyn Error>>,
    active_ms: impl FnMut(&PlaybackClock) -> u64,
    wait: impl FnMut(),
) -> Result<PreparedOutput, Box<dyn Error>> {
    if output.needs_reopen(format)? {
        let end = drain_output_with(output, clock, control, active_ms, wait)?;
        if end != PlaybackEnd::Natural {
            return Ok(PreparedOutput::Interrupted(end));
        }
    }
    output.prepare(format).map(PreparedOutput::Ready)
}

pub(super) fn play_selection(
    paths: &[PathBuf],
    catalog: Option<&Catalog>,
    normalization_mode: NormalizationMode,
    tone: ToneControls,
    normalization: &mut ReadyNormalization<'_>,
    controls: Option<&Controls>,
    output: &mut LocalOutput,
    sender: &mpsc::Sender<PlaybackRecord>,
) -> Res {
    let mut records = PlaybackRecords::new(sender, paths.len());
    let mut processing: Option<PcmSession> = None;
    let mut playback_clock = PlaybackClock::new();
    let mut index = 0;
    let mut cursor = 0;
    let mut token = 0usize;
    let result = (|| -> Res {
        loop {
            while index < paths.len() {
                if let Some(action) = control_action(controls, output, &mut playback_clock)? {
                    let played_ms = records.position_ms(cursor, &playback_clock);
                    output.abort()?;
                    processing = None;
                    normalization.finish_track(index, false)?;
                    records.finish(false, &playback_clock)?;
                    let Some(next) = next_index(cursor, paths.len(), played_ms, action) else {
                        return Ok(());
                    };
                    index = next;
                    playback_clock = PlaybackClock::new();
                    continue;
                }
                let path = &paths[index];
                let prepared = (|| -> Result<_, Box<dyn Error>> {
                    let gain = normalization.prepare(index)?;
                    let track = PcmTrack::open_stereo(path)?;
                    Ok((gain, track))
                })();
                let (selected_gain, mut track) = match prepared {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        // The preceding source ended naturally. Deliver its pending
                        // filter tail before reporting a bad subsequent file.
                        let end = flush_group(
                            &mut processing,
                            &mut records,
                            output,
                            controls,
                            &mut playback_clock,
                        )?;
                        let end = if end == PlaybackEnd::Natural {
                            drain_output(output, controls, &mut playback_clock)?
                        } else {
                            end
                        };
                        if end == PlaybackEnd::Natural {
                            records.finish(true, &playback_clock)?;
                            return Err(error);
                        }
                        eprintln!("Warning: could not prepare the next track: {error}");
                        let played_ms = records.position_ms(cursor, &playback_clock);
                        output.abort()?;
                        normalization.finish_track(index, false)?;
                        records.finish(false, &playback_clock)?;
                        let Some(next) = next_index(cursor, paths.len(), played_ms, end) else {
                            return Ok(());
                        };
                        index = next;
                        playback_clock = PlaybackClock::new();
                        continue;
                    }
                };
                let input_format = track.format();
                if processing
                    .as_ref()
                    .is_some_and(|session| session.input_format() != input_format)
                {
                    let action = flush_group(
                        &mut processing,
                        &mut records,
                        output,
                        controls,
                        &mut playback_clock,
                    )?;
                    if action != PlaybackEnd::Natural {
                        let played_ms = records.position_ms(cursor, &playback_clock);
                        output.abort()?;
                        normalization.finish_track(index, false)?;
                        records.finish(false, &playback_clock)?;
                        let Some(next) = next_index(cursor, paths.len(), played_ms, action) else {
                            return Ok(());
                        };
                        index = next;
                        playback_clock = PlaybackClock::new();
                        continue;
                    }
                }
                let format = if let Some(session) = &processing {
                    session.output_format()
                } else {
                    let format = match prepare_output_with(
                        output,
                        input_format,
                        &mut playback_clock,
                        |output, clock| control_action(controls, output, clock),
                        PlaybackClock::active_ms,
                        || std::thread::sleep(Duration::from_millis(25)),
                    )? {
                        PreparedOutput::Ready(format) => format,
                        PreparedOutput::Interrupted(action) => {
                            let played_ms = records.position_ms(cursor, &playback_clock);
                            output.abort()?;
                            normalization.finish_track(index, false)?;
                            records.finish(false, &playback_clock)?;
                            let Some(next) = next_index(cursor, paths.len(), played_ms, action)
                            else {
                                return Ok(());
                            };
                            index = next;
                            playback_clock = PlaybackClock::new();
                            continue;
                        }
                    };
                    processing = Some(PcmSession::new(input_format, format.sample_rate(), tone)?);
                    records.visualizer = TerminalVisualizer::new(format);
                    format
                };
                let settings = PlaybackSettings {
                    normalization_mode,
                    selected_gain,
                    tone,
                };
                let gain_db = describe_track(
                    &track,
                    format,
                    output,
                    &settings,
                    normalization.is_measuring(),
                    &playing_label(path, catalog),
                    records.visualizer.is_some(),
                )?;
                token = token
                    .checked_add(1)
                    .ok_or("playback track token exhausted")?;
                cursor = index;
                records.begin(token, index, path, format, &playback_clock);
                let session = processing
                    .as_mut()
                    .ok_or("PCM processing session is missing")?;
                session.begin_track(token, gain_db)?;
                let end = loop {
                    if let Some(action) = control_action(controls, output, &mut playback_clock)? {
                        break action;
                    }
                    let raw = track.read_block_observed(
                        |format, samples| {
                            if let Err(error) = normalization.observe_source(format, samples) {
                                eprintln!("Source loudness meter stopped: {error}");
                            }
                        },
                        |_| Ok::<(), std::convert::Infallible>(()),
                    )?;
                    let end = if let Some(raw) = raw {
                        submit_block(
                            session.push_source(raw.samples)?,
                            output,
                            controls,
                            &mut playback_clock,
                            |event| records.submitted(event),
                        )?
                    } else {
                        let update = match normalization.finish_track(index, true) {
                            Ok(update) => update,
                            Err(error) => {
                                eprintln!("Warning: source loudness was not cached: {error}");
                                None
                            }
                        };
                        records.source_finished(token, update)?;
                        break submit_block(
                            session.end_track()?,
                            output,
                            controls,
                            &mut playback_clock,
                            |event| records.submitted(event),
                        )?;
                    };
                    if end != PlaybackEnd::Natural {
                        break end;
                    }
                };
                if end != PlaybackEnd::Natural {
                    let played_ms = records.position_ms(index, &playback_clock);
                    output.abort()?;
                    processing = None;
                    normalization.finish_track(index, false)?;
                    records.finish(false, &playback_clock)?;
                    let Some(next) = next_index(index, paths.len(), played_ms, end) else {
                        return Ok(());
                    };
                    index = next;
                    playback_clock = PlaybackClock::new();
                    continue;
                }
                if output.stopped_early()? {
                    return Err("audio output stopped before the selection ended".into());
                }
                index += 1;
            }
            let end = flush_group(
                &mut processing,
                &mut records,
                output,
                controls,
                &mut playback_clock,
            )?;
            let end = if end == PlaybackEnd::Natural {
                drain_output(output, controls, &mut playback_clock)?
            } else {
                end
            };
            if end == PlaybackEnd::Natural {
                records.finish(true, &playback_clock)?;
                return Ok(());
            }
            let played_ms = records.position_ms(cursor, &playback_clock);
            output.abort()?;
            normalization.finish_track(cursor, false)?;
            records.finish(false, &playback_clock)?;
            let Some(next) = next_index(cursor, paths.len(), played_ms, end) else {
                return Ok(());
            };
            index = next;
            playback_clock = PlaybackClock::new();
        }
    })();
    if result.is_err() {
        // Failed source/transport work must not replay a buffered filter tail.
        let _ = output.abort();
        let _ = normalization.finish_track(index, false);
        records.finish(false, &playback_clock)?;
    }
    result
}

fn report_meter(meter: &mut OutputMeter) {
    let measured = match meter.snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("True-peak meter unavailable: {error}");
            meter.sample_peak_snapshot()
        }
    };
    if measured.frames > 0 {
        let true_peak = measured
            .output_true_peak
            .map(|peak| format!("{} dBTP", peak_db(peak)))
            .unwrap_or_else(|| "unavailable".to_string());
        println!(
            "Submitted signal (before dither/device): sample peak {} dBFS; estimated true peak {true_peak}; pre-guard peak {} dBFS; guarded samples {}",
            peak_db(measured.output_sample_peak),
            peak_db(measured.pre_guard_sample_peak),
            measured.guarded_samples
        );
    }
    if measured.guarded_samples > 0 {
        eprintln!(
            "Warning: {} PCM samples were hard-clamped at full scale before output; peak metadata or filter transients may explain this",
            measured.guarded_samples
        );
    }
}

fn describe_track(
    track: &PcmTrack,
    format: PcmFormat,
    output: &LocalOutput,
    settings: &PlaybackSettings,
    measuring: bool,
    label: &str,
    spectrum: bool,
) -> Result<f32, Box<dyn Error>> {
    if track.source_format().channels() > 2 {
        println!(
            "Channels: {} → stereo (LFE omitted, peak-safe downmix)",
            track.source_format().layout().name()
        );
    }
    if let Some(description) = output.integer_description() {
        println!("Output: {description} with TPDF dither");
    }
    let resample = format.sample_rate() != track.format().sample_rate();
    if resample {
        println!(
            "Sample rate: {} → {} Hz (device conversion)",
            track.format().sample_rate(),
            format.sample_rate()
        );
    }
    let mut normalization_gain_db = 0.0;
    let mut normalization_headroom_db = 0.0;
    if let Some(selection) = settings.selected_gain {
        normalization_gain_db = gain_with_headroom_db(selection.gain_db, selection.source_peak)?;
        normalization_headroom_db = (selection.gain_db - normalization_gain_db).max(0.0);
        println!(
            "Normalization: {normalization_gain_db:+.2} dB ({})",
            selection.label
        );
        if normalization_headroom_db > 0.0 {
            println!(
                "Headroom: requested {:+.2} dB reduced to {normalization_gain_db:+.2} dB ({})",
                selection.gain_db, selection.peak_label
            );
        }
    } else if settings.normalization_mode != NormalizationMode::Off {
        println!("Normalization: no stored loudness or matching gain; decoded level retained");
    }
    if measuring {
        println!(
            "Loudness: measuring source during playback for a future listen; current gain stays fixed"
        );
    }
    let preamp_db = settings.tone.safe_preamp_db();
    if !settings.tone.is_flat() {
        println!(
            "Tone: bass {:+.1} dB, treble {:+.1} dB; headroom preamp {preamp_db:+.1} dB",
            settings.tone.bass_db(),
            settings.tone.treble_db()
        );
    }
    println!("Playing: {label}");
    let normalization_stage = match settings.selected_gain {
        Some(selection) => format!("{} {normalization_gain_db:+.2} dB", selection.label),
        None if settings.normalization_mode == NormalizationMode::Off => "off".to_string(),
        None => "no gain available".to_string(),
    };
    println!(
        "DSP stages: downmix {}; resample {}; normalization {}; tone {}; output guard on; limiter off; output {}; spectrum {}",
        if track.source_format().channels() > 2 {
            "to stereo"
        } else {
            "bypass"
        },
        if resample { "on" } else { "bypass" },
        normalization_stage,
        if settings.tone.is_flat() {
            "bypass"
        } else {
            "on"
        },
        output.stage_description(),
        if spectrum { "display on" } else { "off" }
    );
    println!(
        "Headroom reserve: normalization {:.2} dB; tone {:.2} dB; dynamic gain reduction unavailable (limiter off)",
        normalization_headroom_db, -preamp_db
    );
    Ok(normalization_gain_db + preamp_db)
}

#[cfg(test)]
#[path = "play_session_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "play_drain_tests.rs"]
mod drain_tests;
