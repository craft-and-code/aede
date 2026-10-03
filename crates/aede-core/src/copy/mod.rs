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
mod output;
mod playlists;
pub mod transcode;
pub use output::{
    TemporaryOutput, files_match, validate_destination, validate_filesystem_names,
    validate_new_publication, validate_source,
};

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
    /// UTF-8 derivative text to write instead of copying source bytes.
    /// Set only when destination playlists have explicitly been requested.
    pub contents: Option<String>,
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
    /// Sources or requested companion folders that cannot be read or converted
    /// as requested. The caller must refuse these before writing.
    pub rejected: Vec<Failed>,
    /// Audio files being encoded whose embedded cover cannot follow them into
    /// the target format, because ffmpeg refuses to mux a picture stream into
    /// it — see [`transcode::Target::keeps_embedded_art`]. Counted rather
    /// than named: the loss is the same whichever file it happens to.
    pub covers_the_target_cannot_hold: usize,
    /// Audio files being encoded into a target whose tag format has no room
    /// for one of the tags they carry — see
    /// [`transcode::Target::tags_it_would_drop`]. In practice this is `wav`
    /// and its fixed legacy vocabulary; every other target takes an
    /// arbitrary key and never adds to this count.
    pub tags_the_target_cannot_hold: usize,
}

impl Plan {
    /// Bytes the copy would write.
    pub fn total_bytes(&self) -> u64 {
        self.items
            .iter()
            .fold(0, |total, item| total.saturating_add(item.size))
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
    // A container extension does not identify its codec: lossless ALAC in
    // M4A must still be encoded when AAC is requested. Container aliases such
    // as .wave already hold WAV and can be copied without re-encoding.
    let already = match target {
        transcode::Target::Flac => {
            file.properties.codec.eq_ignore_ascii_case("flac")
                && (file.properties.container.eq_ignore_ascii_case("flac")
                    || (file.properties.container.is_empty()
                        && Path::new(&file.path)
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))))
        }
        transcode::Target::Wav => file.properties.container.eq_ignore_ascii_case("wav"),
        transcode::Target::Mp3 => file.properties.codec.eq_ignore_ascii_case("mp3"),
        transcode::Target::Opus => file.properties.codec.eq_ignore_ascii_case("opus"),
        transcode::Target::Aac => file.properties.codec.eq_ignore_ascii_case("aac"),
        transcode::Target::Vorbis => file.properties.codec.eq_ignore_ascii_case("vorbis"),
    };
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
            match beside(folder, extras) {
                Ok(paths) => wanted.extend(paths.into_iter().map(|path| (path, ItemKind::Other))),
                Err(failure) => out.rejected.push(failure),
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

    let files: BTreeMap<&str, &crate::model::AudioFile> = catalog
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut placement = Placement::default();
    for (path, kind) in wanted {
        let Some(relative) = under_a_root(catalog, &path) else {
            out.rootless.push(PathBuf::from(path));
            continue;
        };
        let file = files.get(path.as_str()).copied();
        // Only the audio is ever encoded: a cover is a cover on any device.
        let convert = match kind {
            ItemKind::Audio => file.and_then(|f| conversion_for(f, recipe.convert)),
            _ => None,
        };
        // Said once, before anything is written, rather than left for a
        // player to discover: an encode that quietly drops a cover or a tag
        // is a wrong answer standing in for a missing one.
        if let (Some(target), Some(f)) = (convert, file) {
            if let Err(reason) = transcode::validate_conversion(&f.properties, target) {
                out.rejected.push(Failed {
                    source: PathBuf::from(&path),
                    reason,
                });
                continue;
            }
            if !target.keeps_embedded_art() && f.has_embedded_art {
                out.covers_the_target_cannot_hold += 1;
            }
            if !target.tags_it_would_drop(&f.tags).is_empty() {
                out.tags_the_target_cannot_hold += 1;
            }
        }
        // The extension changes **before** the name is placed, so that two
        // sources landing on one name — `01.flac` and `01.wav` both becoming
        // `01.mp3` — are seen as the collision they are rather than one file
        // written over the other.
        let renamed_relative = match convert {
            Some(target) => with_extension(&relative, target.extension()),
            None => relative.clone(),
        };
        let placed = placement.place(&renamed_relative, &path, restrict_names);
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
            None => file
                .map(|f| f.size)
                .unwrap_or_else(|| std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0)),
        };
        out.items.push(Item {
            size,
            source: PathBuf::from(path),
            relative: placed,
            kind,
            convert,
            contents: None,
        });
    }
    out
}

/// The same relative path with another extension on its last component.
fn with_extension(relative: &str, extension: &str) -> String {
    let mut path = PathBuf::from(relative);
    path.set_extension(extension);
    path.to_string_lossy().into_owned()
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
/// An unreadable requested folder is an error: reporting a complete copy while
/// silently omitting selected companions would not honour `extras`.
fn beside(folder: &str, extras: Extras) -> Result<Vec<String>, Failed> {
    let folder = Path::new(folder);
    let mut found: Vec<String> = Vec::new();
    let folders = add_files_of(folder, extras, &mut found)?;
    // Aède writes what it draws or keeps beside a track into a subfolder of
    // its own rather than into the album folder directly — `spectrograms/`
    // today, and both `Extras::Images` and `Extras::All` are documented to
    // reach a spectrogram, not just the two of them that happen to be
    // sitting loose next to the audio. One level down and no further: a
    // folder inside `spectrograms/` would not be Aède's, and guessing at its
    // shape is not this function's business.
    for folder in folders {
        add_files_of(&folder, extras, &mut found)?;
    }
    found.sort();
    Ok(found)
}

/// Every plain file directly inside `folder` that this level of `extras`
/// wants, appended as an absolute path.
///
/// Shared between the album folder itself and each subfolder [`beside`] steps
/// into, so a dotfile or an audio file is refused the same way wherever it is
/// found.
fn add_files_of(
    folder: &Path,
    extras: Extras,
    found: &mut Vec<String>,
) -> Result<Vec<PathBuf>, Failed> {
    let entries = std::fs::read_dir(folder).map_err(|error| extra_error(folder, error))?;
    let mut folders = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| extra_error(folder, error))?;
        let kind = entry
            .file_type()
            .map_err(|error| extra_error(&entry.path(), error))?;
        if kind.is_dir() {
            folders.push(entry.path());
            continue;
        }
        if !kind.is_file() {
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
    folders.sort();
    Ok(folders)
}

fn extra_error(path: &Path, error: std::io::Error) -> Failed {
    Failed {
        source: path.to_path_buf(),
        reason: format!("cannot read requested companions: {error}"),
    }
}

/// `true` when a file name ends in an extension pictures use.
fn is_image(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        let extension = extension.to_ascii_lowercase();
        IMAGE_EXTENSIONS.contains(&extension.as_str())
    })
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
        .and_then(|root| text::relative_under(path, root).filter(|rest| !rest.is_empty()))
}

/// Original directories have stable placements; each output parent has its
/// own namespace shared by files and directories. Conservative matching keys make
/// the plan portable without allowing a case variant to silently merge albums.
#[derive(Default)]
struct Placement {
    directories: BTreeMap<String, PathBuf>,
    names: BTreeMap<PathBuf, BTreeSet<String>>,
}

impl Placement {
    fn place(&mut self, relative: &str, source: &str, restrict: bool) -> PathBuf {
        let parts: Vec<&str> = relative
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        let Some((file, folders)) = parts.split_last() else {
            return PathBuf::new();
        };
        let adapt = |part: &str| {
            if restrict {
                names::adapt(part).unwrap_or_else(|| part.to_string())
            } else {
                part.to_string()
            }
        };
        let mut source_root = text::folder(source);
        for _ in folders {
            source_root = text::folder(source_root);
        }
        let mut original = format!("{source_root}/");
        let mut parent = PathBuf::new();
        for folder in folders {
            original.push_str(folder);
            original.push('/');
            parent = match self.directories.get(&original) {
                Some(held) => held.clone(),
                None => {
                    let name = names::make_unique_portable(
                        &adapt(folder),
                        self.names.entry(parent.clone()).or_default(),
                    );
                    let placed = parent.join(name);
                    self.directories.insert(original.clone(), placed.clone());
                    placed
                }
            };
        }
        let name = names::make_unique_portable(
            &adapt(file),
            self.names.entry(parent.clone()).or_default(),
        );
        parent.join(name)
    }
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

/// Recognizes the fixed temporary pathname used by older versions.
///
/// New writes use [`TemporaryOutput`] with an exclusive private directory;
/// this helper remains available for callers identifying abandoned old files.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    /// The file that was being written.
    pub source: PathBuf,
    /// What went wrong, worded for the user.
    pub reason: String,
}

/// Copies one file through isolated temporary output.
///
/// `verify` flushes and compares newly copied bytes using constant memory.
/// Read-back may still come from the operating-system cache; it does not prove
/// durable media contents. Existing same-size files are a resume heuristic and
/// remain unchecked by this wrapper. Use [`copy_one_checked`] to check them.
/// Call [`validate_destination`] first when the destination belongs to a plan.
pub fn copy_one(
    source: &Path,
    destination: &Path,
    expected_size: u64,
    verify: bool,
    replace: bool,
) -> Result<Wrote, String> {
    copy_one_checked(source, destination, expected_size, verify, replace, false)
}

/// Copies a file, optionally requiring matching bytes for an existing output.
///
/// A mismatching existing output is retained and reported as an error; explicit
/// replacement is required to refresh it. Verification has fixed memory use.
pub fn copy_one_checked(
    source: &Path,
    destination: &Path,
    expected_size: u64,
    verify: bool,
    replace: bool,
    verify_existing: bool,
) -> Result<Wrote, String> {
    validate_source(source)?;
    if let (Ok(source_path), Ok(destination_path)) =
        (source.canonicalize(), destination.canonicalize())
        && source_path == destination_path
    {
        return Err(format!(
            "{}: source and destination are the same file",
            destination.display()
        ));
    }
    if !replace && let Some(existing) = output::regular_destination(destination)? {
        if verify_existing {
            return match files_match(source, destination)? {
                true => Ok(Wrote::Skipped),
                false => Err(format!(
                    "{}: existing content does not match the source; use --replace to refresh it",
                    destination.display()
                )),
            };
        }
        if existing.len() == expected_size && expected_size > 0 {
            return Ok(Wrote::Skipped);
        }
        return Err(format!(
            "{}: existing size differs from the source; use --replace to refresh it",
            destination.display()
        ));
    }
    let temporary = TemporaryOutput::new(destination)?;
    let written = std::fs::copy(source, temporary.path())
        .map_err(|error| format!("{} → {}: {error}", source.display(), destination.display()))?;
    if written != expected_size && expected_size > 0 {
        return Err(format!(
            "{}: {written} bytes written where {expected_size} were expected",
            destination.display()
        ));
    }
    if verify {
        #[cfg(windows)]
        if !std::fs::metadata(temporary.path())
            .map_err(|error| error.to_string())?
            .permissions()
            .readonly()
        {
            std::fs::OpenOptions::new()
                .write(true)
                .open(temporary.path())
                .and_then(|file| file.sync_all())
                .map_err(|error| error.to_string())?;
        }
        #[cfg(not(windows))]
        std::fs::File::open(temporary.path())
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())?;
        if !files_match(source, temporary.path())? {
            return Err(format!(
                "{}: what was written back does not match what was read",
                destination.display()
            ));
        }
    }
    if replace {
        temporary.publish()?;
    } else {
        temporary.publish_new()?;
    }
    Ok(Wrote::Copied)
}

#[cfg(test)]
#[path = "copy_tests.rs"]
mod tests;
