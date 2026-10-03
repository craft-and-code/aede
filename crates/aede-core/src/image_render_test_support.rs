//! Small encoded/decoded pictures for response-rendering assertions.

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
