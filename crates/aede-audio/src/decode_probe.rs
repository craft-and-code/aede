//! Protected regular-file probing shared by playback and content analysis.
use super::DecodeError;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    panic::{AssertUnwindSafe, catch_unwind},
};
use symphonia::core::{
    codecs::{CODEC_TYPE_AAC, CodecParameters},
    formats::{FormatOptions, FormatReader},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

/// A retained container and the admitted properties of its selected track.
pub struct ProbedFile {
    /// Reader positioned before the first encoded packet.
    pub format: Box<dyn FormatReader>,
    /// Selected track identity from that same reader.
    pub track_id: u32,
    /// Source codec properties, including an admitted AAC MP4 edit window.
    pub params: CodecParameters,
}
/// Probe an opened regular file. Gapless mode admits explicit AAC MP4 edits;
/// header-only callers retain unqualified metadata without constructing a codec.
/// AAC metadata inspection uses a duplicate descriptor and restores its shared
/// OS cursor before the buffered container resumes. Audio bytes remain untouched.
pub fn probe_file(
    file: File,
    hint: &Hint,
    options: &FormatOptions,
) -> Result<ProbedFile, DecodeError> {
    if !file.metadata().map_err(error)?.is_file() {
        return Err(error("audio source is not a regular file"));
    }
    let keeper = file.try_clone();
    let input = super::source::prepare(file)?;
    let probed = catch_unwind(AssertUnwindSafe(|| {
        super::source::probe().format(
            hint,
            MediaSourceStream::new(input, Default::default()),
            options,
            &MetadataOptions::default(),
        )
    }))
    .map_err(|_| error("audio probe panicked"))?
    .map_err(error)?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| error("no default track"))?;
    let track_id = track.id;
    let original = track.codec_params.clone();
    let mut params = if options.enable_gapless {
        super::codec_parameters(&original).map_err(error)?
    } else {
        original.clone()
    };
    if options.enable_gapless && original.codec == CODEC_TYPE_AAC && original.extra_data.is_some() {
        let mut keeper = keeper.map_err(error)?;
        let saved = keeper.stream_position().map_err(error)?;
        let configured = catch_unwind(AssertUnwindSafe(|| {
            keeper.seek(SeekFrom::Start(0))?;
            let mut header = [0; 8];
            keeper.read_exact(&mut header)?;
            let mp4 = matches!(
                &header[4..8],
                b"ftyp" | b"moov" | b"mdat" | b"free" | b"wide" | b"skip"
            );
            if mp4 {
                let codec = super::aac_config::parameters(&original)?;
                super::aac_mp4::parameters(&mut keeper, &codec).map(Some)
            } else if original.channels.is_none() {
                Err(symphonia::core::errors::Error::Unsupported(
                    "AAC container timing is not admitted",
                ))
            } else {
                Ok(None)
            }
        }))
        .map_err(|_| error("AAC metadata probe panicked"));
        keeper.seek(SeekFrom::Start(saved)).map_err(error)?;
        if let Some((configured, plan)) = configured?.map_err(error)? {
            params = configured;
            format = Box::new(
                super::aac_packets::EditedReader::new(format, track_id, params.clone(), plan)
                    .map_err(error)?,
            );
        }
    }
    Ok(ProbedFile {
        format,
        track_id,
        params,
    })
}
fn error(error: impl std::fmt::Display) -> DecodeError {
    DecodeError(error.to_string())
}
