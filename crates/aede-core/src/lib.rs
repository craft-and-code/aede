#![warn(missing_docs)]
//! Aède — the heart of the music library.
//!
//! Read folders, extract a catalog of interlinked entities
//! from them, answer questions about it, keep what the user thinks of it, and
//! copy a selection of it out to a player or a card.
//!
//! The formats a library is made of are parsed here, from their
//! specifications; `lofty` covers the long tail
//! of containers that do not deserve a parser of their own.

pub mod accounts;
pub mod acoustic;
pub mod acoustid;
pub mod analysis;
pub mod artwork;
mod atomic_file;
pub mod audit;
pub mod backup;
pub mod clock;
pub mod conclusions;
pub mod contributors;
pub mod json;
pub mod lrclib;
pub mod lyrics;
pub mod tags;
pub mod text;

pub mod copy;
pub mod coverart;
pub mod credit_coverage;
pub mod discogs;
pub mod doctor;
pub mod fanarttv;
pub mod ffmpeg;
pub mod fingerprint;
pub mod graph;
#[cfg(feature = "fetch")]
pub mod http;
mod image;
pub mod model;
pub mod musicbrainz;
pub mod places;
pub mod playback;
pub mod playlist;
pub mod query;
pub mod scan;
pub mod sources;
pub mod spectrum;
pub mod stats;
pub mod store;
pub mod store_lock;
mod url;
pub mod user;
pub mod wikipedia;
