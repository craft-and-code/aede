//! The Cover Art Archive: the front image of a record, when it has one.
//!
//! Deliberately **no network**, like [`crate::musicbrainz`]: this module turns
//! an index somebody else fetched into an address to download, and decides
//! which image is the front one. Reaching the address is the client's job.
//!
//! # The index says what exists, so nothing here guesses
//!
//! Asking `coverartarchive.org` about a release answers with a small JSON
//! document — not the image — listing every image it holds:
//!
//! ```text
//! { "images": [ { "front": true, "types": ["Front"], "approved": true,
//!                 "image": "…/12345.jpg",
//!                 "thumbnails": { "250": "…", "500": "…", "1200": "…" } } ],
//!   "release": "https://musicbrainz.org/release/…" }
//! ```
//!
//! The sizes are **named in the answer**, so a caller asking for 1200 px is
//! told whether one exists rather than being sent to an address that might
//! `404`. That is why the index is fetched first: one small request decides
//! everything, including whether there is anything to download at all.

use crate::json::Json;

/// The name records from this service carry in `sources.json`.
pub const SOURCE: &str = "coverartarchive";

/// Base address of the service.
pub const WEB_SERVICE: &str = "https://coverartarchive.org";

/// Which image to take, of the ones the archive offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// The file as it was uploaded: often 1500–3000 px and several megabytes.
    Original,
    /// A thumbnail of this width, when the archive has generated one.
    Thumbnail(u32),
}

impl Size {
    /// Reads what a reader typed after `--size`.
    ///
    /// Only the widths the archive actually generates are accepted. A number it
    /// has never heard of would be asked for, not found, and silently fall back
    /// to something else — so it is refused here, where the message can list
    /// what there is.
    pub fn parse(text: &str) -> Option<Size> {
        match text.trim().to_ascii_lowercase().as_str() {
            "original" | "full" => Some(Size::Original),
            "250" => Some(Size::Thumbnail(250)),
            "500" => Some(Size::Thumbnail(500)),
            "1200" => Some(Size::Thumbnail(1200)),
            _ => None,
        }
    }

    /// How this size reads in a message.
    pub fn as_str(self) -> String {
        match self {
            Size::Original => "original".to_string(),
            Size::Thumbnail(px) => format!("{px} px"),
        }
    }
}

/// Where to ask what images a record has.
///
/// A **release group** rather than a release: the catalog's own identifier for
/// an album is the group's, and the archive answers a group by redirecting to
/// whichever release its editors chose to represent it — which is the right
/// answer for "the cover of this album" and saves picking a pressing here.
pub fn index_url(release_group: &str) -> String {
    format!("{WEB_SERVICE}/release-group/{release_group}")
}

/// Where to ask about one precise edition, when the tags name one.
///
/// Preferred over the group when it is available: the cover of the pressing on
/// the shelf is a better answer than the cover of the pressing somebody else
/// thought representative — a reissue often has different artwork.
pub fn release_index_url(release: &str) -> String {
    format!("{WEB_SERVICE}/release/{release}")
}

/// The front image of a record, at the size asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Front {
    /// Where to download it.
    pub url: String,
    /// The size actually served, which may not be the one asked for.
    pub size: Size,
}

/// Picks the front image out of an index, at the size asked for.
///
/// "Front" is decided by the `front` flag first and by the `types` list after:
/// both are filled by the archive's editors, and an image marked in one and not
/// the other is common enough that reading only one leaves records looking as
/// though they have no cover.
///
/// An **approved** image wins over one that is not, when there is a choice.
/// Anyone may upload; approval is an editor having looked. Where nothing is
/// approved the unapproved image is still taken, because a cover somebody
/// uploaded is a better answer than an empty folder.
///
/// The size falls back to the original when the thumbnail asked for is not in
/// the answer. The archive generates them for everything it holds today, but a
/// caller that got nothing back because a width was missing would look exactly
/// like a record with no artwork, which is a different thing entirely.
pub fn front(response: &Json, size: Size) -> Option<Front> {
    let images = response.get("images")?.as_arr()?;
    let is_front = |image: &&Json| -> bool {
        image.field_bool("front")
            || image
                .get("types")
                .and_then(Json::as_arr)
                .is_some_and(|types| {
                    types
                        .iter()
                        .filter_map(Json::as_str)
                        .any(|t| t.eq_ignore_ascii_case("front"))
                })
    };
    let fronts: Vec<&Json> = images.iter().filter(is_front).collect();
    let chosen = fronts
        .iter()
        .find(|image| image.field_bool("approved"))
        .or_else(|| fronts.first())?;

    let original = chosen.field_str("image");
    if let Size::Thumbnail(px) = size
        && let Some(url) = chosen
            .get("thumbnails")
            .and_then(|t| t.field_str(&px.to_string()))
        && !url.is_empty()
    {
        return Some(Front { url, size });
    }
    Some(Front {
        url: original.filter(|u| !u.is_empty())?,
        size: Size::Original,
    })
}

/// What image format these bytes are, if this program should write them.
///
/// **Sniffed, never taken from the address.** A download that went wrong — a
/// redirect to a login page, an error document, a truncated transfer — arrives
/// as bytes like any other, and writing those into a music folder under the
/// name `cover.jpg` is silent corruption that only surfaces months later when
/// something tries to display it. An extension read off a URL would not catch
/// any of it.
///
/// `None` means: do not write this.
pub fn image_kind(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        // JPEG: SOI marker, then the start of the first segment.
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some("png"),
        // The archive serves neither today, and accepting a format nothing in
        // this program can read would only move the failure somewhere quieter.
        _ => None,
    }
}

/// What a picture is of, for the few kinds worth telling apart.
///
/// The archive and the tag formats both classify their images, and the
/// classification is the only thing that makes a second image worth keeping: a
/// folder of `image-1.jpg` … `image-9.jpg` is not more useful than none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The front of the sleeve: the album cover.
    Front,
    /// The back of the sleeve, usually the track listing.
    Back,
    /// A page of the booklet or insert.
    Booklet,
    /// The disc itself — the label printed on a CD or a record.
    Media,
    /// A photograph of the artist.
    Artist,
    /// Anything else the source classifies, or nothing at all.
    Other,
}

impl Kind {
    /// How this program spells the kind, in a name and in a message.
    ///
    /// `cover` for the front, because that is the name every player and file
    /// manager looks for and this program's own scanner looks for first — see
    /// [`file_name`]. The rest are named for what they are.
    pub fn stem(self) -> &'static str {
        match self {
            Kind::Front => "cover",
            Kind::Back => "back",
            Kind::Booklet => "booklet",
            Kind::Media => "media",
            Kind::Artist => "artist",
            Kind::Other => "image",
        }
    }

    /// Reads the word a source uses for a kind.
    ///
    /// Unknown words become [`Kind::Other`] rather than being dropped: an image
    /// nobody classified is still an image, and losing it because the label was
    /// unfamiliar would be a strange way to fail.
    pub fn parse(text: &str) -> Kind {
        match text.trim().to_ascii_lowercase().as_str() {
            "front" => Kind::Front,
            "back" => Kind::Back,
            "booklet" => Kind::Booklet,
            "medium" | "media" => Kind::Media,
            "artist" => Kind::Artist,
            _ => Kind::Other,
        }
    }
}

/// What to call the file written beside the music.
///
/// `cover` is the first name [`crate::scan`] looks for, so the next scan finds
/// it without being told — the file is not registered anywhere, it is simply
/// discovered like one the user put there themselves.
pub fn file_name(kind: &str) -> String {
    format!("cover.{kind}")
}

/// What to call one image of a given kind, when several may share it.
///
/// `back.jpg` for the only back, `booklet-01.jpg` and `booklet-02.jpg` for the
/// pages of an insert. The number starts at one and is only added when there is
/// more than one of a kind, because `booklet-01.jpg` alone reads as a page torn
/// out of something.
pub fn image_name(kind: Kind, format: &str, index: usize, of: usize) -> String {
    match of {
        0 | 1 => format!("{}.{format}", kind.stem()),
        _ => format!("{}-{:02}.{format}", kind.stem(), index + 1),
    }
}

/// The subfolder everything that is not the cover is written into.
///
/// Not beside the music, and the reason is [`crate::scan::cover_in`]: the
/// scanner takes *any* image in a folder as a candidate cover when none is
/// named `cover`. A `back.jpg` sitting next to the tracks would therefore
/// become the album's artwork in this program and in a good many players —
/// which is precisely the wrong picture, and one nothing would ever look at
/// again. So the front stays beside the music, under the name everything looks
/// for, and the rest go one level down.
///
/// `artwork/` alongside the `spectrograms/` that `aede spectrum` already
/// writes: a folder named for what is in it.
pub const EXTRAS: &str = "artwork";

/// Where the images that are not the cover belong, for one album folder.
pub fn extras_in(folder: &std::path::Path) -> std::path::PathBuf {
    folder.join(EXTRAS)
}

/// Where each image sits among the others of its own kind.
///
/// Answers, for every image in the order given, `(index, of)` — its place
/// among the images of that same kind, and how many there are. That pair is
/// what [`image_name`] needs, and computing it here rather than in each caller
/// is what keeps a downloaded booklet and an extracted one named alike.
pub fn positions(kinds: &[Kind]) -> Vec<(usize, usize)> {
    let mut so_far: Vec<(Kind, usize)> = Vec::new();
    let mut out = Vec::with_capacity(kinds.len());
    for kind in kinds {
        let index = match so_far.iter_mut().find(|(k, _)| k == kind) {
            Some((_, seen)) => {
                *seen += 1;
                *seen - 1
            }
            None => {
                so_far.push((*kind, 1));
                0
            }
        };
        let of = kinds.iter().filter(|k| *k == kind).count();
        out.push((index, of));
    }
    out
}

/// Writes an image beside music, or says why it did not.
///
/// **The one place anything in this program puts a picture into somebody's
/// library**, whether it was downloaded from the archive or pulled out of their
/// own files. One function rather than two, because it carries two guards that
/// would otherwise be written twice and eventually differ:
///
/// - the bytes are **sniffed**, and anything that is not a JPEG or a PNG is
///   refused — see [`image_kind`];
/// - the target is checked **immediately before writing**, not when the caller
///   chose it. A catalog is a snapshot and a disk is not, and this is the one
///   moment in the program where being out of date destroys something the user
///   made.
///
/// Never overwrites. There is no flag to make it.
///
/// **And never adds a second cover to a folder that has one**, whatever it is
/// called. Checking only the name about to be written is not enough: an
/// extracted `cover.png` and a downloaded `cover.jpg` are two different names
/// and both rank as the album's cover, so a folder can end up holding two, with
/// no way to say which a player will show — and the one that wins may be the
/// 1200 px download rather than the full-size picture that was in the files.
/// [`crate::scan::cover_in`] asks the question the scanner itself asks, at the
/// moment of writing, which is the only moment that can answer it.
pub fn write_beside(folder: &std::path::Path, bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    if let Some(already) = crate::scan::cover_in(folder) {
        return Err(format!(
            "{} is already the cover of this folder",
            already.display()
        ));
    }
    match write_image(folder, Kind::Front, (0, 1), bytes)? {
        Written::New(path) => Ok(path),
        Written::Already(path) => Err(format!("{} already exists", path.display())),
    }
}

/// What became of one image.
///
/// Two outcomes rather than one, because a file that is already there is not a
/// failure and must not be counted or reported as one: running `--images` twice
/// over a library would otherwise print a page of errors for having done its
/// job the first time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    /// Written, at this path.
    New(std::path::PathBuf),
    /// A file of that name was already there, and nothing overwrites.
    Already(std::path::PathBuf),
}

/// Writes one image of a known kind, under the name that says what it is.
///
/// The general form of [`write_beside`], carrying the same two guards — the
/// bytes are sniffed, and nothing is ever written over. `where_` is the pair
/// [`positions`] gives for this image.
///
/// The folder is created if it is not there, because the images that are not
/// the cover go into a subfolder that will not exist the first time.
///
/// `Err` is for the two things that are actually wrong: bytes that are not an
/// image, and a write that failed.
pub fn write_image(
    folder: &std::path::Path,
    kind: Kind,
    where_: (usize, usize),
    bytes: &[u8],
) -> Result<Written, String> {
    let Some(format) = image_kind(bytes) else {
        return Err(format!(
            "what came back is not a JPEG or a PNG ({} bytes), so nothing was written",
            bytes.len()
        ));
    };
    let (index, of) = where_;
    if !folder.exists() {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    let path = folder.join(image_name(kind, format, index, of));
    if path.exists() {
        return Ok(Written::Already(path));
    }
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Written::New(path))
}

/// Every image an index holds, with what each one is of.
///
/// The front, when there is one, keeps its place as the album's cover; the rest
/// are what `--images` is for. Ordered front first and then as the archive
/// lists them, so a booklet's pages keep the order somebody scanned them in.
pub fn images(response: &Json, size: Size) -> Vec<(Kind, Front)> {
    let Some(rows) = response.get("images").and_then(Json::as_arr) else {
        return Vec::new();
    };
    let mut out: Vec<(Kind, Front)> = Vec::new();
    for row in rows {
        let kind = match row.field_bool("front") {
            true => Kind::Front,
            false => row
                .get("types")
                .and_then(Json::as_arr)
                .and_then(|types| types.iter().find_map(Json::as_str).map(Kind::parse))
                .unwrap_or(Kind::Other),
        };
        if let Some(found) = at_size(row, size) {
            out.push((kind, found));
        }
    }
    out.sort_by_key(|(kind, _)| u8::from(*kind != Kind::Front));
    out
}

/// One image's address at the size asked for, falling back to the original.
fn at_size(image: &Json, size: Size) -> Option<Front> {
    if let Size::Thumbnail(px) = size
        && let Some(url) = image
            .get("thumbnails")
            .and_then(|t| t.field_str(&px.to_string()))
        && !url.is_empty()
    {
        return Some(Front { url, size });
    }
    Some(Front {
        url: image.field_str("image").filter(|u| !u.is_empty())?,
        size: Size::Original,
    })
}

#[cfg(test)]
#[path = "coverart_tests.rs"]
mod tests;
