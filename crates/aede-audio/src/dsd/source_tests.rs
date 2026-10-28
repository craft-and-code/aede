use super::super::test_fixtures::{dff, dsf, source};
use super::*;

fn drain(reader: &mut RawSource) -> Vec<u8> {
    let mut output = Vec::new();
    let mut next = reader.position();
    while let Some(block) = reader.next_chunk(|| Ok(())).unwrap() {
        assert_eq!(block.first_sample, next);
        assert!(block.bytes.len() <= 4096 * block.channels);
        assert!(block.valid_samples > 0);
        next += block.valid_samples;
        output.extend(block.bytes);
    }
    assert_eq!(next, reader.info().samples);
    output
}

#[test]
fn raw_bits_preserve_channel_and_bit_order_across_container_blocks() {
    for channels in 1..=2 {
        let data: Vec<Vec<u8>> = (0..channels)
            .map(|c| (0..9001).map(|i| (i * 73 + c * 41) as u8).collect())
            .collect();
        let expected: Vec<u8> = (0..9001)
            .flat_map(|i| data.iter().map(move |c| c[i]))
            .collect();
        for bytes in [
            dsf(&data, 2_822_400, 9001 * 8, true),
            dsf(&data, 5_644_800, 9001 * 8, false),
            dff(&data, 2_822_400),
        ] {
            let file = source(&bytes);
            let mut reader = RawSource::open(&file.0).unwrap();
            assert_eq!(drain(&mut reader), expected);
            for byte in [0u64, 17, 4095, 4096, 4097, 8999, 9001] {
                reader.seek(byte * 8).unwrap();
                assert_eq!(drain(&mut reader), expected[byte as usize * channels..]);
            }
            assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
        }
    }
}

#[test]
fn valid_bit_counts_exclude_dsf_padding_and_rejected_seeks_preserve_position() {
    let bytes = dsf(&[vec![0x81, 0x24, 0xff]], 2_822_400, 17, true);
    let file = source(&bytes);
    let mut reader = RawSource::open(&file.0).unwrap();
    assert!(reader.seek(4).is_err());
    assert_eq!(reader.position(), 0);
    let block = reader.next_chunk(|| Ok(())).unwrap().unwrap();
    assert_eq!(block.bytes, [0x81, 0x24, 0x01]);
    assert_eq!((block.first_sample, block.valid_samples), (0, 17));
    reader.seek(8).unwrap();
    assert!(reader.seek(24).is_err());
    assert_eq!(reader.position(), 8);
    let block = reader.next_chunk(|| Ok(())).unwrap().unwrap();
    assert_eq!((block.first_sample, block.valid_samples), (8, 9));
    assert_eq!(block.bytes, [0x24, 0x01]);
    reader.seek(17).unwrap();
    assert!(reader.next_chunk(|| Ok(())).unwrap().is_none());
    assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
}

#[test]
fn raw_cancellation_and_late_read_failure_retire_the_source_before_publication() {
    let bytes = dsf(&[vec![0x81; 9001]], 2_822_400, 9001 * 8, true);
    let file = source(&bytes);
    let mut reader = RawSource::open(&file.0).unwrap();
    let mut calls = 0;
    assert!(
        reader
            .next_chunk(|| {
                calls += 1;
                if calls == 3 {
                    Err(DsdError("retired".into()))
                } else {
                    Ok(())
                }
            })
            .is_err()
    );
    assert!(reader.next_chunk(|| Ok(())).is_err());
    assert!(reader.seek(0).is_err());

    let mut reader = RawSource::open(&file.0).unwrap();
    assert_eq!(
        reader.next_chunk(|| Ok(())).unwrap().unwrap().valid_samples,
        32768
    );
    let writer = std::fs::OpenOptions::new()
        .write(true)
        .open(&file.0)
        .unwrap();
    writer.set_len(92 + 4096 + 20).unwrap();
    assert!(reader.next_chunk(|| Ok(())).is_err());
    assert!(reader.next_chunk(|| Ok(())).is_err());
    assert!(reader.seek(0).is_err());
}

#[test]
fn raw_clock_inspection_does_not_enable_pcm_filters_or_unknown_channels() {
    let file = source(&dsf(&[vec![0x69; 8]], 11_289_600, 64, true));
    let mut raw = RawSource::open(&file.0).unwrap();
    assert_eq!(raw.info().sample_rate, 11_289_600);
    assert_eq!(drain(&mut raw), [0x69; 8]);
    assert!(super::super::Decoder::open(&file.0, super::super::Profile::Listening).is_err());
    let file = source(&dsf(&vec![vec![0x69; 8]; 3], 2_822_400, 64, true));
    assert!(RawSource::open(&file.0).is_err());
    assert!(RawSource::open(&std::env::temp_dir()).is_err());
}
