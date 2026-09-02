//! The picture inside an audio file, pulled out of it.
//!
//! # Why this goes through `lofty` for every format
//!
//! This crate parses FLAC, MP3, MP4 and Ogg itself, and reads the rest through
//! `lofty`. Those parsers exist because tag reading is the hot path — a scan
//! opens every file in the library — and because the fields they take are few
//! and stable.
//!
//! Extraction is neither of those things. It runs once per folder, on demand,
//! and what it produces is written into somebody's music library, where being
//! wrong is expensive and being slow is not. Four more hand-written extractors
//! — a FLAC `PICTURE` block, an ID3 `APIC` frame, an MP4 `covr` atom, a
//! base64-wrapped picture block inside an Ogg comment — would be four more
//! chances to write a corrupt file, to buy speed nobody is waiting for.
//!
//! So one path, for every format, through a dependency this crate already
//! carries. What it hands back is still sniffed before anything is written:
//! see [`crate::coverart::image_kind`].

use std::path::Path;

use crate::coverart::Kind;
use crate::tags::TagError;

/// Every picture a file carries, each with what it is of.
///
/// In the order the file holds them, which for a booklet is the order somebody
/// scanned its pages in, and worth keeping for that reason alone.
///
/// The kinds are the tag formats' own: ID3, Vorbis and MP4 all classify a
/// picture, out of a list far longer than this program has any use for. It is
/// narrowed to [`Kind`] here — a conductor, a lyricist and a brightly coloured
/// fish all being, for the purpose of putting a file in a music folder, simply
/// another image.
pub fn pictures(path: &Path) -> Result<Vec<(Kind, Vec<u8>)>, TagError> {
    use lofty::file::TaggedFileExt;

    let file = lofty::read_from_path(path).map_err(|_| TagError::UnrecognizedFormat)?;
    Ok(file
        .tags()
        .iter()
        .flat_map(|tag| tag.pictures())
        .map(|picture| (kind_of(picture.pic_type()), picture.data().to_vec()))
        .collect())
}

/// What a tag's own classification of a picture amounts to here.
fn kind_of(what: lofty::picture::PictureType) -> Kind {
    use lofty::picture::PictureType as P;
    match what {
        P::CoverFront => Kind::Front,
        P::CoverBack => Kind::Back,
        P::Leaflet => Kind::Booklet,
        P::Media => Kind::Media,
        // A photograph of whoever made the record, under whichever of the
        // three headings the tagger happened to choose.
        P::LeadArtist | P::Artist | P::Band => Kind::Artist,
        _ => Kind::Other,
    }
}

/// The front cover inside a file, if it has one.
///
/// **The front, not the first.** A file may carry several pictures — front,
/// back, the artist, a scan of the booklet — and a folder image that turned out
/// to be the back of the sleeve would be worse than none, because nothing later
/// would look for it again.
///
/// Falls back to the first picture only when nothing is typed as a front cover:
/// a great many files carry exactly one image and type it as nothing at all.
pub fn embedded(path: &Path) -> Result<Option<Vec<u8>>, TagError> {
    let pictures = pictures(path)?;
    let chosen = pictures
        .iter()
        .find(|(kind, _)| *kind == Kind::Front)
        .or_else(|| pictures.first());
    Ok(chosen.map(|(_, bytes)| bytes.clone()))
}

/// Takes the picture out of one file and writes it into a folder.
///
/// The whole of what `aede artwork` does to one album, in one place, so that
/// the end-to-end proof of it can live beside the extraction it depends on —
/// with real containers rather than a fixture agreeing with itself.
pub fn extract_into(source: &Path, folder: &Path) -> Result<std::path::PathBuf, String> {
    let bytes = embedded(source)
        .map_err(|why| why.to_string())?
        .ok_or_else(|| "the file carries no picture".to_string())?;
    crate::coverart::write_beside(folder, &bytes)
}

/// Writes out every picture a file carries that is **not** the front cover.
///
/// Into `folder/artwork/`, never beside the music: the reason is written on
/// [`crate::coverart::EXTRAS`], and it is not a matter of tidiness — an image
/// next to the tracks becomes the album's cover as far as the scanner is
/// concerned, and a back sleeve promoted to cover is worse than no cover.
///
/// Answers one line per picture, said or refused, because a run over a library
/// wants a count of both and the caller cannot tell them apart afterwards. A
/// picture already written is a refusal like any other: nothing overwrites.
pub fn extract_extras_into(source: &Path, folder: &Path) -> Result<Vec<Extracted>, TagError> {
    let all = pictures(source)?;
    let extras: Vec<(Kind, Vec<u8>)> = all
        .into_iter()
        .filter(|(kind, _)| *kind != Kind::Front)
        .collect();
    let kinds: Vec<Kind> = extras.iter().map(|(kind, _)| *kind).collect();
    let places = crate::coverart::positions(&kinds);
    let into = crate::coverart::extras_in(folder);
    Ok(extras
        .iter()
        .zip(places)
        .map(|((kind, bytes), where_)| Extracted {
            kind: *kind,
            wrote: crate::coverart::write_image(&into, *kind, where_, bytes),
        })
        .collect())
}

/// One picture out of a file, and where it went — or why it did not.
#[derive(Debug)]
pub struct Extracted {
    /// What the picture is of.
    pub kind: Kind,
    /// The file written, one already there, or the reason for neither.
    pub wrote: Result<crate::coverart::Written, String>,
}

#[cfg(test)]
#[path = "artwork_tests.rs"]
mod tests;
