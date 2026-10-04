//! Local path occurrences use the core queue, including repeated playlist entries.

use std::collections::BTreeMap;
use std::time::SystemTime;

use aede_core::playback::shuffle::SmartShuffle;
use aede_core::playback::{Queue, Repeat};

use super::*;
use crate::args::{PlaybackOptions, PlaybackShuffle};

pub(super) struct PlaybackOrder {
    queue: Queue,
    mode: PlaybackShuffle,
    revision: u64,
    seed: u64,
    smart: Option<SmartShuffle>,
    preparing_advance: bool,
    last_active: Option<usize>,
}

impl PlaybackOrder {
    pub(super) fn new(
        paths: &[PathBuf],
        catalog: Option<&Catalog>,
        options: &PlaybackOptions,
    ) -> Result<Self, Box<dyn Error>> {
        // These IDs identify occurrences in this path selection, rather than
        // looking up catalog IDs: direct files need no catalog and M3U repeats
        // must remain distinct. Smart metadata uses separate real catalog IDs.
        let entries = (0..paths.len())
            .map(|index| Id::try_from(index).map_err(|_| "too many playback entries"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut order = Self {
            queue: Queue::new(entries, None),
            mode: PlaybackShuffle::Off,
            revision: 0,
            seed: options.seed.unwrap_or_else(fresh_seed),
            smart: None,
            preparing_advance: false,
            last_active: None,
        };
        order.queue.set_repeat(options.repeat);
        order.change_mode(options.shuffle, catalog, paths)?;
        order.queue.play();
        Ok(order)
    }

    pub(super) fn current(&self) -> Option<usize> {
        self.queue.current_entry()
    }

    pub(super) fn focus(&mut self, index: usize) -> Res {
        // A natural advance selects the next source before it is opened. Undo
        // that speculative step in its actual cycle, rather than finding the
        // old occurrence at an unrelated position in a new shuffle cycle.
        if self.preparing_advance {
            self.queue.set_position_ms(0);
            self.queue.previous();
            self.preparing_advance = false;
            if self.current() != Some(index) {
                return Err("active playback cycle could not be restored".into());
            }
            if let Some(seed) = self.queue.seed() {
                self.seed = seed;
            }
            return Ok(());
        }
        if !self.queue.select_entry(index) {
            return Err("playback occurrence is missing".into());
        }
        Ok(())
    }

    pub(super) fn activated(&mut self) {
        self.preparing_advance = false;
    }

    pub(super) fn describe_transition(&mut self, labels: &[String]) {
        let Some(next) = self.current() else {
            return;
        };
        if self.mode == PlaybackShuffle::Smart
            && let Some(previous) = self.last_active.filter(|&previous| previous != next)
            && let Some(smart) = &self.smart
        {
            let distance = smart.transition_distance(previous, next);
            if distance
                .is_none_or(|distance| distance > aede_core::playback::shuffle::MAX_STYLE_STEP)
            {
                let reason = distance.map_or_else(
                    || "genre evidence unavailable".to_owned(),
                    |distance| format!("style distance {distance}/1000"),
                );
                eprintln!(
                    "Smart transition: {} → {} ({reason})",
                    labels[previous], labels[next]
                );
            }
        }
        self.last_active = Some(next);
    }

    pub(super) fn continues(&self) -> bool {
        self.queue.repeat() != Repeat::Off
            || self
                .queue
                .cursor()
                .is_some_and(|cursor| cursor + 1 < self.queue.selection().len())
    }

    pub(super) fn repeats(&self) -> bool {
        self.queue.repeat() != Repeat::Off
    }

    pub(super) fn sync(
        &mut self,
        clock: &PlaybackClock,
        catalog: Option<&Catalog>,
        paths: &[PathBuf],
    ) -> Res {
        self.queue.set_repeat(clock.repeat);
        if self.revision != clock.shuffle_revision {
            self.change_mode(clock.shuffle, catalog, paths)?;
            self.revision = clock.shuffle_revision;
        }
        Ok(())
    }

    fn change_mode(
        &mut self,
        mode: PlaybackShuffle,
        catalog: Option<&Catalog>,
        paths: &[PathBuf],
    ) -> Res {
        let order = match mode {
            PlaybackShuffle::Off => (0..paths.len()).collect(),
            PlaybackShuffle::Random => Queue::new(self.queue.selection().to_vec(), Some(self.seed))
                .order()
                .to_vec(),
            PlaybackShuffle::Smart => {
                if self.smart.is_none() {
                    let catalog = catalog
                        .ok_or("smart shuffle needs a catalog; scan the music folder first")?;
                    let ids = catalog
                        .tracks
                        .iter()
                        .filter_map(|track| {
                            Some((catalog.file(track.file_id)?.path.as_str(), track.id))
                        })
                        .collect::<BTreeMap<_, _>>();
                    let selected = paths
                        .iter()
                        .map(|path| ids.get(path.to_string_lossy().as_ref()).copied())
                        .collect::<Vec<_>>();
                    self.smart = Some(SmartShuffle::from_catalog(catalog, &selected));
                }
                let smart = self
                    .smart
                    .as_ref()
                    .ok_or("smart shuffle metadata is missing")?;
                let prefix = self.queue.cursor().map_or(0, |cursor| cursor + 1);
                let remaining = &self.queue.order()[prefix..];
                let plan = smart.plan_remaining(self.seed, remaining, self.current())?;
                report_smart(&plan.report);
                let mut full = self.queue.order()[..prefix].to_vec();
                full.extend(plan.order);
                full
            }
        };
        let seed = (mode != PlaybackShuffle::Off).then_some(self.seed);
        self.queue.reorder(order, seed)?;
        self.mode = mode;
        if let Some(seed) = seed {
            println!("Shuffle seed: {seed}");
        }
        Ok(())
    }

    pub(super) fn advance(
        &mut self,
        end: PlaybackEnd,
        position_ms: u64,
    ) -> Result<Option<usize>, Box<dyn Error>> {
        self.queue.set_position_ms(position_ms);
        let wraps = matches!(end, PlaybackEnd::Natural | PlaybackEnd::Next)
            && self.queue.repeat() == Repeat::All
            && self
                .queue
                .cursor()
                .is_some_and(|cursor| cursor + 1 == self.queue.selection().len());
        if wraps && self.mode == PlaybackShuffle::Smart {
            let seed = self
                .queue
                .next_seed()
                .ok_or("smart shuffle seed is missing")?;
            let plan = self
                .smart
                .as_ref()
                .ok_or("smart shuffle metadata is missing")?
                .plan(seed, None, self.current());
            if self.queue.set_next_order(plan.order, Some(seed))? {
                report_smart(&plan.report);
            }
        }
        match end {
            PlaybackEnd::Natural => {
                self.queue.finished();
            }
            PlaybackEnd::Next => {
                self.queue.skip_next();
            }
            PlaybackEnd::Previous => {
                self.queue.previous();
            }
            PlaybackEnd::Stop => {
                self.queue.stop();
                return Ok(None);
            }
            PlaybackEnd::SeekRelative(_) => {}
        }
        self.preparing_advance = end == PlaybackEnd::Natural
            && self.queue.repeat() != Repeat::One
            && self.current().is_some();
        if let Some(seed) = self.queue.seed() {
            self.seed = seed;
        }
        Ok(self.current())
    }
}

fn report_smart(report: &aede_core::playback::shuffle::SmartShuffleReport) {
    if report.style_breaks > 0
        || report.unknown_transitions > 0
        || report.unknown_tracks > 0
        || report.limited_tracks > 0
        || report.omitted_genres > 0
    {
        eprintln!(
            "Smart shuffle: {} style breaks, {} transitions without genre evidence, {} tracks without genres, {} tracks with limited metadata, {} omitted genres; every selection entry is retained",
            report.style_breaks,
            report.unknown_transitions,
            report.unknown_tracks,
            report.limited_tracks,
            report.omitted_genres,
        );
    }
}

fn fresh_seed() -> u64 {
    let elapsed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    (elapsed.as_nanos() as u64) ^ u64::from(std::process::id()).rotate_left(32)
}

#[cfg(test)]
#[path = "play_order_tests.rs"]
mod tests;
