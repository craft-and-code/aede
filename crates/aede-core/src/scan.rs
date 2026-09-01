//! Directory traversal and parallel file reading.
//!
//! The scan is **incremental**: a file whose path, size and modification date
//! have not changed is not read again, its tags are taken from the previous
//! catalog. On a library of 50,000 titles, that is the difference between
//! several minutes and a few seconds.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use crate::analysis;
use crate::clock::{mtime_seconds, now_seconds};
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
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            threads: 0,
            follow_symlinks: false,
            skip_hidden: true,
            excluded: Vec::new(),
        }
    }
}

/// Summary of a scan.
#[derive(Debug, Default)]
pub struct ScanReport {
    /// Audio files spotted during traversal, after deduplication. Always the
    /// sum of `read` and `reused`, failures included.
    pub found: usize,
    /// Files actually read from disk.
    pub read: usize,
    /// Files taken unchanged from the previous catalog.
    pub reused: usize,
    /// Files present in the old catalog and gone since.
    pub removed: usize,
    /// Files that could not be read, as `(path, reason)` pairs sorted by path.
    /// A failure excludes the file from the catalog but never aborts the scan.
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
/// `previous` enables the incremental scan; passing `None` forces a full
/// re-read.
pub fn scan(
    roots: &[PathBuf],
    previous: Option<&Catalog>,
    options: &ScanOptions,
    mut on_progress: impl FnMut(Progress) + Send,
) -> std::io::Result<(Catalog, ScanReport)> {
    let started = Instant::now();
    let mut report = ScanReport::default();

    // --- 1. Traversal ------------------------------------------------------
    let mut walker = Walker::new(options);
    for root in roots {
        walker.walk(root)?;
    }
    let Walker {
        mut audio_files,
        reports: mut reports_found,
        folder_covers,
        sidecars,
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

    for path in &audio_files {
        let path_str = path.to_string_lossy().to_string();
        let Ok(meta) = std::fs::metadata(path) else {
            to_read.push(path.clone());
            continue;
        };
        let size = meta.len();
        let mtime = mtime_seconds(&meta);

        match cache.get(path_str.as_str()) {
            Some(old) if old.size == size && old.mtime == mtime => {
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
                    // concluded about it still holds.
                    integrity: old.integrity.clone(),
                });
            }
            _ => to_read.push(path.clone()),
        }
    }
    report.reused = reused.len();
    report.read = to_read.len();
    if let Some(prev) = previous {
        let current: HashSet<String> = audio_files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        report.removed = prev
            .files
            .iter()
            .filter(|f| !current.contains(&f.path))
            .count();
    }

    // --- 3. Parallel reading ----------------------------------------------
    let thread_count = resolve_threads(options.threads).min(to_read.len().max(1));
    let queue = Mutex::new(to_read);
    let results: Mutex<Vec<ScannedFile>> = Mutex::new(Vec::new());
    let failures: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
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
                    let meta = std::fs::metadata(&path).ok();
                    match tags::read(&path) {
                        Ok(tags) => {
                            let file = ScannedFile {
                                path: path_str,
                                size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                                mtime: meta.as_ref().map(mtime_seconds).unwrap_or(0),
                                tags,
                                folder_cover: cover_for(&folder_covers, &path),
                                sidecar: sidecar_for(&sidecars, &path),
                                // A file read again is a file that changed:
                                // any earlier verdict is about other bytes.
                                integrity: None,
                            };
                            results.lock().unwrap_or_else(|e| e.into_inner()).push(file);
                        }
                        Err(error) => {
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

    let mut scanned = results.into_inner().unwrap_or_else(|e| e.into_inner());
    report.failures = failures.into_inner().unwrap_or_else(|e| e.into_inner());
    report.failures.sort();
    scanned.extend(reused);
    on_progress(Progress::Read { done: total, total });

    // --- 4. Building the graph ---------------------------------------------
    let roots_str: Vec<String> = roots
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let mut catalog = model::build(scanned, roots_str, now_seconds());

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
        }
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
            Err(_) => return Ok(()),
        };

        let mut best_cover: Option<(usize, PathBuf)> = None;

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if self.options.skip_hidden && name.starts_with('.') {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_symlink() && !self.options.follow_symlinks {
                continue;
            }
            let is_dir = if file_type.is_symlink() {
                std::fs::metadata(&path)
                    .map(|m| m.is_dir())
                    .unwrap_or(false)
            } else {
                file_type.is_dir()
            };

            if is_dir {
                self.walk(&path)?;
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
                && best_cover.as_ref().map(|(r, _)| rank < *r).unwrap_or(true)
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

/// Preference rank of an image as cover art; `None` if it is not a usable
/// image.
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
mod tests {
    use super::*;

    #[test]
    fn cover_art_rank() {
        assert_eq!(cover_rank("cover.jpg"), Some(0));
        assert_eq!(cover_rank("folder.png"), Some(1));
        assert_eq!(cover_rank("scan-back.jpg"), Some(COVER_NAMES.len()));
        assert_eq!(cover_rank("notes.txt"), None);
        assert_eq!(cover_rank("no_extension"), None);
    }

    #[test]
    fn scan_of_a_real_folder() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let (catalog, report) = scan(&[root], None, &ScanOptions::default(), |_| {}).expect("scan");

        assert!(report.found >= 8, "files found: {}", report.found);
        assert_eq!(report.read, report.found);
        assert_eq!(report.reused, 0);
        assert!(
            report.failures.is_empty(),
            "failures: {:?}",
            report.failures
        );
        assert_eq!(catalog.files.len(), report.found);
        assert!(catalog.find_artist("Miles Davis").is_some());
    }

    #[test]
    fn second_scan_reuses_everything() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let (first, _) = scan(
            std::slice::from_ref(&root),
            None,
            &ScanOptions::default(),
            |_| {},
        )
        .unwrap();
        let (second, report) =
            scan(&[root], Some(&first), &ScanOptions::default(), |_| {}).unwrap();

        assert_eq!(report.read, 0, "no file should have been read again");
        assert_eq!(report.reused, report.found);
        // And the rebuilt catalog must be identical.
        assert_eq!(first.files.len(), second.files.len());
        assert_eq!(first.artists.len(), second.artists.len());
        assert_eq!(first.tracks.len(), second.tracks.len());
    }
}
