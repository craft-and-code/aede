use super::*;

use super::super::test_support as originals;
#[path = "image_render_test_support.rs"]
mod pictures;

#[test]
fn originals_are_returned_exactly_without_size_or_when_already_small_enough() {
    for bytes in [
        originals::JPEG,
        originals::PNG,
        originals::GRAYSCALE_PNG,
        originals::CMYK,
    ] {
        for size in [None, Some(2048)] {
            let result = render(bytes, size).unwrap();
            assert_eq!(result.bytes, bytes);
            assert_eq!(result.format, image_kind(bytes).unwrap());
        }
    }
}

#[test]
fn real_jpeg_png_palette_interlace_and_sample_depths_produce_valid_png_thumbnails() {
    for bytes in [
        originals::JPEG,
        originals::PROGRESSIVE,
        originals::CMYK,
        originals::GRAYSCALE_JPEG,
        originals::PNG,
        originals::PALETTE,
        originals::INTERLACED,
        originals::INDEXED_INTERLACED,
        originals::GRAYSCALE_PNG,
    ] {
        let result = render(bytes, Some(1)).unwrap();
        assert_eq!(result.format, "png");
        super::super::validate(&result.bytes, "png").unwrap();
        let (info, _) = pictures::decode(&result.bytes);
        assert_eq!((info.width, info.height), (1, 1));
    }
}

#[test]
fn thumbnails_keep_aspect_ratio_alpha_edges_and_sixteen_bit_samples() {
    let pixels = [255, 0, 0, 0, 0, 0, 255, 255];
    let original = pictures::encode(2, 1, png::ColorType::Rgba, png::BitDepth::Eight, &pixels);
    let result = render(&original, Some(1)).unwrap();
    let (info, pixels) = pictures::decode(&result.bytes);
    assert_eq!((info.width, info.height), (1, 1));
    assert_eq!(
        pixels,
        [0, 0, 255, 128],
        "transparent red must not bleed into opaque blue"
    );

    let original = pictures::encode(
        2,
        1,
        png::ColorType::Grayscale,
        png::BitDepth::Sixteen,
        &[0x12, 0x34, 0x56, 0x78],
    );
    let result = render(&original, Some(1)).unwrap();
    let (info, pixels) = pictures::decode(&result.bytes);
    assert_eq!(info.bit_depth, png::BitDepth::Sixteen);
    assert_eq!(pixels, [0x34, 0x56]);

    let original = pictures::encode(4, 2, png::ColorType::Rgb, png::BitDepth::Eight, &[42; 24]);
    let result = render(&original, Some(2)).unwrap();
    let (info, pixels) = pictures::decode(&result.bytes);
    assert_eq!((info.width, info.height), (2, 1));
    assert_eq!(pixels, [42; 6]);

    let original = pictures::encode(
        3840,
        2160,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        &vec![42; 3840 * 2160 * 3],
    );
    for (size, height) in [(256, 144), (2048, 1152)] {
        let result = render(&original, Some(size)).unwrap();
        let (info, pixels) = pictures::decode(&result.bytes);
        assert_eq!((info.width, info.height), (size, height));
        assert!(pixels.iter().all(|value| *value == 42));
    }
    assert_eq!(render(&original, None).unwrap().bytes, original);
}

#[test]
fn rendering_refuses_invalid_sizes_and_complete_container_failures_before_output() {
    for size in [0, 2049, u32::MAX] {
        assert!(render(originals::PNG, Some(size)).is_err());
    }
    for bytes in [originals::PNG, originals::JPEG] {
        assert!(render(&bytes[..bytes.len() - 1], Some(1)).is_err());
        let mut appended = bytes.to_vec();
        appended.push(0);
        assert!(render(&appended, Some(1)).is_err());
    }
    assert!(render(originals::ANIMATED, Some(1)).is_err());
    assert!(render(b"<svg>not a permitted container</svg>", Some(1)).is_err());
    let invalid = originals::palette_with_one_entry(originals::INDEXED_INTERLACED);
    assert!(render(&invalid, Some(1)).is_err());
    let invalid = originals::indexed_png(png::BitDepth::Eight, 1, false);
    assert!(render(&invalid, Some(1)).is_err());
    let oversized = originals::png_dimensions(4097, 4097);
    assert!(render(&oversized, Some(1)).is_err());
}
