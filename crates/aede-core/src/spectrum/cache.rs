//! Versioned source identity inside the PNG, published atomically with its pixels.
use png::text_metadata::{EncodableTextChunk, TEXtChunk};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path, time::UNIX_EPOCH};
const KEY: &str = "AedeSpectrum";
const GENERATION: &str = "rust-spectrum-v1-dsd-wideband-v1-opus-v2-alac-config-v5-aac-edit-v2-native-extra-v3-green-standard-v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Input {
    size: u64,
    before_epoch: bool,
    seconds: u64,
    nanos: u32,
}
impl Input {
    pub fn read(path: &Path) -> Result<Self, String> {
        let meta = fs::metadata(path).map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("spectrogram source is not a regular file".into());
        }
        let (before_epoch, time) = match meta
            .modified()
            .map_err(|e| e.to_string())?
            .duration_since(UNIX_EPOCH)
        {
            Ok(time) => (false, time),
            Err(e) => (true, e.duration()),
        };
        Ok(Self {
            size: meta.len(),
            before_epoch,
            seconds: time.as_secs(),
            nanos: time.subsec_nanos(),
        })
    }
    pub fn unchanged(&self, path: &Path) -> Result<(), String> {
        if &Self::read(path)? != self {
            return Err("audio source changed during spectrogram generation".into());
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
struct Stamp {
    generation: String,
    #[serde(default)]
    decoder_provenance: String,
    input: Input,
    decoder: String,
}
pub(super) fn stamp(mut png: Vec<u8>, input: Input, decoder: &str) -> Result<Vec<u8>, String> {
    // Only the trusted renderer's output reaches this function. Put the stamp
    // after IHDR so freshness reads never traverse compressed image data.
    if png.get(..8) != Some(b"\x89PNG\r\n\x1a\n")
        || png.get(12..16) != Some(b"IHDR")
        || png.len() < 33
    {
        return Err("renderer did not return a PNG header".into());
    }
    let suffix = png.split_off(33);
    let record = serde_json::to_string(&Stamp {
        generation: GENERATION.into(),
        decoder_provenance: crate::PCM_DECODER_PROVENANCE.into(),
        input,
        decoder: decoder.into(),
    })
    .map_err(|e| e.to_string())?;
    TEXtChunk::new(KEY, record)
        .encode(&mut png)
        .map_err(|e| e.to_string())?;
    png.extend(suffix);
    Ok(png)
}
pub(super) fn current(audio: &Path, picture: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(picture) else {
        return false;
    };
    if !meta.is_file() || meta.file_type().is_symlink() {
        return false;
    }
    let Ok(input) = Input::read(audio) else {
        return false;
    };
    let Ok(file) = fs::File::open(picture) else {
        return false;
    };
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file.take(8192)));
    decoder.set_limits(png::Limits { bytes: 32 * 1024 });
    let Ok(reader) = decoder.read_info() else {
        return false;
    };
    let records: Vec<_> = reader
        .info()
        .uncompressed_latin1_text
        .iter()
        .filter(|t| t.keyword == KEY)
        .collect();
    if records.len() != 1 {
        return false;
    }
    let Ok(record) = serde_json::from_str::<Stamp>(&records[0].text) else {
        return false;
    };
    record.generation == GENERATION
        && record.decoder_provenance == crate::PCM_DECODER_PROVENANCE
        && record.input == input
}
