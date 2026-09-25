//! FlacCompagnon analyses produced in-process or imported from a saved report.
//!
//! Aède describes what a file *contains* by reading its structure; it does not
//! decode. A decoder answers questions no structural read can: is this FLAC a
//! re-encoded MP3, was it upsampled, where does its spectrum stop, how loud is
//! it really, and — the decisive one — does the audio still match the MD5 the
//! encoder wrote in STREAMINFO.
//!
//! [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/) does that
//! pass through `aede analyze`; its saved reports can also be imported later.
//!
//! **Acoustic values are never merged into Aède's own.** They sit beside them,
//! attributed to their source, because a verdict carries the method that
//! produced it: overwriting `effective_bit_depth` — read from the wasted-bits
//! counts of the frames — with a figure obtained by decoding would leave the
//! catalog unable to say where the number came from, and unable to notice that
//! the two disagree. Noticing is the point.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use crate::json::{self, Json};
use crate::model::Catalog;
use md5::{Digest, Md5};

/// The report format this reader understands.
pub const FLACCOMPAGNON_FORMAT: &str = "flaccompagnon-report";

/// One file, as FlacCompagnon measured it.
///
/// Every measurement is optional: a report may predate a field, omit one, or
/// come from a tool that does not compute it. Nothing here is required for the
/// catalog to work — this is entirely additional.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileAnalysis {
    /// Absolute path of the file this describes.
    ///
    /// The primary key is the **path**, not an identifier into the catalog. Two
    /// reasons, and the second is the important one. Identifiers are positions
    /// that a scan renumbers, so an identifier would have to be remapped after
    /// every scan. And an analysis may perfectly well describe a file the
    /// catalog does not hold *yet*: someone can run the analysis first and
    /// build their library afterwards. Keying on the path lets such a record
    /// wait, and attach itself the day the file is scanned. A whole-file MD5
    /// can identify a renamed copy when the old path no longer exists.
    pub path: String,
    /// Tool that produced the measurements, `flaccompagnon` for now.
    pub source: String,
    /// Version of the report format, kept so an old import stays readable.
    pub source_version: u32,
    /// When it was imported, in seconds since the Unix epoch.
    pub imported_at: u64,
    /// Size the analysed file had, in bytes.
    ///
    /// With [`FileAnalysis::modified_unix`], this makes an exact-path or legacy
    /// name-and-size match expire when the file changes. A moved file with a
    /// matching whole-file MD5 can keep its analysis after a timestamp change.
    pub size_bytes: u64,
    /// Modification date the analysed file had.
    pub modified_unix: u64,
    /// MD5 of the entire file, including tags and artwork, when supplied.
    /// Used to identify a moved file without trusting its name or timestamp.
    pub file_md5: Option<String>,

    /// Verdict on the MD5 stored in STREAMINFO: `Match`, `Mismatch`,
    /// `NoSignature`, `Present`, `Error`.
    ///
    /// A *state*, not a hash — the tool compares and reports, it does not
    /// publish the digest. `Match` is equivalent to a successful `flac -t`.
    pub md5_state: Option<String>,
    /// What went wrong, when the state is an error.
    pub md5_detail: Option<String>,

    /// Bit depth actually carried, as measured by decoding.
    pub real_bit_depth: Option<u16>,
    /// Proportion of samples that show requantisation.
    pub requant_rate: Option<f64>,
    /// Both channels hold the same signal.
    pub fake_stereo: Option<bool>,
    /// The extension does not match the container found inside.
    pub ext_mismatch: Option<bool>,

    /// The file was made from a lower-resolution source: `none`, `suspected`,
    /// `detected`.
    pub transcoding: Option<String>,
    /// A lossless file built from a lossy one.
    pub upscaling: Option<bool>,
    /// Sample rate raised above what the content justifies.
    pub upsampling: Option<bool>,
    /// One-word verdict, as the tool words it.
    pub summary: Option<String>,
    /// The same verdict in a sentence, worth showing as it stands.
    pub detail: Option<String>,

    /// Frequency where the spectrum stops, in hertz. A cutoff well below half
    /// the sample rate is the signature of a lossy ancestry.
    pub cutoff_hz: Option<f64>,
    /// That frequency as a fraction of the Nyquist limit.
    pub cutoff_ratio: Option<f64>,

    /// Dynamic range in decibels: the lower, the more compressed the master.
    pub dr_db: Option<f64>,
    /// Highest sample value, in dBFS.
    pub peak_dbfs: Option<f64>,
    /// True peak, in dBTP — what a converter will actually have to produce.
    pub true_peak_dbtp: Option<f64>,
    /// Samples sitting at full scale.
    pub clipped_samples: Option<u64>,
    /// Runs of consecutive clipped samples.
    pub clip_events: Option<u64>,
    /// The tool's own conclusion on clipping.
    pub clipped: Option<bool>,

    /// What stopped the analysis of this file, when something did.
    pub error: Option<String>,
}

impl FileAnalysis {
    /// `true` when the analysis still describes the file as it is now.
    ///
    /// Same test as the incremental scan: unchanged size and modification date.
    /// A stale analysis is worse than none — it answers with confidence about
    /// bytes that are no longer there.
    pub fn still_applies(&self, size: u64, mtime: u64) -> bool {
        self.size_bytes == size && self.modified_unix == mtime
    }

    /// `true` when the decoded audio did not match the file's own MD5.
    pub fn md5_failed(&self) -> bool {
        self.md5_state.as_deref() == Some("Mismatch")
    }

    /// `true` when the file is suspected of being built from a lossy source.
    pub fn suspect_encoding(&self) -> bool {
        matches!(
            self.transcoding.as_deref(),
            Some("detected") | Some("suspected")
        ) || self.upscaling == Some(true)
            || self.upsampling == Some(true)
    }

    /// Last component of [`FileAnalysis::path`].
    ///
    /// With the size, this is what identifies a file that has moved since it
    /// was analysed — a name and a byte count together are very nearly unique
    /// in a music library.
    pub fn file_name(&self) -> &str {
        crate::text::file_name(&self.path)
    }
}

/// What a report file turned out to hold.
#[derive(Debug)]
pub struct Report {
    /// Folder the tool was pointed at.
    pub root: String,
    /// Version declared by the report.
    pub version: u32,
    /// One entry per analysed file.
    pub files: Vec<FileAnalysis>,
}

/// Waiting and stale **folders** listed by [`Attachment`]; the rest are
/// counted only.
///
/// Bounded on folders rather than on files because that is the unit a reader
/// acts on: one album of fourteen tracks is one decision, and fourteen rows
/// saying so push the next album off the screen.
const FOLDERS_SHOWN: usize = 10;

/// What became of a batch of records handed to [`merge_into`].
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Attachment {
    /// Records whose path the catalog already knew.
    pub matched: usize,
    /// Records attached to a file that has moved since it was analysed.
    pub moved: usize,
    /// Records about bytes that have changed since, and therefore dropped.
    pub stale: usize,
    /// Records kept although no file matches them yet.
    pub waiting: usize,
    /// The first few waiting **folders**, each with how many wait in it, for a
    /// report that names rather than counts.
    ///
    /// Folders, not paths, and for the reason a reader would give: what one
    /// does with a waiting record is scan the folder it names, or decide that
    /// folder is gone. The file name inside it settles nothing, and listing
    /// every file of an album spends ten rows to say one thing. It also
    /// survives the column width — a path cut to fit is cut at the *tail*,
    /// where the file name is, leaving the head that identifies the folder
    /// exactly where it cannot be read.
    pub waiting_folders: BTreeMap<String, usize>,
    /// The first few **folders** holding a record dropped as stale, each with
    /// how many, for the same reason `waiting_folders` names rather than
    /// counts: "some files changed" answers how many, not which, and which is
    /// the one thing a reader needs to know where to point FlacCompagnon again.
    pub stale_folders: BTreeMap<String, usize>,
}

impl Attachment {
    /// Records that found their file, one way or the other.
    pub fn attached(&self) -> usize {
        self.matched + self.moved
    }
}

/// Counts one more record against the folder it sits in, capping how many
/// distinct folders are kept rather than how many records count against them
/// — a folder already listed keeps counting past the cap, only a new one is
/// refused room.
fn note_folder(folders: &mut BTreeMap<String, usize>, path: &str) {
    let folder = crate::text::folder(path);
    if let Some(count) = folders.get_mut(folder) {
        *count += 1;
    } else if folders.len() < FOLDERS_SHOWN {
        folders.insert(folder.to_string(), 1);
    }
}

/// Stores records in the catalog, attaching each to the file it describes.
///
/// Three outcomes make the order of analysis and scanning irrelevant:
///
/// - the path is one the catalog holds — the usual case;
/// - the path is unknown, but one file has the same size and whole-file MD5;
///   older reports without a digest can use a unique name and size instead;
///   the record is refiled under its current location;
/// - nothing matches, and the record is kept as it is. The folder it names has
///   most likely not been scanned yet.
///
/// Exact-path and legacy name-and-size matches also require the old size and
/// modification date. A digest match proves the bytes agree, so a changed
/// timestamp alone is harmless. Duplicate digests remain unresolved.
pub fn merge_into(catalog: &mut Catalog, records: Vec<FileAnalysis>, now: u64) -> Attachment {
    let mut out = Attachment::default();
    let mut resolved: Vec<FileAnalysis> = Vec::with_capacity(records.len());

    {
        let known: BTreeMap<&str, (u64, u64)> = catalog
            .files
            .iter()
            .map(|f| (f.path.as_str(), (f.size, f.mtime)))
            .collect();
        let by_name_size = name_size_index(catalog);
        let wanted_sizes = records
            .iter()
            .filter(|record| !known.contains_key(record.path.as_str()))
            .filter(|record| record.file_md5.is_some())
            .map(|record| record.size_bytes)
            .collect();
        let by_digest = file_md5_index(catalog, &wanted_sizes);

        for mut record in records {
            match known.get(record.path.as_str()) {
                Some(&(size, mtime)) => {
                    if !record.still_applies(size, mtime) {
                        out.stale += 1;
                        note_folder(&mut out.stale_folders, &record.path);
                        continue;
                    }
                    out.matched += 1;
                }
                None => {
                    if let Some(digest) = &record.file_md5 {
                        if let Some(&(path, mtime)) =
                            by_digest.get(&(record.size_bytes, digest.clone()))
                        {
                            record.path = path.to_string();
                            record.modified_unix = mtime;
                            out.moved += 1;
                        } else {
                            out.waiting += 1;
                            note_folder(&mut out.waiting_folders, &record.path);
                        }
                    } else {
                        match by_name_size.get(&(record.file_name(), record.size_bytes)) {
                            Some(&(path, mtime)) => {
                                if !record.still_applies(record.size_bytes, mtime) {
                                    out.stale += 1;
                                    note_folder(&mut out.stale_folders, path);
                                    continue;
                                }
                                record.path = path.to_string();
                                out.moved += 1;
                            }
                            None => {
                                out.waiting += 1;
                                note_folder(&mut out.waiting_folders, &record.path);
                            }
                        }
                    }
                }
            }
            record.imported_at = now;
            resolved.push(record);
        }
    }

    for record in resolved {
        store_one(catalog, record);
    }
    sort_analyses(catalog);
    out
}

/// Attaches the records that were still waiting for their file.
///
/// Called after a scan. A record waits because nothing matched it at import
/// time, and the usual reason is that its folder had not been scanned yet — but
/// also that the path it names is not the path the catalog settled on. Watched
/// folders are stored canonical, so a report written against a symbolic link,
/// or against `/var` where the system says `/private/var`, names the same file
/// by another route. A whole-file MD5 resolves the path when available; older
/// reports still use a unique name, size and modification date.
///
/// Returns how many were attached.
pub fn reconcile(catalog: &mut Catalog) -> usize {
    let mut moves: Vec<(usize, String, u64)> = Vec::new();
    {
        let known: BTreeSet<&str> = catalog.files.iter().map(|f| f.path.as_str()).collect();
        let by_name_size = name_size_index(catalog);
        let wanted_sizes = catalog
            .analyses
            .iter()
            .filter(|record| !known.contains(record.path.as_str()))
            .filter(|record| record.file_md5.is_some())
            .map(|record| record.size_bytes)
            .collect();
        let by_digest = file_md5_index(catalog, &wanted_sizes);
        let taken: BTreeSet<(&str, &str)> = catalog
            .analyses
            .iter()
            .filter(|a| known.contains(a.path.as_str()))
            .map(|a| (a.path.as_str(), a.source.as_str()))
            .collect();

        for (index, record) in catalog.analyses.iter().enumerate() {
            if known.contains(record.path.as_str()) {
                continue;
            }
            let candidate = if let Some(digest) = &record.file_md5 {
                by_digest.get(&(record.size_bytes, digest.clone())).copied()
            } else {
                by_name_size
                    .get(&(record.file_name(), record.size_bytes))
                    .filter(|(_, mtime)| record.still_applies(record.size_bytes, *mtime))
                    .copied()
            };
            let Some((path, mtime)) = candidate else {
                continue;
            };
            // A record already filed under the real path wins: it was attached
            // deliberately, this one is being guessed at.
            if taken.contains(&(path, record.source.as_str())) {
                continue;
            }
            moves.push((index, path.to_string(), mtime));
        }
    }
    let attached = moves.len();
    for (index, path, mtime) in moves {
        catalog.analyses[index].path = path;
        catalog.analyses[index].modified_unix = mtime;
    }
    if attached > 0 {
        // Refiling two records onto the same path is possible when a report
        // named the same file twice by two routes; the last one wins, as it
        // does on import.
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        catalog.analyses.reverse();
        catalog
            .analyses
            .retain(|a| seen.insert((a.path.clone(), a.source.clone())));
        catalog.analyses.reverse();
        sort_analyses(catalog);
    }
    attached
}

/// Files indexed by name and size, with ambiguous keys excluded.
///
/// If two albums contain the same name and byte count, guessing the first
/// path would attach a measurement to the wrong music. The report can wait
/// until an exact path is available instead.
fn name_size_index(catalog: &Catalog) -> BTreeMap<(&str, u64), (&str, u64)> {
    let mut index: BTreeMap<(&str, u64), (&str, u64)> = BTreeMap::new();
    let mut ambiguous = BTreeSet::new();
    for file in &catalog.files {
        let key = (file.file_name(), file.size);
        if ambiguous.contains(&key) {
            continue;
        }
        if index
            .insert(key, (file.path.as_str(), file.mtime))
            .is_some()
        {
            index.remove(&key);
            ambiguous.insert(key);
        }
    }
    index
}

/// Find byte-identical files for reports whose old paths no longer exist.
/// Duplicate hashes stay unresolved: identical copies still have distinct
/// album placements, and the report cannot say which one it described.
fn file_md5_index<'a>(
    catalog: &'a Catalog,
    wanted_sizes: &BTreeSet<u64>,
) -> BTreeMap<(u64, String), (&'a str, u64)> {
    let mut index = BTreeMap::new();
    let mut ambiguous = BTreeSet::new();
    for file in &catalog.files {
        if !wanted_sizes.contains(&file.size) {
            continue;
        }
        let Ok(digest) = file_md5(Path::new(&file.path)) else {
            continue;
        };
        let key = (file.size, digest);
        if ambiguous.contains(&key) {
            continue;
        }
        if index
            .insert(key.clone(), (file.path.as_str(), file.mtime))
            .is_some()
        {
            index.remove(&key);
            ambiguous.insert(key);
        }
    }
    index
}

fn file_md5(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Md5::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Stores one record, replacing what that source had already said about that
/// path. Importing the same report twice replaces, it does not accumulate.
fn store_one(catalog: &mut Catalog, record: FileAnalysis) {
    catalog
        .analyses
        .retain(|a| !(a.path == record.path && a.source == record.source));
    catalog.analyses.push(record);
}

fn sort_analyses(catalog: &mut Catalog) {
    catalog
        .analyses
        .sort_by(|a, b| (&a.path, &a.source).cmp(&(&b.path, &b.source)));
}

/// Anything that can stop a report from being read.
#[derive(Debug)]
pub enum ImportError {
    /// The file could not be opened or read.
    Io(std::io::Error),
    /// The text is not valid JSON.
    Json(String),
    /// Valid JSON, but not a report this reader knows.
    NotAReport {
        /// What the file claimed to be.
        found: String,
    },
    /// A report of a version written after this build.
    Version {
        /// Version the file declares.
        found: u32,
    },
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Io(e) => write!(f, "input/output error: {e}"),
            ImportError::Json(e) => write!(f, "not valid JSON: {e}"),
            ImportError::NotAReport { found } => write!(
                f,
                "not a FlacCompagnon report (format is \"{found}\", expected \"{FLACCOMPAGNON_FORMAT}\")"
            ),
            ImportError::Version { found } => write!(
                f,
                "report version {found} was written by a later FlacCompagnon; upgrade Aède"
            ),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<std::io::Error> for ImportError {
    fn from(e: std::io::Error) -> Self {
        ImportError::Io(e)
    }
}

/// Highest report version this reader understands.
const SUPPORTED_VERSION: u32 = 1;

/// Reads a FlacCompagnon report.
///
/// Unknown fields are ignored rather than refused: the tool will grow new
/// measurements, and a report carrying one must still import the rest.
pub fn read_report(path: &Path) -> Result<Report, ImportError> {
    let text = std::fs::read_to_string(path)?;
    parse_report(&text)
}

/// Parse a report already held in memory, including a newly produced analysis.
pub fn parse_report(text: &str) -> Result<Report, ImportError> {
    let value = json::parse(text).map_err(|e| ImportError::Json(e.to_string()))?;

    let format = value.field_str("format").unwrap_or_default();
    if format != FLACCOMPAGNON_FORMAT {
        return Err(ImportError::NotAReport { found: format });
    }
    let version = value.field_u32("version").unwrap_or(0);
    if version > SUPPORTED_VERSION {
        return Err(ImportError::Version { found: version });
    }

    let report = value.get("report");
    let root = report.and_then(|r| r.field_str("root")).unwrap_or_default();
    let entries = report
        .and_then(|r| r.get("files"))
        .and_then(|f| f.as_arr())
        .unwrap_or(&[]);

    // An entry without a path cannot be attached to anything, ever: it is
    // dropped here rather than stored as a record about no file in particular.
    let files = entries
        .iter()
        .filter(|item| item.field_str("path").is_some())
        .map(|item| from_json(item, version))
        .collect();

    Ok(Report {
        root,
        version,
        files,
    })
}

/// Bytes read from a file to decide whether it is worth parsing.
const SNIFF_BYTES: usize = 512;

/// `true` when a file announces itself as a report, on a glance at its head.
///
/// A scan walks past every file in the library and must not parse each `.json`
/// it meets — a cover-art sidecar, a player's own database, anything. The
/// format marker sits in the first object of the file, so half a kilobyte
/// settles it without reading a report that may hold thousands of entries.
pub fn looks_like_a_report(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; SNIFF_BYTES];
    let Ok(read) = file.read(&mut head) else {
        return false;
    };
    // Lossy on purpose: the read may well have cut a multi-byte character in
    // half, and the marker being looked for is plain ASCII.
    String::from_utf8_lossy(&head[..read]).contains(FLACCOMPAGNON_FORMAT)
}

/// Maps one entry of a report onto [`FileAnalysis`].
fn from_json(item: &Json, version: u32) -> FileAnalysis {
    let detections = item.get("detections");
    let clipping = item.get("clipping");
    let md5 = item.get("flac_md5");

    FileAnalysis {
        path: item.field_str("path").unwrap_or_default(),
        source: "flaccompagnon".to_string(),
        source_version: version,
        imported_at: 0,
        size_bytes: item.field_u64("size_bytes").unwrap_or(0),
        modified_unix: item.field_u64("modified_unix").unwrap_or(0),
        file_md5: item.field_str("file_md5"),

        md5_state: md5.and_then(|m| m.field_str("state")),
        md5_detail: md5.and_then(|m| m.field_str("detail")),

        real_bit_depth: item.field_u32("real_bit_depth").map(|v| v as u16),
        requant_rate: item.field_f64("requant_rate"),
        fake_stereo: item.field_optional_bool("fake_stereo"),
        ext_mismatch: item.field_optional_bool("ext_mismatch"),

        transcoding: detections.and_then(|d| d.field_str("transcoding")),
        upscaling: detections.and_then(|d| d.field_optional_bool("upscaling")),
        upsampling: detections.and_then(|d| d.field_optional_bool("upsampling")),
        summary: detections.and_then(|d| d.field_str("summary")),
        detail: detections.and_then(|d| d.field_str("detail")),

        cutoff_hz: item.field_f64("cutoff_hz"),
        cutoff_ratio: item.field_f64("cutoff_ratio"),

        dr_db: item.field_f64("dr_db"),
        peak_dbfs: clipping.and_then(|c| c.field_f64("peak_dbfs")),
        true_peak_dbtp: clipping.and_then(|c| c.field_f64("true_peak_dbtp")),
        clipped_samples: clipping.and_then(|c| c.field_u64("clipped_samples")),
        clip_events: clipping.and_then(|c| c.field_u64("clip_events")),
        clipped: clipping.and_then(|c| c.field_optional_bool("clipped")),

        error: item.field_str("error"),
    }
}

#[cfg(test)]
#[path = "analysis_tests.rs"]
mod tests;
