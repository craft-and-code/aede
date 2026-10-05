use super::*;

#[test]
fn byte_ranges_select_exact_original_regions_and_refuse_multiple_or_invalid_ranges() {
    for (value, start, length) in [
        ("bytes=0-0", 0, 1),
        ("bytes=2-4", 2, 3),
        ("bytes=7-99", 7, 3),
        ("bytes=4-", 4, 6),
        ("bytes=-3", 7, 3),
        ("bytes=-99", 0, 10),
    ] {
        assert_eq!(
            parse_range(value, 10),
            Some(ByteRange {
                start,
                length,
                partial: true
            }),
            "{value}"
        );
    }
    for value in [
        "bytes=",
        "bytes=10-",
        "bytes=-0",
        "bytes=4-2",
        "bytes=0-1,3-4",
        "bytes=0-18446744073709551616",
        "bytes=+1-2",
        "bytes=1 -2",
        "items=0-1",
    ] {
        assert!(parse_range(value, 10).is_none(), "{value}");
    }
    assert!(parse_range("bytes=0-", 0).is_none());
    assert_eq!(
        parse_range("bytes=0-18446744073709551615", u64::MAX)
            .unwrap()
            .length,
        u64::MAX
    );
}

#[test]
fn unknown_if_range_returns_the_whole_entity_and_duplicate_headers_are_refused() {
    let mut headers = HeaderMap::new();
    headers.insert(header::RANGE, "bytes=3-5".parse().unwrap());
    assert_eq!(requested_range(&headers, 10).unwrap().unwrap().length, 3);
    headers.insert(header::IF_RANGE, "\"earlier-entity\"".parse().unwrap());
    assert_eq!(
        requested_range(&headers, 10).unwrap(),
        Some(ByteRange {
            start: 0,
            length: 10,
            partial: false
        })
    );
    headers.append(header::IF_RANGE, "\"other-entity\"".parse().unwrap());
    assert!(requested_range(&headers, 10).is_err());
    headers.remove(header::IF_RANGE);
    headers.append(header::RANGE, "bytes=0-1".parse().unwrap());
    assert!(requested_range(&headers, 10).is_err());
}

#[test]
fn media_types_describe_original_containers_instead_of_requesting_transcoding() {
    for (container, codec, expected_suffix, expected_type) in [
        ("wav", "pcm", "wav", "audio/wav"),
        ("mp4", "alac", "m4a", "audio/mp4"),
        ("ogg", "opus", "ogg", "audio/ogg"),
        ("", "flac", "flac", "audio/flac"),
        ("unknown", "unknown", "bin", "application/octet-stream"),
    ] {
        let properties = AudioProperties {
            container: container.into(),
            codec: codec.into(),
            ..AudioProperties::default()
        };
        assert_eq!(suffix(&properties), expected_suffix);
        assert_eq!(content_type(&properties), expected_type);
    }
}
