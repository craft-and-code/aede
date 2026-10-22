//! Shared metadata-free native FLAC view for verified playback.

pub(super) use aede_audio::decode::FlacAudioSource;

#[cfg(test)]
use std::fs::File;
#[cfg(test)]
use std::io::{Read, Seek, SeekFrom};
#[cfg(test)]
use symphonia::core::io::MediaSource;

#[cfg(test)]
const PREFIX_BYTES: usize = 42;
#[cfg(test)]
const MAX_METADATA_BLOCKS: usize = 65_536;

#[cfg(test)]
#[path = "flac_source_tests.rs"]
mod tests;
