pub(super) const JPEG: &[u8] = include_bytes!("../tests/fixtures/images/baseline.jpg");
pub(super) const PROGRESSIVE: &[u8] = include_bytes!("../tests/fixtures/images/progressive.jpg");
pub(super) const CMYK: &[u8] = include_bytes!("../tests/fixtures/images/cmyk.jpg");
pub(super) const GRAYSCALE_JPEG: &[u8] = include_bytes!("../tests/fixtures/images/grayscale.jpg");
pub(super) const PNG: &[u8] = include_bytes!("../tests/fixtures/images/rgba.png");
pub(super) const PALETTE: &[u8] = include_bytes!("../tests/fixtures/images/palette.png");
pub(super) const INTERLACED: &[u8] = include_bytes!("../tests/fixtures/images/interlaced.png");
pub(super) const INDEXED_INTERLACED: &[u8] =
    include_bytes!("../tests/fixtures/images/indexed-interlaced.png");
pub(super) const GRAYSCALE_PNG: &[u8] = include_bytes!("../tests/fixtures/images/grayscale16.png");
pub(super) const ANIMATED: &[u8] = include_bytes!("../tests/fixtures/images/animated.png");

pub(super) fn crc(bytes: &[u8]) -> [u8; 4] {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    (!crc).to_be_bytes()
}

pub(super) fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend(kind);
    chunk.extend(data);
    chunk.extend(crc(&chunk[4..]));
    chunk
}

pub(super) fn png_dimensions(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = PNG.to_vec();
    bytes[16..20].copy_from_slice(&width.to_be_bytes());
    bytes[20..24].copy_from_slice(&height.to_be_bytes());
    let checksum = crc(&bytes[12..29]);
    bytes[29..33].copy_from_slice(&checksum);
    bytes
}

pub(super) fn indexed_png(depth: png::BitDepth, pixel: u8, interlaced: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(depth);
        encoder.set_palette(vec![0, 0, 0]);
        let mut writer = encoder.write_header().expect("indexed header");
        writer.write_image_data(&[pixel]).expect("encoded pixel");
    }
    if interlaced {
        // One pixel is present in Adam7's first pass, with the same raw row.
        bytes[28] = 1;
        let checksum = crc(&bytes[12..29]);
        bytes[29..33].copy_from_slice(&checksum);
    }
    bytes
}

pub(super) fn palette_with_one_entry(bytes: &[u8]) -> Vec<u8> {
    with_palette(bytes, &[0, 0, 0])
}

pub(super) fn with_palette(bytes: &[u8], palette: &[u8]) -> Vec<u8> {
    let mut at = 8;
    loop {
        let length = u32::from_be_bytes(bytes[at..at + 4].try_into().expect("length")) as usize;
        if &bytes[at + 4..at + 8] == b"PLTE" {
            let mut result = bytes[..at].to_vec();
            result.extend(chunk(b"PLTE", palette));
            result.extend(&bytes[at + 12 + length..]);
            return result;
        }
        at += length + 12;
    }
}
