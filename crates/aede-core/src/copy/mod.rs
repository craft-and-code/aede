//! Copying a selection out of the library, keeping the tree it sits in.
//!
//! What this is for: filling a portable player, a card, an external drive —
//! somewhere that is not a library and will never be scanned. The copy is a
//! **derived artifact**, not a second catalog, and nothing here writes to the
//! catalog or to the files it reads.
//!
//! Two halves, and the split is the point. [`plan`] decides everything and
//! touches nothing: which files, where each one lands, what had to be renamed,
//! what could not be placed at all. Only then does the caller write. A run that
//! can say what it is about to do before doing it is a run that can be shown to
//! the user first — which is what `--dry-run` is — and, more importantly, one
//! whose decisions can be tested without a filesystem.
//!
//! The tree is kept **relative to the watched root that holds the file**, so a
//! track at `/Volumes/Music/Ozzy/1980 Blizzard/01.flac`, scanned under
//! `/Volumes/Music`, lands at `<destination>/Ozzy/1980 Blizzard/01.flac`. The
//! alternative — inventing a layout from the tags — is a different feature
//! (organising), and one this project has not decided it wants.

pub mod names;
pub mod transcode;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::model::{Catalog, Id};
use crate::text;

/// Which files beside the audio travel with it.
///
/// The ladder exists because "images" is the wrong question. A rip folder holds
/// the cover *and* the spectrograms *and* the scans of the booklet, all of them
/// PNG or JPEG, and a player wants exactly one of the three. The catalog
/// already knows which: the scan picked a cover by rank and stored it on the
/// release. [`Extras::Cover`] is therefore an exact answer where
/// [`Extras::Images`] can only be a heuristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Extras {
    /// Audio only. Cover art embedded in the tags still travels: it is inside
    /// the file.
    None,
    /// The one cover the catalog identified for the release.
    #[default]
    Cover,
    /// Every image in the folder, spectrograms and booklet scans included.
    Images,
    /// Everything sitting beside the audio: logs, cue sheets, reports.
    All,
}

impl Extras {
    /// The keyword as it is typed, or `None` for a word that names none.
    pub fn parse(word: &str) -> Option<Extras> {
        match text::normalize(word).as_str() {
            "none" => Some(Extras::None),
            "cover" | "covers" => Some(Extras::Cover),
            "image" | "images" => Some(Extras::Images),
            "all" => Some(Extras::All),
            _ => None,
        }
    }

    /// Every spelling accepted, for a message that offers what it refuses.
    pub const NAMES: &'static str = "none, cover, images, all";
}

/// Extensions counted as an image by [`Extras::Images`].
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff"];

/// What a file is doing in the plan, which is what lets a report say "12
/// albums and their covers" rather than a single undifferentiated count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ItemKind {
    /// A track that was selected.
    Audio,
    /// The cover art of a release holding a selected track.
    Cover,
    /// Anything else asked for by [`Extras::Images`] or [`Extras::All`].
    Other,
}

/// One file to write, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Where it is read from.
    pub source: PathBuf,
    /// Where it goes, relative to the destination folder.
    ///
    /// Relative rather than absolute so the plan can be compared, printed and
    /// tested without knowing which drive it is bound for.
    pub relative: PathBuf,
    /// Bytes, as the catalog last read them — or an **estimate** when this
    /// item is to be converted, since what an encoder produces is not known
    /// until it has produced it.
    pub size: u64,
    /// Why it is here.
    pub kind: ItemKind,
    /// What to encode it into, or `None` to copy the bytes as they are.
    pub convert: Option<transcode::Target>,
}

/// A name the destination could not have taken as it stood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    /// The path as it is in the library.
    pub from: PathBuf,
    /// The path as it will be written.
    pub to: PathBuf,
}

/// Everything a copy would do, decided before anything is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The files to write, in a stable order.
    pub items: Vec<Item>,
    /// Every component the destination filesystem forced a change to.
    pub renamed: Vec<Renamed>,
    /// Selected files sitting under no watched root, which therefore have no
    /// tree to keep. Reported rather than dropped, and rather than invented a
    /// place for.
    pub rootless: Vec<PathBuf>,
}

impl Plan {
    /// Bytes the copy would write.
    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|item| item.size).sum()
    }

    /// How many files of each kind, for a report that says what it is copying.
    pub fn counts(&self) -> BTreeMap<ItemKind, usize> {
        let mut out = BTreeMap::new();
        for item in &self.items {
            *out.entry(item.kind).or_insert(0) += 1;
        }
        out
    }
}

/// Everything the caller chose, gathered so that a plan reads as one decision
/// rather than as four positional booleans nobody can tell apart at the call
/// site.
#[derive(Debug, Clone, Copy, Default)]
pub struct Recipe {
    /// What travels beside the audio.
    pub extras: Extras,
    /// Whether the destination refuses the punctuation a music library is full
    /// of — see [`names::restricts_names`], which answers it by asking the
    /// volume rather than by guessing.
    pub restrict_names: bool,
    /// What to encode the audio into, or `None` to copy it unchanged.
    pub convert: Option<transcode::Target>,
    /// How hard the encoder should try.
    pub quality: Option<transcode::Quality>,
}

/// Whether a file should be encoded on the way out, and what into.
///
/// **One rule, and it falls out of what conversion is for.** A file is encoded
/// only when it is lossless *and* not already in the target format. Everything
/// else is copied as it stands, which covers three cases that would otherwise
/// each need their own argument:
///
/// - it is already an MP3 and MP3 was asked for — re-encoding would lose
///   quality to produce the same thing;
/// - it is an MP3 and Opus was asked for — a second lossy pass over a first one
///   is audible, and the file is already small, which was the point;
/// - it is an MP3 and FLAC was asked for — the result would be *larger* than
///   the source and no better: lossless in name, lossy in substance, which is
///   the one thing nobody rips on purpose. Producing it deliberately would be
///   absurd.
///
/// So a mixed library converted for a phone comes out with its lossless half
/// encoded and its lossy half untouched, which is what somebody filling a phone
/// wants and never has to ask for.
fn conversion_for(
    file: &crate::model::AudioFile,
    target: Option<transcode::Target>,
) -> Option<transcode::Target> {
    let target = target?;
    if !file.properties.lossless {
        return None;
    }
    let already = crate::text::file_name(&file.path)
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case(target.extension()));
    (!already).then_some(target)
}

/// Works out what copying these tracks would mean. Writes nothing, and reads
/// the disk only to list what sits beside the audio: every other answer comes
/// from the catalog.
pub fn plan(catalog: &Catalog, tracks: &[Id], recipe: &Recipe) -> Plan {
    let (extras, restrict_names) = (recipe.extras, recipe.restrict_names);
    let mut out = Plan::default();
    // Two tracks can share a file — the same path selected twice through two
    // routes — and a folder's extras are gathered once however many of its
    // tracks were picked.
    let mut sources: BTreeSet<&str> = BTreeSet::new();
    let mut folders: BTreeSet<&str> = BTreeSet::new();
    let mut releases: BTreeSet<Id> = BTreeSet::new();

    for &id in tracks {
        let Some(track) = catalog.track(id) else {
            continue;
        };
        let Some(file) = catalog.file(track.file_id) else {
            continue;
        };
        if sources.insert(file.path.as_str()) {
            folders.insert(text::folder(&file.path));
        }
        if let Some(release_id) = track.release_id {
            releases.insert(release_id);
        }
    }

    let mut wanted: Vec<(String, ItemKind)> = sources
        .iter()
        .map(|path| ((*path).to_string(), ItemKind::Audio))
        .collect();

    // The cover the catalog settled on, which is the whole reason `Cover` can
    // be exact where an extension filter cannot.
    if extras == Extras::Cover {
        for release_id in &releases {
            if let Some(cover) = catalog
                .release(*release_id)
                .and_then(|r| r.cover_path.as_deref())
            {
                wanted.push((cover.to_string(), ItemKind::Cover));
            }
        }
    }
    if matches!(extras, Extras::Images | Extras::All) {
        for folder in &folders {
            for path in beside(folder, extras) {
                wanted.push((path, ItemKind::Other));
            }
        }
    }

    // Sorted and deduplicated before any path is decided, so that the same
    // catalog and the same selection produce the same plan every time — the
    // renaming below depends on the order names are met in. `Audio` sorts
    // before `Cover` and `Other`, so a file wanted for two reasons is kept as
    // the more specific one.
    wanted.sort();
    wanted.dedup_by(|a, b| a.0 == b.0);

    let mut taken: BTreeSet<PathBuf> = BTreeSet::new();
    for (path, kind) in wanted {
        let Some(relative) = under_a_root(catalog, &path) else {
            out.rootless.push(PathBuf::from(path));
            continue;
        };
        let file = catalog.files.iter().find(|f| f.path == path);
        // Only the audio is ever encoded: a cover is a cover on any device.
        let convert = match kind {
            ItemKind::Audio => file.and_then(|f| conversion_for(f, recipe.convert)),
            _ => None,
        };
        // The extension changes **before** the name is placed, so that two
        // sources landing on one name — `01.flac` and `01.wav` both becoming
        // `01.mp3` — are seen as the collision they are rather than one file
        // written over the other.
        let renamed_relative = match convert {
            Some(target) => with_extension(&relative, target.extension()),
            None => relative.clone(),
        };
        let placed = place(&renamed_relative, restrict_names, &mut taken);
        if placed != Path::new(&relative) {
            out.renamed.push(Renamed {
                from: PathBuf::from(&relative),
                to: placed.clone(),
            });
        }
        let size = match convert {
            Some(target) => transcode::estimated_size(
                target,
                recipe.quality,
                file.and_then(|f| f.properties.duration_ms).unwrap_or(0),
                file.map(|f| f.size).unwrap_or(0),
            ),
            None => size_of(catalog, &path),
        };
        out.items.push(Item {
            size,
            source: PathBuf::from(path),
            relative: placed,
            kind,
            convert,
        });
    }
    out
}

/// The same relative path with another extension on its last component.
///
/// Written out rather than `Path::set_extension`, which takes everything after
/// the **first** dot of the file name to be an extension on some platforms and
/// would turn `Vol. 2 - Live.flac` into `Vol.mp3`.
fn with_extension(relative: &str, extension: &str) -> String {
    let (folder, name) = match relative.rfind('/') {
        Some(slash) => (&relative[..=slash], &relative[slash + 1..]),
        None => ("", relative),
    };
    let stem = match name.rfind('.') {
        Some(dot) if dot > 0 => &name[..dot],
        _ => name,
    };
    format!("{folder}{stem}.{extension}")
}

/// How many files the plan would encode rather than copy.
impl Plan {
    /// Items that go through an encoder, and those that are copied as they are.
    ///
    /// Reported separately because the difference is the one thing a user needs
    /// to see before a conversion starts: a library that is half MP3 already
    /// comes out half untouched, and a count that hid that would look like the
    /// conversion had silently skipped things.
    pub fn converted(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.convert.is_some())
            .count()
    }

    /// `true` when any size in the plan is an encoder's output, and therefore
    /// a guess rather than a measurement.
    pub fn size_is_estimated(&self) -> bool {
        self.converted() > 0
    }
}

/// The files sitting beside the audio that this level of `extras` asks for.
///
/// The one place in this module that reads the disk, and it has to: the catalog
/// holds what the scan recognised as audio, and everything asked for here is by
/// definition what it did not. Sorted, so two runs agree.
///
/// An unreadable folder yields nothing rather than stopping the plan — the
/// audio is what was asked for, and the extras are extra.
fn beside(folder: &str, extras: Extras) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        // A dotfile beside an album is the operating system's business —
        // `.DS_Store`, thumbnail caches — and never something to carry onto a
        // player.
        if name.starts_with('.') {
            continue;
        }
        // Audio is already in the plan through the catalog, with its size and
        // its kind; picking it up again here would put it in twice under a
        // vaguer name.
        if crate::tags::is_audio_path(&path) {
            continue;
        }
        if extras == Extras::Images && !is_image(&name) {
            continue;
        }
        found.push(path.to_string_lossy().to_string());
    }
    found.sort();
    found
}

/// `true` when a file name ends in an extension pictures use.
fn is_image(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        let extension = extension.to_ascii_lowercase();
        IMAGE_EXTENSIONS.contains(&extension.as_str())
    })
}

/// Bytes the catalog last read for this path, or zero for a file it does not
/// hold — a cover is not an audio file and has no entry.
fn size_of(catalog: &Catalog, path: &str) -> u64 {
    catalog
        .files
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.size)
        .unwrap_or(0)
}

/// The path of a file relative to the watched root that holds it.
///
/// `None` when no root does, which is not a fault to hide: the file has no tree
/// to keep, and putting it at the destination's top level would silently mix it
/// in with the folders that do.
fn under_a_root(catalog: &Catalog, path: &str) -> Option<String> {
    // The longest matching root wins. Nesting one watched folder inside another
    // is legal, and taking the shorter one would carry the intermediate
    // folders into a destination the user asked to be rid of.
    catalog
        .roots
        .iter()
        .filter(|root| text::is_under(path, root))
        .max_by_key(|root| root.len())
        .and_then(|root| {
            let rest = path.strip_prefix(root.as_str())?;
            let rest = rest.trim_start_matches('/');
            (!rest.is_empty()).then(|| rest.to_string())
        })
}

/// Turns a relative path into one the destination will accept, component by
/// component, keeping it distinct from everything already placed.
fn place(relative: &str, restrict_names: bool, taken: &mut BTreeSet<PathBuf>) -> PathBuf {
    let parts: Vec<&str> = relative.split('/').filter(|p| !p.is_empty()).collect();
    let adapt = |part: &str| match restrict_names {
        true => names::adapt(part).unwrap_or_else(|| part.to_string()),
        false => part.to_string(),
    };
    let Some((file, folders)) = parts.split_last() else {
        return PathBuf::new();
    };

    // Folders are *meant* to be met again — every other track of the album
    // lands in the same one — so only the file name is forced unique, and only
    // against the names already placed in that same folder. Uniqueness across
    // the whole tree would rename half a library, where two albums legitimately
    // both hold an `01 Intro.flac`.
    let directory: PathBuf = folders.iter().map(|part| adapt(part)).collect();
    let mut here: BTreeSet<String> = taken
        .iter()
        .filter(|path| path.parent() == Some(directory.as_path()))
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .collect();

    let placed = directory.join(names::make_unique(&adapt(file), &mut here));
    taken.insert(placed.clone());
    placed
}

/// `true` when a destination sits inside a folder the catalog watches.
///
/// Copying a library into itself is not a mistake to warn about afterwards: the
/// next scan reads the copies as new files, the catalog doubles, and `doctor`
/// reports every album as a duplicate of itself. Refused before anything is
/// written.
pub fn inside_a_watched_root(catalog: &Catalog, destination: &Path) -> Option<String> {
    let destination = destination.to_string_lossy().to_string();
    catalog
        .roots
        .iter()
        .find(|root| text::is_under(&destination, root) || text::is_under(root, &destination))
        .cloned()
}

/// The name a file is written under before it is moved into place.
///
/// **The extension is kept**, and not as a nicety: ffmpeg chooses its muxer
/// from it, so a file written as `01 Crazy Train.aede-partial` made it answer
/// "Invalid argument" and the whole conversion failed on every track. The
/// marker therefore goes *before* the extension rather than replacing it.
///
/// One helper for the copy and the conversion alike, so the two cannot disagree
/// about what a half-written file is called — which is what the resume test
/// depends on.
pub fn partial_path(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name.as_str(), ""),
    };
    destination.with_file_name(format!("{stem}.aede-partial{extension}"))
}

/// What became of one file the plan named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrote {
    /// Written, and verified if verification was asked for.
    Copied,
    /// Already there, the same size, and left alone — which is what makes an
    /// interrupted run cheap to finish.
    Skipped,
}

/// Why one file did not make it.
#[derive(Debug, Clone)]
pub struct Failed {
    /// The file that was being written.
    pub source: PathBuf,
    /// What went wrong, worded for the user.
    pub reason: String,
}

/// Copies one file, creating the folders above it.
///
/// `verify` re-reads what was written and compares a checksum against what was
/// read. Two honest limits, both worth knowing rather than glossing over:
///
/// - the file is flushed to the device before being read back, but a read can
///   still be served from the kernel's cache, so this proves the bytes made it
///   through the program and the filesystem rather than onto the platter. It
///   catches a truncated write, a full disk and a bad cable; it is not a
///   substitute for reading the card back on another machine;
/// - the checksum is a CRC-32, which detects accidental corruption and is not
///   meant to resist anybody deliberately producing a collision. Nothing here
///   is a security boundary — the question is whether a transfer went wrong.
pub fn copy_one(
    source: &Path,
    destination: &Path,
    expected_size: u64,
    verify: bool,
    replace: bool,
) -> Result<Wrote, String> {
    use std::io::Read;

    // Already there and the right length: the cheap half of verification, and
    // it runs on every file whether `--verify` was asked for or not. It costs
    // one metadata read and catches the failure that actually happens — a run
    // interrupted mid-file, or a disk that filled up.
    if !replace
        && let Ok(existing) = std::fs::metadata(destination)
        && existing.is_file()
        && existing.len() == expected_size
        && expected_size > 0
    {
        return Ok(Wrote::Skipped);
    }

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }

    // Written under a temporary name and moved into place, so that an
    // interrupted run never leaves a half-file wearing the name of a whole
    // one — the next run would see the right name, and only a wrong length
    // would give it away.
    let partial = partial_path(destination);
    let written = std::fs::copy(source, &partial)
        .map_err(|e| format!("{} → {}: {e}", source.display(), destination.display()))?;

    let checked = verify
        .then(|| {
            // Flushed before reading back: without this the comparison is
            // between two views of the same buffer and proves nothing at all.
            let file = std::fs::File::open(&partial).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            let read = |path: &Path| -> Result<u32, String> {
                let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                let mut buffer = vec![0u8; 1 << 20];
                let mut all = Vec::new();
                loop {
                    let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    all.extend_from_slice(&buffer[..n]);
                }
                Ok(crate::audit::crc::crc32_ogg(&all))
            };
            Ok::<(u32, u32), String>((read(source)?, read(&partial)?))
        })
        .transpose()
        .map_err(|e| format!("{}: {e}", destination.display()))?;

    if let Some((from, to)) = checked
        && from != to
    {
        let _ = std::fs::remove_file(&partial);
        return Err(format!(
            "{}: what was written back does not match what was read",
            destination.display()
        ));
    }
    if written != expected_size && expected_size > 0 {
        let _ = std::fs::remove_file(&partial);
        return Err(format!(
            "{}: {written} bytes written where {expected_size} were expected",
            destination.display()
        ));
    }

    std::fs::rename(&partial, destination)
        .map_err(|e| format!("{}: {e}", destination.display()))?;
    Ok(Wrote::Copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{self, ScannedFile};
    use crate::tags::RawTags;

    fn file(path: &str, album: &str) -> ScannedFile {
        let mut tags = RawTags::default();
        for (key, value) in [
            ("title", "T"),
            ("artist", "A"),
            ("albumartist", "A"),
            ("album", album),
            ("date", "1990"),
        ] {
            tags.insert(key, value);
        }
        tags.properties.codec = "flac".into();
        tags.properties.duration_ms = Some(1000);
        ScannedFile {
            path: path.into(),
            size: 1000,
            mtime: 0,
            tags,
            folder_cover: None,
            integrity: None,
        }
    }

    fn catalog_of(paths: &[(&str, &str)], roots: &[&str]) -> Catalog {
        model::build(
            paths.iter().map(|(p, a)| file(p, a)).collect(),
            roots.iter().map(|r| (*r).to_string()).collect(),
            0,
        )
    }

    fn all_tracks(catalog: &Catalog) -> Vec<Id> {
        catalog.tracks.iter().map(|t| t.id).collect()
    }

    #[test]
    fn the_tree_is_kept_relative_to_the_root_that_holds_it() {
        // The whole promise of the command: what sat under the watched folder
        // arrives under the destination in the same shape.
        let catalog = catalog_of(
            &[
                ("/m/Ozzy/1980 Blizzard/01.flac", "Blizzard"),
                ("/m/Danzig/1994 Danzig 4/02.flac", "Danzig 4"),
            ],
            &["/m"],
        );
        let plan = plan(
            &catalog,
            &all_tracks(&catalog),
            &Recipe {
                extras: Extras::None,
                ..Default::default()
            },
        );
        let places: Vec<String> = plan
            .items
            .iter()
            .map(|i| i.relative.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            places,
            vec![
                "Danzig/1994 Danzig 4/02.flac".to_string(),
                "Ozzy/1980 Blizzard/01.flac".to_string(),
            ]
        );
        assert_eq!(plan.total_bytes(), 2000);
        assert!(plan.renamed.is_empty(), "nothing needed adapting");
    }

    #[test]
    fn the_deepest_watched_root_decides_the_tree() {
        // Watching a folder and something inside it is legal. Taking the
        // shorter root would carry the intermediate folders into a destination
        // the user asked precisely to be rid of.
        let catalog = catalog_of(&[("/m/live/set/01.flac", "Set")], &["/m", "/m/live"]);
        let plan = plan(
            &catalog,
            &all_tracks(&catalog),
            &Recipe {
                extras: Extras::None,
                ..Default::default()
            },
        );
        assert_eq!(plan.items[0].relative, Path::new("set/01.flac"));
    }

    #[test]
    fn a_file_under_no_root_is_reported_and_not_invented_a_place_for() {
        // It has no tree to keep. Dropping it in at the top would silently mix
        // it in with the folders that do have one.
        let mut catalog = catalog_of(&[("/m/a/01.flac", "A")], &["/m"]);
        catalog.roots = vec!["/elsewhere".into()];
        let plan = plan(
            &catalog,
            &all_tracks(&catalog),
            &Recipe {
                extras: Extras::None,
                ..Default::default()
            },
        );
        assert!(plan.items.is_empty());
        assert_eq!(plan.rootless, vec![PathBuf::from("/m/a/01.flac")]);
    }

    #[test]
    fn one_file_reached_by_two_tracks_is_copied_once() {
        let catalog = catalog_of(&[("/m/a/01.flac", "A")], &["/m"]);
        let id = catalog.tracks[0].id;
        let plan = plan(
            &catalog,
            &[id, id, id],
            &Recipe {
                extras: Extras::None,
                ..Default::default()
            },
        );
        assert_eq!(plan.items.len(), 1);
    }

    #[test]
    fn a_restricted_destination_renames_and_says_so() {
        // The punctuation a music library is full of and a card refuses. Every
        // change is reported: a copy whose names quietly differ from the
        // library cannot be compared against it afterwards.
        let catalog = catalog_of(
            &[(
                "/m/Pixies/Surfer Rosa/Where Is My Mind?.flac",
                "Surfer Rosa",
            )],
            &["/m"],
        );
        let plan = plan(
            &catalog,
            &all_tracks(&catalog),
            &Recipe {
                extras: Extras::None,
                restrict_names: true,
                ..Default::default()
            },
        );
        assert_eq!(
            plan.items[0].relative,
            Path::new("Pixies/Surfer Rosa/Where Is My Mind_.flac")
        );
        assert_eq!(plan.renamed.len(), 1, "and it is reported");

        // Left alone when the destination takes them.
        let unrestricted = super::plan(
            &catalog,
            &all_tracks(&catalog),
            &Recipe {
                extras: Extras::None,
                ..Default::default()
            },
        );
        assert_eq!(
            unrestricted.items[0].relative,
            Path::new("Pixies/Surfer Rosa/Where Is My Mind?.flac")
        );
        assert!(unrestricted.renamed.is_empty());
    }

    #[test]
    fn two_names_that_adapt_to_one_do_not_become_one_file() {
        // "1: Live" and "1? Live" both become "1_ Live". Writing the second
        // over the first would lose a track and report a complete copy.
        let catalog = catalog_of(
            &[("/m/a/1: Live.flac", "A"), ("/m/a/1? Live.flac", "A")],
            &["/m"],
        );
        let plan = plan(
            &catalog,
            &all_tracks(&catalog),
            &Recipe {
                extras: Extras::None,
                restrict_names: true,
                ..Default::default()
            },
        );
        let places: BTreeSet<PathBuf> = plan.items.iter().map(|i| i.relative.clone()).collect();
        assert_eq!(places.len(), 2, "two files, two destinations: {places:?}");
    }

    #[test]
    fn the_same_file_name_in_two_albums_is_not_renamed() {
        // Uniqueness is per folder. Forcing it across the tree would rename
        // half a library, where every album legitimately opens with an `01`.
        let catalog = catalog_of(&[("/m/a/01.flac", "A"), ("/m/b/01.flac", "B")], &["/m"]);
        let plan = plan(
            &catalog,
            &all_tracks(&catalog),
            &Recipe {
                extras: Extras::None,
                restrict_names: true,
                ..Default::default()
            },
        );
        assert!(plan.renamed.is_empty(), "{:?}", plan.renamed);
        assert_eq!(plan.items.len(), 2);
    }

    #[test]
    fn a_destination_inside_a_watched_root_is_refused() {
        // The next scan would read the copies as new files, the catalog would
        // double, and doctor would report every album as its own duplicate.
        let catalog = catalog_of(&[("/m/a/01.flac", "A")], &["/m"]);
        assert_eq!(
            inside_a_watched_root(&catalog, Path::new("/m/backup")).as_deref(),
            Some("/m")
        );
        // And the other way round: a destination that would swallow the root.
        assert_eq!(
            inside_a_watched_root(&catalog, Path::new("/")).as_deref(),
            Some("/m")
        );
        assert_eq!(
            inside_a_watched_root(&catalog, Path::new("/Volumes/USB")),
            None
        );
    }

    /// A catalog holding one file of a given codec, lossless or not.
    fn catalog_of_codec(path: &str, codec: &str, lossless: bool) -> Catalog {
        let mut f = file(path, "A");
        f.tags.properties.codec = codec.into();
        f.tags.properties.lossless = lossless;
        f.tags.properties.duration_ms = Some(240_000);
        model::build(vec![f], vec!["/m".into()], 0)
    }

    fn converted_to(catalog: &Catalog, target: transcode::Target) -> Option<transcode::Target> {
        let plan = plan(
            catalog,
            &all_tracks(catalog),
            &Recipe {
                extras: Extras::None,
                convert: Some(target),
                ..Default::default()
            },
        );
        plan.items[0].convert
    }

    #[test]
    fn only_a_lossless_source_is_ever_encoded() {
        use transcode::Target;
        // The rule, and the three cases it settles at once.
        let flac = catalog_of_codec("/m/a/01.flac", "flac", true);
        assert_eq!(converted_to(&flac, Target::Mp3), Some(Target::Mp3));

        // Already lossy, lossy asked for: a second pass over a first one is
        // audible, and the file was already small, which was the point.
        let mp3 = catalog_of_codec("/m/a/01.mp3", "mp3", false);
        assert_eq!(
            converted_to(&mp3, Target::Opus),
            None,
            "copied as it stands"
        );

        // Already lossy, lossless asked for: the result would be larger than
        // the source and no better — lossless in name only. Producing it
        // deliberately would be absurd.
        assert_eq!(converted_to(&mp3, Target::Flac), None);

        // Already in the target format: nothing to do.
        let already = catalog_of_codec("/m/a/01.mp3", "mp3", false);
        assert_eq!(converted_to(&already, Target::Mp3), None);
        let flac_to_flac = catalog_of_codec("/m/a/01.flac", "flac", true);
        assert_eq!(converted_to(&flac_to_flac, Target::Flac), None);

        // Lossless to lossless, different format: the reason to convert a WAV
        // rip at all.
        let wav = catalog_of_codec("/m/a/01.wav", "pcm", true);
        assert_eq!(converted_to(&wav, Target::Flac), Some(Target::Flac));
    }

    #[test]
    fn a_converted_file_carries_the_new_extension() {
        use transcode::Target;
        let catalog = catalog_of_codec("/m/a/01 Crazy Train.flac", "flac", true);
        let plan = plan(
            &catalog,
            &all_tracks(&catalog),
            &Recipe {
                extras: Extras::None,
                convert: Some(Target::Mp3),
                ..Default::default()
            },
        );
        assert_eq!(plan.items[0].relative, Path::new("a/01 Crazy Train.mp3"));
        assert_eq!(plan.converted(), 1);
        assert!(plan.size_is_estimated(), "an encoder's output is a guess");
    }

    #[test]
    fn two_sources_landing_on_one_name_do_not_become_one_file() {
        use transcode::Target;
        // `01.flac` and `01.wav` both want to be `01.mp3`. Writing the second
        // over the first would lose a track and report a complete copy — which
        // is why the extension changes before the name is placed.
        let mut a = file("/m/a/01.flac", "A");
        a.tags.properties.lossless = true;
        a.tags.properties.duration_ms = Some(1000);
        let mut b = file("/m/a/01.wav", "A");
        b.tags.properties.codec = "pcm".into();
        b.tags.properties.lossless = true;
        b.tags.properties.duration_ms = Some(1000);
        let catalog = model::build(vec![a, b], vec!["/m".into()], 0);
        let plan = plan(
            &catalog,
            &all_tracks(&catalog),
            &Recipe {
                extras: Extras::None,
                convert: Some(Target::Mp3),
                ..Default::default()
            },
        );
        let places: BTreeSet<PathBuf> = plan.items.iter().map(|i| i.relative.clone()).collect();
        assert_eq!(places.len(), 2, "two sources, two destinations: {places:?}");
    }

    #[test]
    fn a_title_whose_own_dot_is_not_an_extension_keeps_it() {
        use transcode::Target;
        // `Path::set_extension` takes everything after the first dot on some
        // platforms and would turn this into `Vol.mp3`.
        assert_eq!(
            with_extension("a/Vol. 2 - Live.flac", "mp3"),
            "a/Vol. 2 - Live.mp3"
        );
        assert_eq!(with_extension("no folder.flac", "opus"), "no folder.opus");
        assert_eq!(
            with_extension("a/no extension", "mp3"),
            "a/no extension.mp3"
        );
        let _ = Target::Mp3;
    }

    #[test]
    fn a_word_that_names_no_extras_is_refused_rather_than_guessed_at() {
        assert_eq!(Extras::parse("cover"), Some(Extras::Cover));
        assert_eq!(Extras::parse("COVERS"), Some(Extras::Cover));
        assert_eq!(Extras::parse("none"), Some(Extras::None));
        assert_eq!(Extras::parse("everything"), None);
    }
}
