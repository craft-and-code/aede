//! Directory traversal and parallel file reading.
//!
//! The scan is **incremental**: a file whose path, size and modification date
//! have not changed is not read again, its tags are taken from the previous
//! catalog. On a library of 50,000 titles, that is the difference between
//! several minutes and a few seconds.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use crate::analysis;
use crate::clock::{mtime_seconds, mtime_subseconds, now_seconds};
use crate::model::{self, Catalog, ScannedFile};
use crate::tags::{self, RawTags};

/// Image file names recognised as album cover art, in order of preference.
const COVER_NAMES: &[&str] = &["cover", "folder", "front", "albumart", "album", "artwork"];
const COVER_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp"];

/// Knobs governing a traversal. [`Default`] gives the settings suited to a
/// personal library: automatic parallelism, no symlink following, hidden
/// entries left alone.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Read every accessible file again while retaining the previous catalog
    /// as evidence for inaccessible paths. This does not discard cached data
    /// merely because a watched drive or subfolder cannot be read.
    pub force_read: bool,
    /// Number of reader threads. 0 = automatic detection.
    pub threads: usize,
    /// Follow symbolic links (with loop detection).
    pub follow_symlinks: bool,
    /// Skip files and folders starting with a dot.
    pub skip_hidden: bool,
    /// Folders never to walk into, canonical, whatever a root says.
    ///
    /// A music folder is rarely only music: `Audiobooks`, `Podcasts`,
    /// `_incoming`, a `Samples` folder for a DAW. Without this the only way to
    /// keep them out of the catalog is to reorganise the disk to suit the
    /// program, which is the wrong way round.
    ///
    /// They live in the **catalog**, not in this run's options, because a
    /// plain `aede scan` re-reads every watched root: an exclusion that had to
    /// be retyped would be forgotten exactly when it mattered.
    pub excluded: Vec<PathBuf>,
    /// Spellings the owner of the disk has said are one artist.
    ///
    /// The half of artist identity no identifier can settle, and the reason it
    /// travels here rather than in the catalog: it is **the user's**, it lives
    /// in `user.json`, and this crate does not read that file — the caller
    /// does, as it does for every other statement in it.
    ///
    /// It is applied when the graph is built, which is why a merge takes
    /// effect on the next `aede scan` and not before: the spelling a track is
    /// filed under is decided as the artist is interned, and there is no later
    /// moment at which one row can become another without rebuilding
    /// everything that points at it.
    pub same_artist: Vec<crate::model::identity::Chosen>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            force_read: false,
            threads: 0,
            follow_symlinks: false,
            skip_hidden: true,
            excluded: Vec::new(),
            same_artist: Vec::new(),
        }
    }
}

/// Summary of a scan.
#[derive(Debug, Default)]
pub struct ScanReport {
    /// Audio files spotted during traversal, after deduplication. Always the
    /// sum of `read` and `reused`, failures included.
    pub found: usize,
    /// Files selected for a fresh read, including unsuccessful attempts.
    pub read: usize,
    /// Files taken unchanged from the previous catalog.
    pub reused: usize,
    /// Files present in the old catalog and gone since.
    pub removed: usize,
    /// Previous files retained because their file or directory was inaccessible.
    /// These are separate from unchanged files counted in `reused`.
    pub preserved: usize,
    /// File paths newly entering the catalog, sorted by path.
    pub added_paths: Vec<String>,
    /// Existing file paths whose size or precise timestamp changed.
    pub changed_paths: Vec<String>,
    /// Previously catalogued file paths absent from accessible scanned folders.
    pub removed_paths: Vec<String>,
    /// Files or directories that could not be read, sorted by path.
    /// Inaccessible paths retain their previous entries. Invalid audio content
    /// excludes the file, with its parsing failure reported here. Non-UTF-8
    /// entries are skipped and reported with escaped native path spelling.
    pub failures: Vec<(String, String)>,
    /// Analysis reports found inside the folders and taken in.
    pub reports: usize,
    /// Imported analyses that were waiting for a file and found it this time.
    pub attached: usize,
    /// Imported analyses the catalog holds once the scan is done, the ones
    /// carried over from the previous catalog included.
    pub analyses: usize,
    /// Wall-clock time of the whole scan in milliseconds, traversal included.
    pub elapsed_ms: u128,
}

/// Progress event, for display.
#[derive(Debug, Clone, Copy)]
pub enum Progress {
    /// Directory traversal finished: number of audio files spotted.
    Discovered(usize),
    /// `n` files processed out of `total`.
    Read {
        /// Files pulled off the read queue so far, failures included.
        done: usize,
        /// Files needing a fresh read. On an incremental scan this stays well
        /// below the count announced by [`Progress::Discovered`], since reused
        /// files never enter the queue.
        total: usize,
    },
}

/// Scans the given folders and builds a catalog.
///
/// `previous` enables incremental reuse and retention of inaccessible paths.
/// To reread accessible files while keeping that evidence, set
/// [`ScanOptions::force_read`]. Passing `None` starts without any prior data.
///
/// Only ordinary files are opened as audio. Roots and directory entries are
/// traversed in native path order so followed directory aliases choose the
/// same spelling. A root that cannot be represented exactly as UTF-8 is an
/// invalid input; unrepresentable entries are reported and skipped.
///
/// Fresh reads compare filesystem metadata before and after tag parsing. An
/// observed rewrite is reported as temporary unavailability rather than
/// cached or mistaken for permanent corruption. This is a filesystem snapshot
/// check, not a content hash or a transaction with concurrent file writers.
pub fn scan(
    roots: &[PathBuf],
    previous: Option<&Catalog>,
    options: &ScanOptions,
    mut on_progress: impl FnMut(Progress) + Send,
) -> std::io::Result<(Catalog, ScanReport)> {
    let started = Instant::now();
    let mut report = ScanReport::default();
    // The persistent path contract is UTF-8. Refuse an unrepresentable root
    // before a lossy spelling could become watched or collide with another.
    let mut roots_str: Vec<String> = roots
        .iter()
        .map(|path| {
            path.to_str().map(str::to_owned).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{path:?}: path is not valid UTF-8"),
                )
            })
        })
        .collect::<std::io::Result<_>>()?;
    roots_str.sort();
    roots_str.dedup();
    let mut traversal_roots: Vec<&PathBuf> = roots.iter().collect();
    traversal_roots.sort();
    traversal_roots.dedup();

    // --- 1. Traversal ------------------------------------------------------
    let mut walker = Walker::new(options);
    for root in traversal_roots {
        walker.walk(root)?;
    }
    let Walker {
        mut audio_files,
        reports: mut reports_found,
        folder_covers,
        sidecars,
        failures: traversal_failures,
        inaccessible,
        ..
    } = walker;
    reports_found.sort();
    audio_files.sort();
    audio_files.dedup();
    report.found = audio_files.len();
    on_progress(Progress::Discovered(audio_files.len()));

    // --- 2. What can be taken as is ----------------------------------------
    let mut cache: HashMap<&str, &model::AudioFile> = HashMap::new();
    if let Some(prev) = previous {
        for file in &prev.files {
            cache.insert(file.path.as_str(), file);
        }
    }

    let mut to_read: Vec<PathBuf> = Vec::new();
    let mut reused: Vec<ScannedFile> = Vec::new();
    let mut fractions = BTreeMap::new();

    for path in &audio_files {
        let path_str = path.to_string_lossy().to_string();
        let Ok(meta) = file_metadata(path, options.follow_symlinks) else {
            to_read.push(path.clone());
            continue;
        };
        let size = meta.len();
        let mtime = mtime_seconds(&meta);
        let fraction = mtime_subseconds(&meta);

        match cache.get(path_str.as_str()) {
            Some(old)
                if !options.force_read
                    && old.size == size
                    && old.mtime == mtime
                    && previous
                        .and_then(|catalog| catalog.file_mtime_subseconds.get(&path_str))
                        == Some(&fraction) =>
            {
                fractions.insert(path_str.clone(), fraction);
                let mut tags = RawTags {
                    fields: old.tags.clone(),
                    properties: old.properties.clone(),
                    has_embedded_art: old.has_embedded_art,
                };
                // Cover art may have appeared in the folder since.
                tags.properties.container = old.properties.container.clone();
                reused.push(ScannedFile {
                    path: path_str,
                    size,
                    mtime,
                    tags,
                    folder_cover: cover_for(&folder_covers, path),
                    // Read from the fresh walk rather than carried over: a
                    // `.lrc` dropped beside a track nobody touched must attach
                    // on the next scan, and the track's own bytes have not
                    // changed to say so.
                    sidecar: sidecar_for(&sidecars, path),
                    // The file has not moved and has not changed, so what was
                    // concluded about it still holds — both of the expensive
                    // conclusions, the checksum verdict and the fingerprint.
                    integrity: old.integrity.clone(),
                    fingerprint: old.fingerprint.clone(),
                });
            }
            _ => to_read.push(path.clone()),
        }
    }
    report.reused = reused.len();
    report.read = to_read.len();

    // --- 3. Parallel reading ----------------------------------------------
    let thread_count = resolve_threads(options.threads).min(to_read.len().max(1));
    let queue = Mutex::new(to_read);
    let results: Mutex<Vec<(ScannedFile, u32)>> = Mutex::new(Vec::new());
    let failures: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
    let unavailable: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
    let done = AtomicUsize::new(0);
    let total = report.read;

    std::thread::scope(|scope| {
        for _ in 0..thread_count {
            scope.spawn(|| {
                loop {
                    let Some(path) = queue.lock().unwrap_or_else(|e| e.into_inner()).pop() else {
                        break;
                    };
                    let path_str = path.to_string_lossy().to_string();
                    let outcome = read_fresh(&path, options.follow_symlinks, tags::read);
                    match outcome {
                        Ok((metadata, tags)) => {
                            let fraction = mtime_subseconds(&metadata);
                            let file = ScannedFile {
                                path: path_str,
                                size: metadata.len(),
                                mtime: mtime_seconds(&metadata),
                                tags,
                                folder_cover: cover_for(&folder_covers, &path),
                                sidecar: sidecar_for(&sidecars, &path),
                                // A file read again is a file that changed:
                                // any earlier verdict is about other bytes,
                                // and so is any earlier fingerprint.
                                integrity: None,
                                fingerprint: None,
                            };
                            results.lock().unwrap_or_else(|e| e.into_inner()).push((file, fraction));
                        }
                        Err(error) => {
                            if matches!(&error, tags::TagError::Io(error) if error.kind() != std::io::ErrorKind::NotFound) {
                                unavailable.lock().unwrap_or_else(|e| e.into_inner()).insert(path_str.clone());
                            }
                            failures
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .push((path_str, error.to_string()));
                        }
                    }
                    done.fetch_add(1, Ordering::Relaxed);
                }
            });
        }

        // The main thread only reports progress.
        let mut last = 0usize;
        while done.load(Ordering::Relaxed) < total {
            let current = done.load(Ordering::Relaxed);
            if current != last {
                last = current;
                on_progress(Progress::Read {
                    done: current,
                    total,
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
    });

    let mut scanned: Vec<ScannedFile> = results
        .into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .into_iter()
        .map(|(file, fraction)| {
            fractions.insert(file.path.clone(), fraction);
            file
        })
        .collect();
    report.failures = failures.into_inner().unwrap_or_else(|e| e.into_inner());
    report.failures.extend(traversal_failures.iter().cloned());
    report.failures.sort();
    report.failures.dedup();
    scanned.extend(reused);
    // An unsuccessful walk cannot establish absence. Preserve only entries
    // below the failed paths, never excluded folders or intentionally removed
    // roots. An I/O failure while opening an audio file has the same meaning.
    let unavailable = unavailable.into_inner().unwrap_or_else(|e| e.into_inner());
    if let Some(previous) = previous {
        let present: HashSet<&str> = scanned.iter().map(|file| file.path.as_str()).collect();
        let retained: Vec<&model::AudioFile> = previous
            .files
            .iter()
            .filter(|file| {
                !present.contains(file.path.as_str())
                    && !options
                        .excluded
                        .iter()
                        .any(|folder| crate::text::is_under(&file.path, &folder.to_string_lossy()))
                    && (Path::new(&file.path)
                        .ancestors()
                        .any(|ancestor| inaccessible.contains(ancestor))
                        || unavailable.contains(&file.path))
            })
            .collect();
        let covers: HashMap<model::Id, &str> = if retained.is_empty() {
            HashMap::new()
        } else {
            previous
                .tracks
                .iter()
                .filter_map(|track| {
                    let release = previous.release(track.release_id?)?;
                    Some((track.file_id, release.cover_path.as_deref()?))
                })
                .collect()
        };
        for file in retained {
            if let Some(fraction) = previous.file_mtime_subseconds.get(&file.path) {
                fractions.insert(file.path.clone(), *fraction);
            }
            scanned.push(cached_file(
                file,
                covers.get(&file.id).map(|path| (*path).to_owned()),
            ));
            report.preserved += 1;
        }
    }
    on_progress(Progress::Read { done: total, total });

    // --- 4. Building the graph ---------------------------------------------
    let mut catalog = model::build(scanned, roots_str, now_seconds(), &options.same_artist);
    catalog.file_mtime_subseconds = fractions;
    let current_paths: HashSet<&str> = catalog
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    catalog
        .file_mtime_subseconds
        .retain(|path, _| current_paths.contains(path.as_str()));
    record_changes(&catalog, previous, &mut report);

    // Analyses are keyed by path, so they simply travel: nothing to remap, and
    // nothing to lose. They are the one thing in a catalog that reading the
    // files again cannot recompute.
    //
    // Exclusions travel for the same reason and it is the same rule — **a scan
    // may not destroy what it cannot recompute**. They are typed by the user
    // and derived from nothing, so a rebuild that dropped them would forget
    // them on the very run they were meant to shape. That is not theory: the
    // first version of this feature lost them exactly here, and the symptom
    // was an exclusion that worked once and then vanished.
    if let Some(previous) = previous {
        catalog.analyses = previous.analyses.clone();
        catalog.excluded = previous.excluded.clone();
    }
    // Reports lying in the library are taken in, so that analysing a folder and
    // then scanning it works as well as the other way round.
    report.reports = import_reports(&reports_found, &mut catalog);
    // And whatever was waiting is given another chance now that the files are
    // known — including records naming the same file by another route, which
    // is what a report written against a symbolic link looks like.
    report.attached = analysis::reconcile(&mut catalog);
    report.analyses = catalog.analyses.len();

    report.elapsed_ms = started.elapsed().as_millis();
    Ok((catalog, report))
}

fn read_fresh(
    path: &Path,
    follow_symlinks: bool,
    read_tags: impl FnOnce(&Path) -> Result<RawTags, tags::TagError>,
) -> Result<(std::fs::Metadata, RawTags), tags::TagError> {
    let before = file_metadata(path, follow_symlinks)?;
    let tags = read_tags(path);
    let after = file_metadata(path, follow_symlinks)?;
    if !same_file_snapshot(&before, &after) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "file changed while its tags were read; scan again once it is stable",
        )
        .into());
    }
    tags.map(|tags| (after, tags))
}

fn file_metadata(path: &Path, follow_symlinks: bool) -> std::io::Result<std::fs::Metadata> {
    let metadata = if follow_symlinks {
        std::fs::metadata(path)?
    } else {
        std::fs::symlink_metadata(path)?
    };
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "audio path is no longer an ordinary file",
        ));
    }
    Ok(metadata)
}

fn same_file_snapshot(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || before.created().ok() != after.created().ok()
    {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return false;
        }
    }
    true
}

/// Rebuilds a cached file without inspecting an inaccessible directory.
fn cached_file(file: &model::AudioFile, folder_cover: Option<String>) -> ScannedFile {
    ScannedFile {
        path: file.path.clone(),
        size: file.size,
        mtime: file.mtime,
        tags: RawTags {
            fields: file.tags.clone(),
            properties: file.properties.clone(),
            has_embedded_art: file.has_embedded_art,
        },
        folder_cover,
        sidecar: file.lyrics_path.clone(),
        integrity: file.integrity.clone(),
        fingerprint: file.fingerprint.clone(),
    }
}

fn record_changes(catalog: &Catalog, previous: Option<&Catalog>, report: &mut ScanReport) {
    let old: HashMap<&str, &model::AudioFile> = previous
        .into_iter()
        .flat_map(|catalog| &catalog.files)
        .map(|file| (file.path.as_str(), file))
        .collect();
    let current: HashSet<&str> = catalog
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    for file in &catalog.files {
        match old.get(file.path.as_str()) {
            None => report.added_paths.push(file.path.clone()),
            Some(old)
                if old.size != file.size
                    || old.mtime != file.mtime
                    || previous
                        .and_then(|catalog| catalog.file_mtime_subseconds.get(&file.path))
                        != catalog.file_mtime_subseconds.get(&file.path) =>
            {
                report.changed_paths.push(file.path.clone());
            }
            _ => {}
        }
    }
    report.removed_paths = old
        .keys()
        .filter(|path| !current.contains(**path))
        .map(|path| (*path).to_string())
        .collect();
    report.removed_paths.sort();
    report.removed = report.removed_paths.len();
}

/// Reads the reports found while walking, and merges what they hold.
///
/// Returns how many were actually reports. A file is only parsed once
/// [`analysis::looks_like_a_report`] has recognised it, and one that turns out
/// to be unreadable is passed over: a scan is not the place to fail over a
/// sidecar file nobody asked about.
///
/// The records go through the same matching as `aede import` — the scan has no
/// business being more trusting than the command whose whole job this is.
fn import_reports(found: &[PathBuf], catalog: &mut Catalog) -> usize {
    let mut count = 0;
    let now = now_seconds();
    for path in found {
        let Ok(report) = analysis::read_report(path) else {
            continue;
        };
        count += 1;
        analysis::merge_into(catalog, report.files, now);
    }
    count
}

/// How many threads a run should use: what was asked for, or what the machine
/// offers.
///
/// Shared with `spectrum`, which spawns one ffmpeg per track and has exactly
/// the same question to answer. Two defaults that could disagree about what
/// `--threads` with no value means is one too many.
pub fn resolve_threads(requested: usize) -> usize {
    if requested > 0 {
        return requested;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// The `.lrc` beside this file, if the walk saw one.
fn sidecar_for(sidecars: &HashSet<PathBuf>, file: &Path) -> Option<String> {
    let expected = crate::lyrics::sidecar_of(file);
    sidecars
        .contains(&expected)
        .then(|| expected.to_string_lossy().to_string())
}

fn cover_for(covers: &HashMap<PathBuf, PathBuf>, file: &Path) -> Option<String> {
    let folder = file.parent()?;
    covers.get(folder).map(|p| p.to_string_lossy().to_string())
}

// --------------------------------------------------------------------------
// Tree traversal
// --------------------------------------------------------------------------

struct Walker<'a> {
    options: &'a ScanOptions,
    audio_files: Vec<PathBuf>,
    /// Analysis reports met on the way, to be taken in at the end.
    reports: Vec<PathBuf>,
    /// Best cover art found per folder.
    folder_covers: HashMap<PathBuf, PathBuf>,
    sidecars: HashSet<PathBuf>,
    /// Folders already visited, so as not to go round in circles on a
    /// circular symbolic link.
    visited: HashSet<PathBuf>,
    /// Failed paths are evidence of an unavailable subtree, not its deletion.
    failures: Vec<(String, String)>,
    /// Native paths used for ancestor lookups. One failed subtree must not
    /// trigger a scan of every recorded failure for every previous file.
    inaccessible: HashSet<PathBuf>,
}

impl<'a> Walker<'a> {
    fn new(options: &'a ScanOptions) -> Self {
        Walker {
            options,
            audio_files: Vec::new(),
            reports: Vec::new(),
            folder_covers: HashMap::new(),
            sidecars: HashSet::new(),
            visited: HashSet::new(),
            failures: Vec::new(),
            inaccessible: HashSet::new(),
        }
    }

    fn failure(&mut self, path: &Path, reason: impl Into<String>) {
        let spelling = path
            .to_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{path:?}"));
        self.failures.push((spelling, reason.into()));
        self.inaccessible.insert(path.to_path_buf());
    }

    /// `true` when a folder is one the user asked never to read, or sits
    /// inside one.
    fn is_excluded(&self, canonical: &Path) -> bool {
        let path = canonical.to_string_lossy();
        self.options
            .excluded
            .iter()
            .any(|folder| crate::text::is_under(&path, &folder.to_string_lossy()))
    }

    fn walk(&mut self, dir: &Path) -> std::io::Result<()> {
        let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        // Tested on the canonical path, so a folder reached through a symbolic
        // link is excluded too — the same reason every comparison against a
        // stored path in this program is made on a resolved one.
        if self.is_excluded(&canonical) {
            return Ok(());
        }
        if !self.visited.insert(canonical) {
            return Ok(()); // already seen: symlink loop
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            // An unreadable folder must not interrupt the whole scan.
            Err(error) => {
                self.failure(dir, error.to_string());
                return Ok(());
            }
        };

        let mut best_cover: Option<(usize, PathBuf)> = None;

        // Resolve directory aliases in a stable order. ReadDir order is not
        // promised by the filesystem and otherwise decides which native path
        // survives when two links reach the same directory.
        let mut paths = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    self.failure(dir, error.to_string());
                    continue;
                }
            };
            paths.push((entry.path(), entry.file_type()));
        }
        paths.sort_by(|(left, _), (right, _)| left.cmp(right));
        for (path, file_type) in paths {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default();
            if self.options.skip_hidden && name.starts_with('.') {
                continue;
            }
            if path.to_str().is_none() {
                self.failure(
                    &path,
                    "path is not valid UTF-8 and cannot be stored without losing its identity",
                );
                continue;
            }
            let file_type = match file_type {
                Ok(file_type) => file_type,
                Err(error) => {
                    self.failure(&path, error.to_string());
                    continue;
                }
            };

            if file_type.is_symlink() && !self.options.follow_symlinks {
                continue;
            }
            let file_type = if file_type.is_symlink() {
                match std::fs::metadata(&path) {
                    Ok(metadata) => metadata.file_type(),
                    Err(error) => {
                        self.failure(&path, error.to_string());
                        continue;
                    }
                }
            } else {
                file_type
            };

            if file_type.is_dir() {
                self.walk(&path)?;
            } else if !file_type.is_file() {
                // Opening a FIFO may wait forever; devices and sockets are
                // not library audio regardless of their filename extension.
                continue;
            } else if tags::is_audio_path(&path) {
                self.audio_files.push(path);
            } else if name.to_ascii_lowercase().ends_with(".json")
                && analysis::looks_like_a_report(&path)
            {
                // Someone may analyse a folder before ever scanning it, and
                // leave the report sitting in it. Picking it up here is what
                // makes the order of the two operations irrelevant.
                self.reports.push(path);
            } else if crate::lyrics::is_sidecar(&name) {
                // Noted while the folder is open rather than looked for later:
                // the walk already has the names in hand, and asking the
                // filesystem again once per track would be ten thousand
                // questions with the answers already on the table.
                self.sidecars.insert(path);
            } else if let Some(rank) = cover_rank(&name)
                && best_cover
                    .as_ref()
                    .is_none_or(|(seen_rank, seen_path)| (rank, &path) < (*seen_rank, seen_path))
            {
                best_cover = Some((rank, path));
            }
        }

        if let Some((_, cover)) = best_cover {
            self.folder_covers.insert(dir.to_path_buf(), cover);
        }
        Ok(())
    }
}

/// The image already serving as cover art in a folder, if there is one.
///
/// Reads the folder rather than the catalog, and shares its list of cover names with
/// the scanner so that "does this album have a cover" has one answer in this
/// program rather than two that drift. Used before anything writes an image
/// beside music: the catalog is a snapshot and the disk is not.
pub fn cover_in(folder: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut best: Option<(usize, std::path::PathBuf)> = None;
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(rank) = cover_rank(name)
            && best
                .as_ref()
                .is_none_or(|(seen_rank, seen_path)| (rank, &path) < (*seen_rank, seen_path))
        {
            best = Some((rank, path));
        }
    }
    best.map(|(_, path)| path)
}

fn cover_rank(name: &str) -> Option<usize> {
    let lower = name.to_ascii_lowercase();
    let (stem, ext) = lower.rsplit_once('.')?;
    if !COVER_EXTENSIONS.contains(&ext) {
        return None;
    }
    for (rank, candidate) in COVER_NAMES.iter().enumerate() {
        if stem == *candidate || stem.starts_with(candidate) {
            return Some(rank);
        }
    }
    // Any image is still a candidate, but of last rank.
    Some(COVER_NAMES.len())
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
