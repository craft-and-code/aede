//! Explicit original-DSD transport, independent of the PCM/DSP session.
use super::*;
#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", any(target_env = "gnu", feature = "dop-alsa"))
))]
use crate::args::PlaybackOptions;

struct Progress {
    position: u64,
    rate: u32,
    length: u64,
    paused: bool,
    completed: bool,
    error: Option<String>,
}
trait Transport {
    fn snapshot(&self) -> Progress;
    fn pause(&self);
    fn resume(&self);
    /// Return the old route's final consumed position after native retirement.
    fn seek(&mut self, frame: u64) -> Result<Progress, String>;
    fn close(&mut self) -> Result<(), String>;
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", any(target_env = "gnu", feature = "dop-alsa"))
))]
impl From<aede_audio::dop::output::Snapshot> for Progress {
    fn from(value: aede_audio::dop::output::Snapshot) -> Self {
        Self {
            position: value.position_samples,
            rate: value.source_rate,
            length: value.source_samples,
            paused: value.paused,
            completed: value.completed,
            error: value.error,
        }
    }
}
#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", any(target_env = "gnu", feature = "dop-alsa"))
))]
impl Transport for aede_audio::dop::output::Player {
    fn snapshot(&self) -> Progress {
        self.snapshot().into()
    }
    fn pause(&self) {
        self.pause();
    }
    fn resume(&self) {
        self.resume();
    }
    fn seek(&mut self, frame: u64) -> Result<Progress, String> {
        self.seek_frame_admitted(frame, || Ok(())).map(Into::into)
    }
    fn close(&mut self) -> Result<(), String> {
        self.close()
    }
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", any(target_env = "gnu", feature = "dop-alsa"))
))]
pub(super) fn play_selection(
    paths: &[PathBuf],
    catalog: Option<&Catalog>,
    controls: Option<&Controls>,
    history: &mpsc::SyncSender<PlaybackRecord>,
    order: &mut PlaybackOrder,
    options: PlaybackOptions,
    device: &str,
) -> Res {
    let labels = playing_labels(paths, catalog);
    let mut clock = PlaybackClock::new();
    clock.repeat = options.repeat;
    clock.shuffle = options.shuffle;
    clock.smart_available = catalog.is_some();
    let mut first = true;
    let mut paused = false;
    println!(
        "DSD output: DoP 1.1, original DSD bits, dedicated native output; no DSD-to-PCM conversion"
    );
    println!("The selected DAC must support DoP. Set listening level on the DAC/amplifier.");
    while let Some(index) = order.current() {
        let mut seek_ms = if first { options.seek_ms } else { 0 };
        if let Some(end) = before_open(&mut clock, &mut seek_ms, &mut paused, |paused| {
            controls.map_or(Ok(None), |controls| {
                if paused {
                    controls.wait()
                } else {
                    controls.poll()
                }
            })
        })? {
            if !advance_selection(order, &clock, catalog, paths, end, 0)? {
                break;
            }
            continue;
        }
        order.sync(&clock, catalog, paths)?;
        let path = &paths[index];
        let started = clock::now_seconds();
        let mut player = aede_audio::dop::output::Player::open_at_admitted(
            path,
            device,
            Duration::from_millis(seek_ms),
            || Ok(()),
        )?;
        first = false;
        order.activated();
        order.describe_transition(&labels);
        let source = player.info();
        println!("Playing: {}", crate::ui::literal(&labels[index]));
        println!(
            "DSD {} Hz, {} source channel(s); stereo DoP carrier {} Hz",
            source.sample_rate,
            source.channels,
            player.output_format().carrier_rate()
        );
        let (visit, result) = drive(
            &mut player,
            &mut clock,
            seek_ms,
            || controls.map_or(Ok(None), Controls::poll),
            || std::thread::sleep(Duration::from_millis(10)),
        );
        paused = player.snapshot().paused;
        history.send(PlaybackRecord::History(HistoryItem {
            path: path.clone(),
            started,
            played_ms: visit.played_ms,
            completed: visit.completed,
        }))?;
        let end = result?;
        if source.samples == 0 && order.repeats() {
            return Err("an empty DSD source cannot repeat indefinitely".into());
        }
        if !advance_selection(order, &clock, catalog, paths, end, visit.position_ms)? {
            break;
        }
    }
    Ok(())
}

fn advance_selection(
    order: &mut PlaybackOrder,
    clock: &PlaybackClock,
    catalog: Option<&Catalog>,
    paths: &[PathBuf],
    end: PlaybackEnd,
    position_ms: u64,
) -> Result<bool, Box<dyn Error>> {
    order.sync(clock, catalog, paths)?;
    // A stopped queue retains its cursor for presentation. Only advance's
    // returned successor authorizes another native visit.
    Ok(order.advance(end, position_ms)?.is_some())
}

/// Queued keys precede device admission. A paused selection waits without
/// opening a route, and manual skips retain that pause for the next selection.
fn before_open(
    clock: &mut PlaybackClock,
    seek_ms: &mut u64,
    paused: &mut bool,
    mut poll: impl FnMut(bool) -> std::io::Result<Option<Action>>,
) -> std::io::Result<Option<PlaybackEnd>> {
    loop {
        match poll(*paused)? {
            Some(Action::Stop) => return Ok(Some(PlaybackEnd::Stop)),
            Some(Action::Next) => return Ok(Some(PlaybackEnd::Next)),
            Some(Action::Previous) => return Ok(Some(PlaybackEnd::Previous)),
            Some(Action::Pause) => *paused = !*paused,
            Some(Action::SeekRelative(delta)) => {
                *seek_ms = seek_ms.saturating_add_signed(delta);
            }
            Some(Action::CycleRepeat) => clock.cycle_repeat(),
            Some(Action::CycleShuffle) => clock.cycle_shuffle(),
            None if *paused => *paused = false,
            None => return Ok(None),
        }
    }
}

struct Visit {
    played_ms: u64,
    position_ms: u64,
    completed: bool,
}
fn milliseconds(samples: u128, rate: u32) -> u64 {
    (samples.saturating_mul(1000) / u128::from(rate)).min(u128::from(u64::MAX)) as u64
}

/// Account only callback-consumed original bits. A seek's old native route is
/// closed before its final count is added; skipped/paused bits never enter history.
fn drive(
    player: &mut impl Transport,
    clock: &mut PlaybackClock,
    initial_ms: u64,
    mut poll: impl FnMut() -> std::io::Result<Option<Action>>,
    mut wait: impl FnMut(),
) -> (Visit, Result<PlaybackEnd, Box<dyn Error>>) {
    let initial = player.snapshot();
    let mut had_seek = initial_ms != 0;
    let offset = ((u128::from(initial_ms) * u128::from(initial.rate) / 16_000)
        .min(u128::from(initial.length.div_ceil(16)))
        * 16)
        .min(u128::from(initial.length)) as u64;
    let mut counter = Counter {
        previous: offset,
        played: 0,
    };
    let result = (|| {
        loop {
            let progress = player.snapshot();
            counter.observe(&progress);
            if let Some(error) = &progress.error {
                return Err(error.clone().into());
            }
            match poll()? {
                Some(Action::Stop) => return Ok(PlaybackEnd::Stop),
                Some(Action::Next) => return Ok(PlaybackEnd::Next),
                Some(Action::Previous) => return Ok(PlaybackEnd::Previous),
                Some(Action::Pause) => {
                    if progress.paused {
                        player.resume();
                    } else {
                        player.pause();
                    }
                }
                Some(Action::CycleRepeat) => clock.cycle_repeat(),
                Some(Action::CycleShuffle) => clock.cycle_shuffle(),
                Some(Action::SeekRelative(delta)) => {
                    let position_ms = milliseconds(u128::from(progress.position), progress.rate);
                    let target_ms = position_ms.saturating_add_signed(delta);
                    let frame = (u128::from(target_ms) * u128::from(progress.rate) / 16_000)
                        .min(u128::from(progress.length.div_ceil(16)))
                        as u64;
                    let old = player.seek(frame)?;
                    counter.observe(&old);
                    // End this segment before moving the baseline to the new
                    // source position. No part of the skipped interval is played.
                    counter.previous = frame.saturating_mul(16).min(progress.length);
                    had_seek = true;
                    continue;
                }
                None => {}
            }
            if progress.completed {
                return Ok(PlaybackEnd::Natural);
            }
            wait();
        }
    })();
    let cleanup = player.close();
    let final_progress = player.snapshot();
    counter.observe(&final_progress);
    let result = match (result, cleanup) {
        (Ok(end), Ok(())) => Ok(end),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
        (Err(error), Err(cleanup)) => Err(format!("{error}; {cleanup}").into()),
    };
    let visit = Visit {
        played_ms: milliseconds(counter.played, initial.rate),
        position_ms: milliseconds(u128::from(final_progress.position), initial.rate),
        completed: result
            .as_ref()
            .is_ok_and(|end| *end == PlaybackEnd::Natural)
            && final_progress.completed
            && final_progress.error.is_none()
            && !had_seek,
    };
    (visit, result)
}

struct Counter {
    previous: u64,
    played: u128,
}
impl Counter {
    fn observe(&mut self, progress: &Progress) {
        self.played += u128::from(progress.position.saturating_sub(self.previous));
        self.previous = progress.position;
    }
}

#[cfg(test)]
#[path = "play_dop_tests.rs"]
mod tests;
