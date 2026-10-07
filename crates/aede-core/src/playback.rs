//! In-memory playback order and transport state for a catalog selection.
//!
//! The queue holds track IDs, including repeated entries. It does not decode
//! audio, open files, or persist a session. Its seeded shuffle permutes the
//! entries once, so both the next and previous track remain knowable.

use crate::model::Id;

pub mod decoder;
pub mod exact_output;
pub mod exact_session;
pub mod format;
pub mod gain_plan;
pub mod loudness;
pub mod normalization;
pub mod output;
pub mod session;
pub mod shuffle;
pub mod stream;

/// What happens when the current track reaches its end.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Repeat {
    /// Stop after the final entry.
    #[default]
    Off,
    /// Restart the current entry after it finishes naturally.
    One,
    /// Continue from the beginning after the final entry.
    All,
}

/// Whether the transport is currently producing sound.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Transport {
    /// No audio is playing; the selected entry starts at its beginning.
    #[default]
    Stopped,
    /// Audio is playing.
    Playing,
    /// The selected entry retains its position but produces no audio.
    Paused,
}

/// A selection with a cursor and a deterministic playing order.
#[derive(Clone, Debug)]
pub struct Queue {
    tracks: Vec<Id>,
    order: Vec<usize>,
    previous_cycle: Option<(Vec<usize>, Option<u64>)>,
    next_cycle: Option<(Vec<usize>, Option<u64>)>,
    cursor: Option<usize>,
    seed: Option<u64>,
    repeat: Repeat,
    transport: Transport,
    position_ms: u64,
}

impl Queue {
    /// Build from the same ordered track IDs used by playlist rendering.
    ///
    /// `seed = None` keeps the selection order. A seed shuffles it with a
    /// fixed algorithm, independently of platform or process state.
    pub fn new(tracks: Vec<Id>, seed: Option<u64>) -> Self {
        let mut order: Vec<usize> = (0..tracks.len()).collect();
        if let Some(seed) = seed {
            shuffle(&mut order, seed);
        }
        Self {
            tracks,
            order,
            previous_cycle: None,
            next_cycle: None,
            cursor: None,
            seed,
            repeat: Repeat::Off,
            transport: Transport::Stopped,
            position_ms: 0,
        }
    }

    /// The original selection, including duplicate entries.
    pub fn selection(&self) -> &[Id] {
        &self.tracks
    }

    /// IDs in their current playing order.
    pub fn ordered_tracks(&self) -> impl Iterator<Item = Id> + '_ {
        self.order.iter().map(|&index| self.tracks[index])
    }

    /// The shuffle seed, if the queue is shuffled.
    pub fn seed(&self) -> Option<u64> {
        self.seed
    }

    /// The currently selected track.
    pub fn current(&self) -> Option<Id> {
        self.cursor.map(|cursor| self.tracks[self.order[cursor]])
    }

    /// The cursor in the current playing order.
    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    /// Original selection occurrence under the cursor, distinguishing repeats.
    pub fn current_entry(&self) -> Option<usize> {
        self.cursor.map(|cursor| self.order[cursor])
    }

    /// Select an existing occurrence after the driver abandons preparation of
    /// a later source. Missing occurrences leave the queue unchanged.
    pub fn select_entry(&mut self, entry: usize) -> bool {
        let Some(cursor) = self.order.iter().position(|&index| index == entry) else {
            return false;
        };
        self.cursor = Some(cursor);
        self.position_ms = 0;
        true
    }

    /// The current permutation of original selection occurrences.
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    /// Replace future order, retaining the played prefix and current occurrence.
    ///
    /// `order` must contain every original occurrence exactly once. Invalid
    /// input leaves transport, history, seed and position unchanged. Changing
    /// mode clears saved neighbouring cycles, but not Previous within this one.
    pub fn reorder(&mut self, order: Vec<usize>, seed: Option<u64>) -> Result<(), &'static str> {
        self.validate_order(&order)?;
        let prefix_len = self.cursor.map_or(0, |cursor| cursor + 1);
        let mut retained = vec![false; self.tracks.len()];
        for &entry in &self.order[..prefix_len] {
            retained[entry] = true;
        }
        self.order.truncate(prefix_len);
        self.order
            .extend(order.into_iter().filter(|&entry| !retained[entry]));
        self.seed = seed;
        self.previous_cycle = None;
        self.next_cycle = None;
        Ok(())
    }

    /// Supply an externally planned next repeat-all cycle.
    ///
    /// Return false when Previous has already retained the forward cycle.
    /// Such an order must not be silently replaced by a new shuffle draw.
    pub fn set_next_order(
        &mut self,
        order: Vec<usize>,
        seed: Option<u64>,
    ) -> Result<bool, &'static str> {
        self.validate_order(&order)?;
        if self.next_cycle.is_some() {
            return Ok(false);
        }
        self.next_cycle = Some((order, seed));
        Ok(true)
    }

    /// Reproducible seed for the next shuffled repeat-all cycle.
    pub fn next_seed(&self) -> Option<u64> {
        self.seed
            .map(|seed| seed.wrapping_add(0x9e37_79b9_7f4a_7c15))
    }

    fn validate_order(&self, order: &[usize]) -> Result<(), &'static str> {
        if order.len() != self.tracks.len() {
            return Err("playback order must contain every selection occurrence");
        }
        let mut seen = vec![false; self.tracks.len()];
        for &entry in order {
            let Some(present) = seen.get_mut(entry) else {
                return Err("playback order has an out-of-range occurrence");
            };
            if *present {
                return Err("playback order repeats a selection occurrence");
            }
            *present = true;
        }
        Ok(())
    }

    /// Current transport state.
    pub fn transport(&self) -> Transport {
        self.transport
    }

    /// Position in the current track, supplied by the playback driver.
    pub fn position_ms(&self) -> u64 {
        self.position_ms
    }

    /// Update the observed position after a successful playback or seek.
    /// An empty queue cannot acquire a position.
    pub fn set_position_ms(&mut self, position_ms: u64) -> bool {
        if self.cursor.is_none() {
            return false;
        }
        self.position_ms = position_ms;
        true
    }

    /// Change how natural ends are handled.
    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
    }

    /// The active repeat mode.
    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    /// Start or resume the selected track; select the first entry if needed.
    pub fn play(&mut self) -> Option<Id> {
        if self.cursor.is_none() && !self.order.is_empty() {
            self.cursor = Some(0);
        }
        let current = self.current()?;
        self.transport = Transport::Playing;
        Some(current)
    }

    /// Pause only while playing.
    pub fn pause(&mut self) {
        if self.transport == Transport::Playing {
            self.transport = Transport::Paused;
        }
    }

    /// Stop sound and return the selected track to its start.
    pub fn stop(&mut self) {
        self.transport = Transport::Stopped;
        self.position_ms = 0;
    }

    /// Skip to the next entry, even with repeat-one enabled.
    /// Keeps the current playing or paused state until the queue is exhausted.
    pub fn skip_next(&mut self) -> Option<Id> {
        self.advance(false)
    }

    /// Advance after natural end; repeat-one restarts the current entry.
    pub fn finished(&mut self) -> Option<Id> {
        if self.transport != Transport::Playing || self.cursor.is_none() {
            return None;
        }
        self.advance(true)
    }

    fn advance(&mut self, natural_end: bool) -> Option<Id> {
        if self.order.is_empty() {
            self.stop();
            return None;
        }
        if natural_end && self.repeat == Repeat::One && self.cursor.is_some() {
            self.position_ms = 0;
            return self.current();
        }
        let next = self.cursor.map_or(0, |cursor| cursor + 1);
        if next == self.order.len() {
            if self.repeat == Repeat::All {
                let (order, seed) = self.next_cycle.take().unwrap_or_else(|| {
                    let seed = self.next_seed();
                    let mut order: Vec<usize> = (0..self.tracks.len()).collect();
                    if let Some(seed) = seed {
                        shuffle(&mut order, seed);
                    }
                    (order, seed)
                });
                let old_order = std::mem::replace(&mut self.order, order);
                self.previous_cycle = Some((old_order, self.seed));
                self.seed = seed;
                self.cursor = Some(0);
            } else {
                self.cursor = None;
                self.stop();
                return None;
            }
        } else {
            self.cursor = Some(next);
        }
        self.position_ms = 0;
        self.current()
    }

    /// Restart the current track after three seconds; otherwise step back.
    /// At the beginning of the current playing order, restart its first entry.
    pub fn previous(&mut self) -> Option<Id> {
        let cursor = self.cursor?;
        if self.position_ms <= 3_000 {
            if cursor > 0 {
                self.cursor = Some(cursor - 1);
            } else if let Some((order, seed)) = self.previous_cycle.take() {
                let forward_order = std::mem::replace(&mut self.order, order);
                self.next_cycle = Some((forward_order, self.seed));
                self.seed = seed;
                self.cursor = Some(self.order.len() - 1);
            }
        }
        self.position_ms = 0;
        self.current()
    }
}

/// SplitMix64 has a fully specified bit pattern, unlike a platform RNG.
fn next_random(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut bits = *state;
    bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    bits ^ (bits >> 31)
}

fn shuffle(order: &mut [usize], seed: u64) {
    let mut state = seed;
    for upper in (2..=order.len()).rev() {
        let bound = upper as u64;
        let threshold = bound.wrapping_neg() % bound;
        let index = loop {
            let value = next_random(&mut state);
            if value >= threshold {
                break (value % bound) as usize;
            }
        };
        order.swap(upper - 1, index);
    }
}

#[cfg(test)]
#[path = "playback_tests.rs"]
mod tests;
