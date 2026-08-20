#![warn(missing_docs)]
//! Aède — the heart of the music library.
//!
//! Milestone M0: read folders, extract a catalog of interlinked entities from
//! them, and answer questions about it.
//!
//! The formats a library is made of are parsed here, from their
//! specifications; `lofty` is the single dependency, and covers the long tail
//! of containers that do not deserve a parser of their own.

pub mod analysis;
pub mod audit;
pub mod json;
pub mod tags;
pub mod text;

pub mod doctor;
pub mod model;
pub mod scan;
pub mod stats;
pub mod store;
