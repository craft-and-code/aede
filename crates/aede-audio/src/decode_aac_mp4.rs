//! Explicit single-edit AAC-LC timeline, from ISO-BMFF mvhd/mdhd/elst/stts.
//! Apple documents why priming is decoder pre-roll, not source audio:
//! <https://developer.apple.com/documentation/quicktime-file-format/background_aac_encoding>
use std::fs::File;
use symphonia::core::{
    codecs::CodecParameters,
    errors::{Error, Result},
};
#[path = "decode_aac_mp4_boxes.rs"]
mod boxes;
use super::aac_config::FRAMES;
use boxes::{Atom, Reader, wide, word};
const UNSUPPORTED: Error =
    Error::Unsupported("AAC requires one explicit full-track edit and an unambiguous PCM timeline");

pub(super) struct Plan {
    pub delay: u64,
    pub packets: u64,
    pub media_end: u64,
}

pub(super) fn parameters(
    file: &mut File,
    input: &CodecParameters,
) -> Result<(CodecParameters, Plan)> {
    let mut r = Reader::new(file)?;
    let root = Atom {
        kind: *b"root",
        body: 0,
        end: r.length,
    };
    let movie = r.one(root, *b"moov")?;
    let movie_header = r.one(movie, *b"mvhd")?;
    let (movie_clock, _) = clock(&mut r, movie_header)?;
    let mut selected = None;
    for track in r.children(movie.body, movie.end)? {
        if track.kind != *b"trak" {
            continue;
        }
        let media = r.one(track, *b"mdia")?;
        let handler = r.one(media, *b"hdlr")?;
        if r.payload(handler, 12)?.get(8..12) != Some(b"soun") {
            continue;
        }
        let header = r.one(track, *b"tkhd")?;
        let version = r.payload(header, 1)?[0];
        let bytes = r.payload(header, if version == 1 { 36 } else { 24 })?;
        let id = match version {
            0 => word(&bytes, 12)?,
            1 => word(&bytes, 20)?,
            _ => return Err(UNSUPPORTED),
        };
        // The pinned demuxer uses ordinal IDs, not tkhd IDs. One audio track
        // makes its edit unambiguous without guessing across unrelated tracks.
        if id == 0 || selected.replace(track).is_some() {
            return Err(UNSUPPORTED);
        }
    }
    let track = selected.ok_or(UNSUPPORTED)?;
    let media = r.one(track, *b"mdia")?;
    let handler = r.one(media, *b"hdlr")?;
    if r.payload(handler, 12)?.get(8..12) != Some(b"soun") {
        return Err(UNSUPPORTED);
    }
    let media_header = r.one(media, *b"mdhd")?;
    let (media_clock, media_duration) = clock(&mut r, media_header)?;
    let rate = input.sample_rate.ok_or(UNSUPPORTED)?;
    if media_clock != rate
        || input.start_ts != 0
        || input.n_frames != Some(media_duration)
        || input
            .time_base
            .is_none_or(|t| t.numer != 1 || t.denom != rate)
    {
        return Err(UNSUPPORTED);
    }
    let edits = r.one(track, *b"edts")?;
    let edit = r.one(edits, *b"elst")?;
    let (movie_duration, delay) = single_edit(&mut r, edit)?;
    let information = r.one(media, *b"minf")?;
    let table = r.one(information, *b"stbl")?;
    let timing = r.one(table, *b"stts")?;
    let (packets, duration, last) = samples(&mut r, timing)?;
    if duration != media_duration || delay >= duration {
        return Err(UNSUPPORTED);
    }
    let raw = packets.checked_mul(FRAMES).ok_or(UNSUPPORTED)?;
    let padding = raw
        .checked_sub(duration)
        .filter(|n| *n < FRAMES)
        .ok_or(UNSUPPORTED)?;
    let playable = duration - delay;
    let exact = u128::from(playable) * u128::from(movie_clock);
    let advertised = u128::from(movie_duration) * u128::from(rate);
    // A short final sample delta supplies the exact source end even when
    // movie-time edits were rounded. A full final delta with rounded movie
    // timing cannot distinguish unknown padding from a genuinely aligned end.
    // If the source end is representable exactly in the movie clock, any
    // different edit is a real crop/extension, not quantization. Do not ignore
    // a one-frame edit just because it is within the generic one-tick bound.
    if exact != advertised
        && (last == FRAMES
            || exact.is_multiple_of(u128::from(rate))
            || exact.abs_diff(advertised) >= u128::from(rate))
    {
        return Err(UNSUPPORTED);
    }
    let mut result = input.clone();
    result.delay = Some(u32::try_from(delay).map_err(|_| UNSUPPORTED)?);
    result.padding = Some(u32::try_from(padding).map_err(|_| UNSUPPORTED)?);
    result.n_frames = Some(playable);
    Ok((
        result,
        Plan {
            delay,
            packets,
            media_end: duration,
        },
    ))
}

fn clock(r: &mut Reader<'_>, atom: Atom) -> Result<(u32, u64)> {
    let version = r.payload(atom, 1)?[0];
    let data = r.payload(atom, if version == 1 { 32 } else { 20 })?;
    let values = match version {
        0 => (word(&data, 12)?, u64::from(word(&data, 16)?)),
        1 => (word(&data, 20)?, wide(&data, 24)?),
        _ => return Err(UNSUPPORTED),
    };
    if data.get(1..4) != Some(&[0, 0, 0]) || values.0 == 0 {
        return Err(UNSUPPORTED);
    }
    Ok(values)
}
fn single_edit(r: &mut Reader<'_>, atom: Atom) -> Result<(u64, u64)> {
    let prefix = r.payload(atom, 8)?;
    if prefix.get(1..4) != Some(&[0, 0, 0]) || word(&prefix, 4)? != 1 {
        return Err(UNSUPPORTED);
    }
    let (duration, media) = match prefix[0] {
        0 => {
            let p = r.payload(atom, 20)?;
            if word(&p, 16)? != 0x00010000 {
                return Err(UNSUPPORTED);
            }
            (u64::from(word(&p, 8)?), i64::from(word(&p, 12)? as i32))
        }
        1 => {
            let p = r.payload(atom, 28)?;
            if word(&p, 24)? != 0x00010000 {
                return Err(UNSUPPORTED);
            }
            (wide(&p, 8)?, wide(&p, 16)? as i64)
        }
        _ => return Err(UNSUPPORTED),
    };
    if duration == 0 || media < 0 {
        return Err(UNSUPPORTED);
    }
    Ok((duration, media as u64))
}
fn samples(r: &mut Reader<'_>, atom: Atom) -> Result<(u64, u64, u64)> {
    let prefix = r.payload(atom, 8)?;
    let entries = word(&prefix, 4)? as usize;
    if word(&prefix, 0)? != 0 || entries == 0 || entries > 4096 {
        return Err(UNSUPPORTED);
    }
    let data = r.payload(atom, 8 + entries * 8)?;
    let (mut packets, mut duration, mut last) = (0u64, 0u64, 0);
    for i in 0..entries {
        let count = u64::from(word(&data, 8 + 8 * i)?);
        let delta = u64::from(word(&data, 12 + 8 * i)?);
        if count == 0
            || delta == 0
            || delta > FRAMES
            || (delta != FRAMES && (i + 1 != entries || count != 1))
        {
            return Err(UNSUPPORTED);
        }
        packets = packets.checked_add(count).ok_or(UNSUPPORTED)?;
        duration = duration
            .checked_add(count.checked_mul(delta).ok_or(UNSUPPORTED)?)
            .ok_or(UNSUPPORTED)?;
        last = delta;
    }
    Ok((packets, duration, last))
}

#[cfg(test)]
#[path = "decode_aac_mp4_tests.rs"]
mod tests;
