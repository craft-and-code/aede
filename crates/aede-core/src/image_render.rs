//! Response-only thumbnails; downloaded and stored originals stay untouched.

use super::{MAX_OUTPUT, TIME_BUDGET, TimedCursor, check_time};
use crate::coverart::{RenderedImage, image_kind};
use std::time::Instant;

struct Pixels {
    bytes: Vec<u8>,
    width: usize,
    height: usize,
    color: png::ColorType,
    depth: png::BitDepth,
}

pub(crate) fn render(bytes: &[u8], size: Option<u32>) -> Result<RenderedImage, String> {
    if size.is_some_and(|size| !(1..=2048).contains(&size)) {
        return Err("thumbnail size must be between 1 and 2048 pixels".into());
    }
    let format = image_kind(bytes).ok_or("image is not a JPEG or PNG")?;
    let started = Instant::now();
    // Keep the proven complete-container, palette and checksum checks before
    // response rendering. EXPAND alone can conceal malformed palette indices.
    super::validate(bytes, format)?;
    check_time(started, TIME_BUDGET)?;
    let Some(size) = size else {
        return Ok(RenderedImage {
            bytes: bytes.to_vec(),
            format,
        });
    };
    let (width, height) = dimensions(bytes, format, started)?;
    if width <= size as usize && height <= size as usize {
        return Ok(RenderedImage {
            bytes: bytes.to_vec(),
            format,
        });
    }
    let pixels = match format {
        "png" => decode_png(bytes, started)?,
        "jpg" => decode_jpeg(bytes, started)?,
        _ => return Err("unsupported image format".into()),
    };
    let longest = width.max(height);
    let target_width = (width * size as usize / longest).max(1);
    let target_height = (height * size as usize / longest).max(1);
    let resized = resize(&pixels, target_width, target_height, started)?;
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, target_width as u32, target_height as u32);
        encoder.set_color(pixels.color);
        encoder.set_depth(pixels.depth);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("cannot encode thumbnail: {e}"))?;
        writer
            .write_image_data(&resized)
            .map_err(|e| format!("cannot encode thumbnail: {e}"))?;
        writer
            .finish()
            .map_err(|e| format!("cannot finish thumbnail: {e}"))?;
    }
    check_time(started, TIME_BUDGET)?;
    if output.len() > super::MAX_BYTES {
        return Err("thumbnail exceeds the 32 MiB image limit".into());
    }
    Ok(RenderedImage {
        bytes: output,
        format: "png",
    })
}

fn dimensions(bytes: &[u8], format: &str, started: Instant) -> Result<(usize, usize), String> {
    match format {
        "png" => {
            let header = bytes.get(16..24).ok_or("missing PNG dimensions")?;
            let width = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
            let height = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
            Ok((width, height))
        }
        "jpg" => {
            let mut decoder = jpeg_decoder(bytes, started);
            decoder
                .decode_headers()
                .map_err(|e| format!("invalid JPEG: {e}"))?;
            decoder
                .dimensions()
                .ok_or_else(|| "missing JPEG dimensions".into())
        }
        _ => Err("unsupported image format".into()),
    }
}

fn buffer(size: usize) -> Result<Vec<u8>, String> {
    if size > MAX_OUTPUT {
        return Err("image exceeds the decoded-output budget".into());
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| "cannot allocate image buffer")?;
    bytes.resize(size, 0);
    Ok(bytes)
}

fn decode_png(bytes: &[u8], started: Instant) -> Result<Pixels, String> {
    let mut options = png::DecodeOptions::default();
    options.set_ignore_checksums(false);
    options.set_skip_ancillary_crc_failures(false);
    let mut decoder =
        png::Decoder::new_with_options(TimedCursor::new(bytes, started, TIME_BUDGET), options);
    decoder.set_limits(png::Limits {
        bytes: 16 * 1024 * 1024,
    });
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    // Expand palette/packed grayscale, while retaining 16-bit sample depth.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("invalid PNG: {e}"))?;
    let mut pixels = buffer(
        reader
            .output_buffer_size()
            .ok_or("PNG output size overflow")?,
    )?;
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|e| format!("invalid PNG: {e}"))?;
    pixels.truncate(info.buffer_size());
    reader.finish().map_err(|e| format!("invalid PNG: {e}"))?;
    check_time(started, TIME_BUDGET)?;
    Ok(Pixels {
        bytes: pixels,
        width: info.width as usize,
        height: info.height as usize,
        color: info.color_type,
        depth: info.bit_depth,
    })
}

fn jpeg_decoder(bytes: &[u8], started: Instant) -> zune_jpeg::JpegDecoder<TimedCursor<'_>> {
    use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
    let options = DecoderOptions::default()
        .set_strict_mode(true)
        .set_use_unsafe(false)
        .set_max_width(super::MAX_AXIS)
        .set_max_height(super::MAX_AXIS)
        .jpeg_set_max_scans(super::MAX_SCANS)
        .jpeg_set_out_colorspace(ColorSpace::RGB);
    zune_jpeg::JpegDecoder::new_with_options(TimedCursor::new(bytes, started, TIME_BUDGET), options)
}

fn decode_jpeg(bytes: &[u8], started: Instant) -> Result<Pixels, String> {
    let mut decoder = jpeg_decoder(bytes, started);
    decoder
        .decode_headers()
        .map_err(|e| format!("invalid JPEG: {e}"))?;
    let (width, height) = decoder.dimensions().ok_or("missing JPEG dimensions")?;
    let mut pixels = buffer(
        decoder
            .output_buffer_size()
            .ok_or("JPEG output size overflow")?,
    )?;
    decoder
        .decode_into(&mut pixels)
        .map_err(|e| format!("invalid JPEG: {e}"))?;
    check_time(started, TIME_BUDGET)?;
    Ok(Pixels {
        bytes: pixels,
        width,
        height,
        color: png::ColorType::Rgb,
        depth: png::BitDepth::Eight,
    })
}

fn resize(
    source: &Pixels,
    width: usize,
    height: usize,
    started: Instant,
) -> Result<Vec<u8>, String> {
    let channels = source.color.samples();
    let sample_bytes = match source.depth {
        png::BitDepth::Eight => 1,
        png::BitDepth::Sixteen => 2,
        _ => return Err("unexpected packed thumbnail pixels".into()),
    };
    let stride = channels * sample_bytes;
    let size = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(stride))
        .ok_or("thumbnail size overflow")?;
    let mut output = buffer(size)?;
    let alpha = matches!(
        source.color,
        png::ColorType::Rgba | png::ColorType::GrayscaleAlpha
    );
    for y in 0..height {
        check_time(started, TIME_BUDGET)?;
        let (y0, y1, fy) = axis(y, height, source.height);
        for x in 0..width {
            let (x0, x1, fx) = axis(x, width, source.width);
            let neighbors = [
                (x0, y0, (1.0 - fx) * (1.0 - fy)),
                (x1, y0, fx * (1.0 - fy)),
                (x0, y1, (1.0 - fx) * fy),
                (x1, y1, fx * fy),
            ];
            let mut alpha_sum = if alpha { 0.0 } else { 1.0 };
            if alpha {
                for (x, y, weight) in neighbors {
                    alpha_sum += sample(source, x, y, channels - 1, sample_bytes)? * weight;
                }
            }
            for channel in 0..channels {
                let mut total = 0.0;
                for (x, y, weight) in neighbors {
                    let mut value = sample(source, x, y, channel, sample_bytes)?;
                    if alpha && channel != channels - 1 {
                        value *= sample(source, x, y, channels - 1, sample_bytes)?;
                    }
                    total += value * weight;
                }
                if alpha && channel != channels - 1 {
                    total = if alpha_sum > 0.0 {
                        total / alpha_sum
                    } else {
                        0.0
                    };
                }
                let value = total.round() as u16;
                let at = ((y * width + x) * channels + channel) * sample_bytes;
                if sample_bytes == 1 {
                    output[at] = value as u8;
                } else {
                    output[at..at + 2].copy_from_slice(&value.to_be_bytes());
                }
            }
        }
    }
    Ok(output)
}

fn axis(at: usize, target: usize, source: usize) -> (usize, usize, f64) {
    let position = ((at as f64 + 0.5) * source as f64 / target as f64 - 0.5).max(0.0);
    let first = (position.floor() as usize).min(source - 1);
    (first, (first + 1).min(source - 1), position.fract())
}

fn sample(
    source: &Pixels,
    x: usize,
    y: usize,
    channel: usize,
    sample_bytes: usize,
) -> Result<f64, String> {
    let at = ((y * source.width + x) * source.color.samples() + channel) * sample_bytes;
    let bytes = source
        .bytes
        .get(at..at + sample_bytes)
        .ok_or("thumbnail pixels are truncated")?;
    Ok(if sample_bytes == 1 {
        f64::from(bytes[0])
    } else {
        f64::from(u16::from_be_bytes([bytes[0], bytes[1]]))
    })
}

#[cfg(test)]
#[path = "image_render_tests.rs"]
mod tests;
