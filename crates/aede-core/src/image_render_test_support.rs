//! Small encoded/decoded pictures for response-rendering assertions.

/// Pixel correctness must not depend on how much CPU time a busy runner grants
/// a large debug-build resize. Deadline enforcement has its own controlled clock.
pub(super) fn render_with_frozen_clock(
    bytes: &[u8],
    size: Option<u32>,
) -> Result<crate::coverart::RenderedImage, String> {
    let elapsed = || std::time::Duration::ZERO;
    let budget = super::super::TimeBudget::new(&elapsed, super::super::TIME_BUDGET);
    super::super::render_with_budget(bytes, size, &budget)
}

pub(super) fn encode(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    pixels: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
    }
    bytes
}

pub(super) fn decode(bytes: &[u8]) -> (png::OutputInfo, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    pixels.truncate(info.buffer_size());
    (info, pixels)
}
