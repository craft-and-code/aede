//! Cached source integrity evidence; MD5 and container CRC must remain distinguishable.
use aede_core::{analysis::FileAnalysis, audit::integrity::Verdict, model::AudioFile};
use serde::Serialize;
#[derive(Default, Serialize)]
pub(super) struct Counts {
    pub ok: usize,
    pub ko: usize,
    pub no_hash: usize,
    pub unchecked: usize,
    pub unsupported: usize,
    pub stale: usize,
    pub md5: usize,
    pub crc: usize,
}
impl Counts {
    pub fn add(&mut self, file: &AudioFile, records: Option<&Vec<&FileAnalysis>>) {
        if file.properties.codec == "flac" {
            self.md5 += 1;
            let current = records.and_then(|rows| {
                rows.iter()
                    .filter(|r| r.still_applies(file.size, file.mtime))
                    .max_by_key(|r| (r.result_at_ns, r.imported_at))
            });
            match current.and_then(|r| r.md5_state.as_deref()) {
                Some("Match") => self.ok += 1,
                Some("Mismatch") => self.ko += 1,
                Some("NoSignature") => self.no_hash += 1,
                _ if current.is_none() && records.is_some() => self.stale += 1,
                _ => self.unchecked += 1,
            }
        } else if file.properties.container == "ogg"
            || matches!(file.properties.codec.as_str(), "vorbis" | "opus")
        {
            self.crc += 1;
            match file
                .integrity
                .as_ref()
                .filter(|r| r.method == "ogg-page-crc")
                .map(|r| &r.verdict)
            {
                Some(Verdict::Intact) => self.ok += 1,
                Some(Verdict::Damaged { .. }) => self.ko += 1,
                _ => self.unchecked += 1,
            }
        } else {
            self.unsupported += 1;
        }
    }
}
