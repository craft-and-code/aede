//! Deep analyses produced by another tool, imported into the catalog.
//!
//! Aède describes what a file *contains* by reading its structure; it does not
//! decode. A decoder answers questions no structural read can: is this FLAC a
//! re-encoded MP3, was it upsampled, where does its spectrum stop, how loud is
//! it really, and — the decisive one — does the audio still match the MD5 the
//! encoder wrote in STREAMINFO.
//!
//! [FlacCompagnon](https://github.com/kcell/FlacCompagnon) already does that
//! pass and can export it. Rather than wait for the decoder that arrives at M3,
//! a user who has run it can hand the results over.
//!
//! **Imported values are never merged into Aède's own.** They sit beside them,
//! attributed to their source, because a verdict carries the method that
//! produced it: overwriting `effective_bit_depth` — read from the wasted-bits
//! counts of the frames — with a figure obtained by decoding would leave the
//! catalog unable to say where the number came from, and unable to notice that
//! the two disagree. Noticing is the point.

use std::path::Path;

use crate::json::{self, Json};

/// The report format this reader understands.
pub const FLACCOMPAGNON_FORMAT: &str = "flaccompagnon-report";

/// One file, as another tool measured it.
///
/// Every measurement is optional: a report may predate a field, omit one, or
/// come from a tool that does not compute it. Nothing here is required for the
/// catalog to work — this is entirely additional.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileAnalysis {
    /// Absolute path of the file this describes.
    ///
    /// The key is the **path**, not an identifier into the catalog. Two
    /// reasons, and the second is the important one. Identifiers are positions
    /// that a scan renumbers, so an identifier would have to be remapped after
    /// every scan. And an analysis may perfectly well describe a file the
    /// catalog does not hold *yet*: someone can run the analysis first and
    /// build their library afterwards. Keying on the path lets such a record
    /// wait, and attach itself the day the file is scanned.
    pub path: String,
    /// Tool that produced the measurements, `flaccompagnon` for now.
    pub source: String,
    /// Version of the report format, kept so an old import stays readable.
    pub source_version: u32,
    /// When it was imported, in seconds since the Unix epoch.
    pub imported_at: u64,
    /// Size the analysed file had, in bytes.
    ///
    /// With [`FileAnalysis::modified_unix`], this is what makes an imported
    /// analysis expire: the same key the incremental scan uses. A file edited
    /// after the report was written is no longer the file that was measured.
    pub size_bytes: u64,
    /// Modification date the analysed file had.
    pub modified_unix: u64,

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
        match self.path.rfind('/') {
            Some(i) => &self.path[i + 1..],
            None => &self.path,
        }
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
    let value = json::parse(&text).map_err(|e| ImportError::Json(e.to_string()))?;

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
mod tests {
    use super::*;

    /// A report holding one file, with the fields a real one carries.
    fn example(extra: &str) -> String {
        format!(
            r#"{{
              "format": "flaccompagnon-report",
              "version": 1,
              "report": {{
                "root": "/music/Danzig",
                "files": [{{
                  "path": "/music/Danzig/01 7th House.flac",
                  "file_name": "01 7th House.flac",
                  "size_bytes": 33551356,
                  "modified_unix": 1782122103,
                  "ext_mismatch": false,
                  "detections": {{
                    "upscaling": false,
                    "upsampling": false,
                    "transcoding": "none",
                    "summary": "Clean",
                    "detail": "Clean — full-band content to ~22.1 kHz."
                  }},
                  "cutoff_hz": 22050.0,
                  "cutoff_ratio": 1.0,
                  "real_bit_depth": 16,
                  "requant_rate": 0.12820514,
                  "fake_stereo": false,
                  "clipping": {{
                    "clipped_samples": 0,
                    "clip_events": 0,
                    "peak_dbfs": -0.13382691,
                    "true_peak_dbtp": 0.28483155,
                    "clipped": false
                  }},
                  "dr_db": 9.345364,
                  "flac_md5": {{ "state": "Match" }},
                  "error": null
                  {extra}
                }}]
              }}
            }}"#
        )
    }

    fn write(name: &str, text: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, text).expect("writing the report");
        path
    }

    #[test]
    fn reads_a_report() {
        let path = write("aede_report_ok.json", &example(""));
        let report = read_report(&path).expect("a readable report");
        std::fs::remove_file(&path).ok();

        assert_eq!(report.root, "/music/Danzig");
        assert_eq!(report.version, 1);
        assert_eq!(report.files.len(), 1);

        let a = &report.files[0];
        assert_eq!(a.path, "/music/Danzig/01 7th House.flac");
        assert_eq!(a.file_name(), "01 7th House.flac");
        assert_eq!(a.source, "flaccompagnon");
        assert_eq!(a.size_bytes, 33_551_356);
        assert_eq!(a.md5_state.as_deref(), Some("Match"));
        assert_eq!(a.real_bit_depth, Some(16));
        assert_eq!(a.cutoff_hz, Some(22_050.0));
        assert_eq!(a.transcoding.as_deref(), Some("none"));
        assert_eq!(a.dr_db, Some(9.345_364));
        // Zero measured is not the same as nothing measured, and a peak below
        // zero has to survive the sign.
        assert_eq!(a.clipped_samples, Some(0));
        assert_eq!(a.clipped, Some(false));
        assert!(a.peak_dbfs.unwrap() < 0.0);
        assert!(!a.md5_failed());
        assert!(!a.suspect_encoding());
        // Nothing was measured about this, and the record says so rather than
        // guessing a default.
        assert_eq!(a.error, None);
    }

    #[test]
    fn a_measurement_the_reader_does_not_know_is_not_an_error() {
        // The other tool will grow fields; a report carrying one still imports.
        let path = write(
            "aede_report_new_field.json",
            &example(", \"loudness_lufs\": -9.4"),
        );
        let report = read_report(&path).expect("unknown fields are ignored");
        std::fs::remove_file(&path).ok();
        assert_eq!(report.files.len(), 1);
    }

    #[test]
    fn another_tools_json_is_refused_by_name() {
        let path = write(
            "aede_report_foreign.json",
            r#"{"format": "something-else"}"#,
        );
        let error = read_report(&path).expect_err("not a report");
        std::fs::remove_file(&path).ok();
        match error {
            ImportError::NotAReport { found } => assert_eq!(found, "something-else"),
            other => panic!("wrong error: {other:?}"),
        }
    }

    #[test]
    fn a_report_is_recognised_without_being_parsed() {
        // A scan walks past every file in the library; it must be able to tell
        // a report from any other .json without reading the whole of it.
        let report = write("aede_sniff_report.json", &example(""));
        let other = write("aede_sniff_other.json", r#"{"hello": "world"}"#);
        let padded = write(
            "aede_sniff_padded.json",
            &format!("{}{}", " ".repeat(SNIFF_BYTES * 2), example("")),
        );
        assert!(looks_like_a_report(&report));
        assert!(!looks_like_a_report(&other), "someone else's JSON");
        assert!(
            !looks_like_a_report(&padded),
            "the marker has to be near the head, or the sniff is not cheap"
        );
        assert!(
            !looks_like_a_report(Path::new("/nowhere/at/all.json")),
            "an unreadable file is not a report"
        );
        for path in [report, other, padded] {
            std::fs::remove_file(&path).ok();
        }
    }

    #[test]
    fn a_report_from_a_later_tool_is_refused_rather_than_half_read() {
        let text = example("").replace("\"version\": 1", "\"version\": 99");
        let path = write("aede_report_future.json", &text);
        let error = read_report(&path).expect_err("too new");
        std::fs::remove_file(&path).ok();
        assert!(matches!(error, ImportError::Version { found: 99 }));
    }

    #[test]
    fn an_analysis_expires_with_the_bytes_it_describes() {
        let a = FileAnalysis {
            size_bytes: 100,
            modified_unix: 10,
            ..FileAnalysis::default()
        };
        assert!(a.still_applies(100, 10));
        assert!(!a.still_applies(101, 10), "re-encoded");
        assert!(!a.still_applies(100, 11), "re-tagged");
    }

    #[test]
    fn a_lossy_ancestry_is_suspect_however_it_was_found() {
        let detected = FileAnalysis {
            transcoding: Some("detected".into()),
            ..FileAnalysis::default()
        };
        let suspected = FileAnalysis {
            transcoding: Some("suspected".into()),
            ..FileAnalysis::default()
        };
        let upscaled = FileAnalysis {
            upscaling: Some(true),
            ..FileAnalysis::default()
        };
        let clean = FileAnalysis {
            transcoding: Some("none".into()),
            upscaling: Some(false),
            upsampling: Some(false),
            ..FileAnalysis::default()
        };
        assert!(detected.suspect_encoding());
        assert!(suspected.suspect_encoding());
        assert!(upscaled.suspect_encoding());
        assert!(!clean.suspect_encoding());
        // Nothing measured is not a suspicion.
        assert!(!FileAnalysis::default().suspect_encoding());
    }
}
