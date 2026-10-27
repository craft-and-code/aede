use super::tests::{Scratch, fixture};
use super::*;
const TOTAL: u64 = 882000;
fn source() -> Vec<u8> {
    fs::read(fixture("silence-3970.ape")).unwrap()
}
fn zeroes(path: &Path) {
    let mut d = Decoder::open(path).unwrap().unwrap();
    assert_eq!(d.info().frames, Some(TOTAL));
    assert_eq!(d.info().bits, Some(16));
    let mut count = 0;
    while let Some(pcm) = d.next_chunk().unwrap() {
        assert!(pcm.len() <= 1_048_576);
        assert!(pcm.iter().all(|v| *v == 0.0));
        count += pcm.len() as u64 / 2;
    }
    assert_eq!(count, TOTAL);
}
#[test]
fn old_containers_decode_complete_crc_checked_frames() {
    for version in [3950, 3970] {
        zeroes(&fixture(&format!("silence-{version}.ape")));
    }
}
#[test]
fn legacy_optional_fields_wrappers_and_tags_keep_original_pcm() {
    let dir = Scratch::new("legacy-tags");
    let original = source();
    // Peak + explicit seek count + stored WAV header + terminating WAV bytes.
    let mut b = original[..32].to_vec();
    b[8..10].copy_from_slice(&22u16.to_le_bytes());
    b[16..20].copy_from_slice(&44u32.to_le_bytes());
    b[20..24].copy_from_slice(&12u32.to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(&3u32.to_le_bytes());
    b.extend_from_slice(&[0; 44]);
    for chunk in original[32..44].as_chunks::<4>().0 {
        b.extend_from_slice(&(u32::from_le_bytes(*chunk) + 52).to_le_bytes());
    }
    b.extend_from_slice(&original[44..]);
    b.extend_from_slice(&[0; 12]);
    // APEv2 header/footer containing arbitrary metadata, followed by ID3v1.
    let mut tag = [0u8; 32];
    tag[..8].copy_from_slice(b"APETAGEX");
    tag[8..12].copy_from_slice(&2000u32.to_le_bytes());
    tag[12..16].copy_from_slice(&64u32.to_le_bytes());
    tag[20..24].copy_from_slice(&0xa0000000u32.to_le_bytes());
    b.extend_from_slice(&tag);
    b.extend_from_slice(&[0; 32]);
    tag[20..24].copy_from_slice(&0x80000000u32.to_le_bytes());
    b.extend_from_slice(&tag);
    b.extend_from_slice(b"TAG");
    b.extend_from_slice(&[0; 125]);
    let path = dir.write("tagged.ape", &b);
    zeroes(&path);
    assert_eq!(fs::read(path).unwrap(), b);
}
#[test]
fn id3_prefix_recognition_skips_tag_payload_without_stealing_other_codecs() {
    let dir = Scratch::new("legacy-prefix");
    let mut prefix = b"ID3\x04\0\0\0\0\0\x08MAC xxx\0".to_vec();
    prefix.extend_from_slice(&source());
    zeroes(&dir.write("prefixed.any", &prefix));
    let other = dir.write(
        "other.mp3",
        &[b"ID3\x04\0\0\0\0\0\0\xff\xfb".as_slice(), &source()].concat(),
    );
    assert!(Decoder::open(&other).unwrap().is_none());
}
#[test]
fn invalid_legacy_headers_and_extents_are_refused_without_panics() {
    let dir = Scratch::new("legacy-bounds");
    for (at, value, width) in [
        (24, 0u32, 4),
        (24, u32::MAX, 4),
        (28, 294913, 4),
        (8, 9, 2),
        (10, 3, 2),
        (12, 0, 4),
        (32, u32::MAX, 4),
        (36, 44, 4),
    ] {
        let mut b = source();
        b[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
        assert!(Decoder::open(&dir.write("bad.ape", &b)).is_err(), "{at}");
    }
    let b = source();
    for end in [6, 31, 43, b.len() - 14] {
        let path = dir.write("cut.ape", &b[..end]);
        if let Ok(Some(mut d)) = Decoder::open(&path) {
            while let Ok(Some(_)) = d.next_chunk() {}
            assert!(d.next_chunk().is_err());
        }
    }
}
#[test]
fn damaged_old_frames_publish_no_prefix_and_retire_the_decoder() {
    let dir = Scratch::new("legacy-crc");
    let mut b = source();
    b[44] ^= 1;
    let mut d = Decoder::open(&dir.write("bad.ape", &b)).unwrap().unwrap();
    assert!(d.next_chunk().is_err());
    assert!(d.next_chunk().is_err());
}
#[cfg(feature = "ape-pcm16")]
#[test]
fn old_pcm16_exact_seeks_cover_frame_edges_backwards_and_eof() {
    let mut d = ApePcm16Decoder::open(File::open(fixture("silence-3970.ape")).unwrap()).unwrap();
    for at in [
        294911, 294912, 294913, 589823, 589824, 881999, 0, 588, TOTAL,
    ] {
        d.seek(at).unwrap();
        let mut frames = 0;
        while let Some(pcm) = d.next_chunk().unwrap() {
            assert!(pcm.iter().all(|v| *v == 0));
            frames += pcm.len() as u64 / 4;
        }
        assert_eq!(frames, TOTAL - at);
    }
}

#[test]
fn cancelled_old_frames_never_reach_the_caller() {
    let mut d = Decoder::open(&fixture("silence-3970.ape"))
        .unwrap()
        .unwrap();
    let mut calls = 0;
    assert!(
        d.next_chunk_admitted(|| {
            calls += 1;
            if calls == 2 {
                Err(Error("cancelled".into()))
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    assert_eq!(calls, 2);
    assert!(d.next_chunk().is_err());
}
#[cfg(feature = "ape-pcm16")]
#[test]
fn legacy_opened_handle_survives_path_replacement() {
    let dir = Scratch::new("legacy-handle");
    let p = dir.write("original.ape", &source());
    let mut d = ApePcm16Decoder::open(File::open(&p).unwrap()).unwrap();
    fs::rename(&p, p.with_extension("retained")).unwrap();
    fs::write(&p, b"replacement").unwrap();
    d.seek(294913).unwrap();
    assert!(d.next_chunk().unwrap().unwrap().iter().all(|v| *v == 0));
}

#[test]
fn malformed_trailing_metadata_cannot_hide_audio_or_allocate_payloads() {
    let dir = Scratch::new("legacy-tag-bounds");
    for (size, flags) in [(u32::MAX, 0u32), (31, 0), (32, 1 << 29), (32, 1 << 31)] {
        let mut b = source();
        let mut tag = [0; 32];
        tag[..8].copy_from_slice(b"APETAGEX");
        tag[8..12].copy_from_slice(&2000u32.to_le_bytes());
        tag[12..16].copy_from_slice(&size.to_le_bytes());
        tag[20..24].copy_from_slice(&flags.to_le_bytes());
        b.extend_from_slice(&tag);
        assert!(Decoder::open(&dir.write("bad.ape", &b)).is_err());
    }
}
