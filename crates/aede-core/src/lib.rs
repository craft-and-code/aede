#![warn(missing_docs)]
//! Aède — the heart of the music library.
//!
//! Milestone M0.6: read folders, extract a catalog of interlinked entities
//! from them, answer questions about it, keep what the user thinks of it, and
//! copy a selection of it out to a player or a card.
//!
//! The formats a library is made of are parsed here, from their
//! specifications; `lofty` is the single dependency, and covers the long tail
//! of containers that do not deserve a parser of their own.

pub mod analysis;
pub mod audit;
pub mod clock;
pub mod json;
pub mod tags;
pub mod text;

pub mod copy;
pub mod doctor;
pub mod ffmpeg;
pub mod model;
pub mod playlist;
pub mod query;
pub mod scan;
pub mod spectrum;
pub mod stats;
pub mod store;
pub mod user;
