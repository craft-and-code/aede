use super::*;

use super::test_support::*;

#[test]
fn a_png_palette_cannot_end_with_an_incomplete_rgb_entry() {
    let valid = indexed_png(png::BitDepth::Eight, 0, false);
    for partial in [&[0, 0, 0, 0][..], &[0, 0, 0, 0, 0][..]] {
        assert!(validate(&with_palette(&valid, partial), "png").is_err());
    }
}

#[test]
fn real_jpeg_and_png_variants_decode_without_external_helpers() {
    for bytes in [JPEG, PROGRESSIVE, CMYK, GRAYSCALE_JPEG] {
        validate(bytes, "jpg").expect("a complete JPEG");
    }
    for bytes in [PNG, PALETTE, INTERLACED, INDEXED_INTERLACED, GRAYSCALE_PNG] {
        validate(bytes, "png").expect("a complete PNG");
    }
}

#[test]
fn png_palette_indices_must_exist_while_unused_padding_bits_are_ignored() {
    assert!(
        validate(&palette_with_one_entry(INDEXED_INTERLACED), "png")
            .expect_err("invalid index in reassembled Adam7 frame")
            .contains("missing palette entry")
    );
    for (depth, invalid, valid_padding) in [
        (png::BitDepth::One, 0x80, 0x7f),
        (png::BitDepth::Two, 0x40, 0x3f),
        (png::BitDepth::Four, 0x10, 0x0f),
        (png::BitDepth::Eight, 0x01, 0x00),
    ] {
        for interlaced in [false, true] {
            assert!(
                validate(&indexed_png(depth, invalid, interlaced), "png").is_err(),
                "{depth:?}, interlaced: {interlaced}"
            );
            validate(&indexed_png(depth, valid_padding, interlaced), "png")
                .expect("unused padding is not a pixel");
        }
    }
}

#[test]
fn every_truncation_retaining_a_format_signature_is_refused() {
    for (bytes, format, signature) in [(JPEG, "jpg", 3), (PROGRESSIVE, "jpg", 3), (PNG, "png", 8)] {
        for end in signature..bytes.len() {
            assert!(
                validate(&bytes[..end], format).is_err(),
                "{format}: cut at {end}"
            );
        }
    }
}

#[test]
fn png_crc_errors_are_refused_in_image_data_and_ancillary_chunks_after_the_pixels() {
    let mut corrupt = PNG.to_vec();
    corrupt[45] ^= 1;
    assert!(validate(&corrupt, "png").is_err());

    let mut ancillary = chunk(b"tEXt", b"Comment\0valid text");
    *ancillary.last_mut().expect("CRC byte") ^= 1;
    let mut corrupt = PNG[..PNG.len() - 12].to_vec();
    corrupt.extend(ancillary);
    corrupt.extend(&PNG[PNG.len() - 12..]);
    assert!(
        validate(&corrupt, "png").is_err(),
        "ancillary CRC is not silently skipped"
    );
}

#[test]
fn a_valid_chunk_crc_does_not_hide_an_invalid_png_adler_checksum() {
    let at = PNG.windows(4).position(|b| b == b"IDAT").expect("IDAT");
    let length = u32::from_be_bytes(PNG[at - 4..at].try_into().expect("length")) as usize;
    let mut data = PNG[at + 4..at + 4 + length].to_vec();
    *data.last_mut().expect("Adler byte") ^= 1;
    let mut corrupt = PNG[..at - 4].to_vec();
    corrupt.extend(chunk(b"IDAT", &data));
    corrupt.extend(&PNG[at + 4 + length + 4..]);
    assert!(validate(&corrupt, "png").is_err());
}

#[test]
fn invalid_jpeg_huffman_tables_are_refused_even_with_a_complete_container() {
    let at = JPEG
        .windows(2)
        .position(|b| b == [0xff, 0xc4])
        .expect("DHT");
    let mut corrupt = JPEG.to_vec();
    corrupt[at + 5..at + 21].fill(255);
    assert!(validate(&corrupt, "jpg").is_err());
}

#[test]
fn dimension_and_pixel_limits_are_checked_before_decoding() {
    for (width, height) in [(0, 1), (8193, 1), (1, 8193), (4096, 4096)] {
        assert!(
            validate(&png_dimensions(width, height), "png")
                .expect_err("dimensions")
                .contains("8192")
        );
    }
    let at = JPEG
        .windows(2)
        .position(|b| b == [0xff, 0xc0])
        .expect("SOF");
    let mut huge = JPEG.to_vec();
    huge[at + 7..at + 9].copy_from_slice(&8193u16.to_be_bytes());
    assert!(
        validate(&huge, "jpg")
            .expect_err("dimensions")
            .contains("8192")
    );
}

#[test]
fn excessive_jpeg_sampling_factors_are_refused_before_internal_allocation() {
    let at = JPEG
        .windows(2)
        .position(|b| b == [0xff, 0xc0])
        .expect("SOF");
    let mut excessive = JPEG.to_vec();
    excessive[at + 11] = 0xff;
    assert!(
        validate(&excessive, "jpg")
            .expect_err("sampling")
            .contains("sampling")
    );
}

#[test]
fn animated_png_is_refused_instead_of_validating_only_the_first_frame() {
    assert!(
        validate(ANIMATED, "png")
            .expect_err("animation")
            .contains("animated")
    );
}

#[test]
fn trailing_payloads_and_multiple_images_are_refused() {
    for (bytes, format) in [(JPEG, "jpg"), (PNG, "png")] {
        let mut trailing = bytes.to_vec();
        trailing.extend(bytes);
        assert!(validate(&trailing, format).is_err());
    }
}

#[test]
fn encoded_size_and_container_count_limits_bound_the_work() {
    assert!(
        validate(&vec![0; MAX_BYTES + 1], "jpg")
            .expect_err("encoded size")
            .contains("32 MiB")
    );
    let at = JPEG
        .windows(2)
        .position(|b| b == [0xff, 0xda])
        .expect("SOS");
    let mut excessive = JPEG[..at].to_vec();
    for _ in 0..MAX_SCANS + 1 {
        excessive.extend(&JPEG[at..JPEG.len() - 2]);
    }
    excessive.extend([0xff, 0xd9]);
    assert!(
        validate(&excessive, "jpg")
            .expect_err("scans")
            .contains("64-scan")
    );

    let mut many_markers = vec![0xff, 0xd8];
    for _ in 0..MAX_PARTS {
        many_markers.extend([0xff, 0xfe, 0x00, 0x02]);
    }
    many_markers.extend(&JPEG[2..]);
    assert!(
        validate(&many_markers, "jpg")
            .expect_err("markers")
            .contains("marker-count")
    );

    let mut many_chunks = PNG[..33].to_vec();
    for _ in 0..MAX_PARTS {
        many_chunks.extend(chunk(b"tEXt", b"Comment\0"));
    }
    many_chunks.extend(&PNG[33..]);
    assert!(
        validate(&many_chunks, "png")
            .expect_err("chunks")
            .contains("chunk-count")
    );
}

#[test]
fn exhausted_time_budget_prevents_validation_and_publication() {
    for (bytes, format) in [(JPEG, "jpg"), (PNG, "png")] {
        assert!(
            validate_with_budget(bytes, format, Duration::ZERO)
                .expect_err("deadline")
                .contains("time budget")
        );
    }
}
