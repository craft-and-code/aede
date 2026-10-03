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
        let result = pictures::render_with_frozen_clock(&original, Some(size)).unwrap();
        let (info, pixels) = pictures::decode(&result.bytes);
        assert_eq!((info.width, info.height), (size, height));
        assert!(pixels.iter().all(|value| *value == 42));
    }
    assert_eq!(
        pictures::render_with_frozen_clock(&original, None)
            .unwrap()
            .bytes,
        original
    );
}

#[test]
fn expired_deadlines_stop_pixel_work_and_completed_image_results() {
    use std::cell::Cell;
    use std::time::Duration;

    let elapsed = Cell::new(Duration::ZERO);
    let clock = || {
        let now = elapsed.get();
        elapsed.set(now + Duration::from_secs(3));
        now
    };
    let budget = TimeBudget::new(&clock, TIME_BUDGET);
    let pixels = Pixels {
        bytes: vec![42; 3 * 3 * 3],
        width: 3,
        height: 3,
        color: png::ColorType::Rgb,
        depth: png::BitDepth::Eight,
    };
    assert!(
        resize(&pixels, 2, 3, &budget)
            .unwrap_err()
            .contains("time budget")
    );

    let elapsed = Cell::new(TIME_BUDGET - Duration::from_nanos(1));
    let clock = || elapsed.get();
    let budget = TimeBudget::new(&clock, TIME_BUDGET);
    // The same final guard covers untouched JPEG/PNG originals and encoded PNG
    // thumbnails; it must refuse a result exactly at the deadline.
    let thumbnail = pictures::encode(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[42; 3]);
    for (bytes, format) in [
        (originals::JPEG, "jpg"),
        (originals::PNG, "png"),
        (thumbnail.as_slice(), "png"),
    ] {
        elapsed.set(TIME_BUDGET - Duration::from_nanos(1));
        let result = finish(bytes.to_vec(), format, &budget).unwrap();
        assert_eq!(result.bytes, bytes);
        assert_eq!(result.format, format);
        elapsed.set(TIME_BUDGET);
        assert!(
            finish(bytes.to_vec(), format, &budget)
                .unwrap_err()
                .contains("time budget")
        );
    }
    for bytes in [originals::JPEG, originals::PNG] {
        for size in [None, Some(2048), Some(1)] {
            assert!(
                render_with_budget(bytes, size, &budget)
                    .unwrap_err()
                    .contains("time budget")
            );
        }
    }

    for (bytes, size) in [
        (originals::JPEG, None),
        (originals::PNG, None),
        (originals::PNG, Some(2048)),
    ] {
        // Derive the validation boundary rather than relying on codec-specific
        // read counts. The next check expires while returning an original.
        let checks = Cell::new(0);
        let clock = || {
            checks.set(checks.get() + 1);
            Duration::ZERO
        };
        let budget = TimeBudget::new(&clock, TIME_BUDGET);
        super::super::validate_with_deadline(bytes, image_kind(bytes).unwrap(), &budget).unwrap();
        let validation_checks = checks.replace(0);
        let clock = || {
            checks.set(checks.get() + 1);
            if checks.get() <= validation_checks {
                Duration::ZERO
            } else {
                TIME_BUDGET
            }
        };
        let budget = TimeBudget::new(&clock, TIME_BUDGET);
        assert!(
            render_with_budget(bytes, size, &budget)
                .unwrap_err()
                .contains("time budget")
        );
    }
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
