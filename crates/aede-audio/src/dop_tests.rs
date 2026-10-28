use super::*;
use crate::dsd::test_fixtures::{dff, dsf, source};

#[test]
fn standard_t0_slot_and_markers_have_exact_integer_storage() {
    let format = Format::new(2_822_400, 2).unwrap();
    assert_eq!(
        (
            format.source_rate(),
            format.carrier_rate(),
            format.channels()
        ),
        (2_822_400, 176_400, 2)
    );
    let mut encoder = Encoder::new(format, Carrier::Packed24Le);
    let mut out = [0; 6];
    encoder
        .write_frame(&[Payload(0x8001), Payload(0x69b4)], &mut out)
        .unwrap();
    assert_eq!(out, [1, 0x80, 5, 0xb4, 0x69, 5]);
    encoder.write_silence(&mut out).unwrap();
    assert_eq!(out, [0x69, 0x69, 0xfa, 0x69, 0x69, 0xfa]);
    encoder
        .write_frame(&[Payload(0x8001), Payload(0x69b4)], &mut out)
        .unwrap();
    assert_eq!(out, [1, 0x80, 5, 0xb4, 0x69, 5]);
}

#[test]
fn all_integer_carriers_preserve_markers_and_every_payload_bit() {
    for carrier in [
        Carrier::Packed24Le,
        Carrier::Low24In32Le,
        Carrier::High24In32Le,
    ] {
        let mut encoder = Encoder::new(Format::new(5_644_800, 1).unwrap(), carrier);
        let mut output = vec![0; carrier.bytes()];
        for (index, word) in [0u16, 1, 0x7fff, 0x8000, 0x8001, 0xffff]
            .into_iter()
            .enumerate()
        {
            encoder.write_frame(&[Payload(word)], &mut output).unwrap();
            let mut bytes = [0; 4];
            bytes[..output.len()].copy_from_slice(&output);
            let raw = u32::from_le_bytes(bytes);
            let bits = if carrier == Carrier::High24In32Le {
                raw >> 8
            } else {
                raw & 0xffffff
            };
            assert_eq!(
                bits,
                (if index % 2 == 0 { 0x05 } else { 0xfa }) << 16 | u32::from(word)
            );
            if carrier == Carrier::High24In32Le {
                assert_eq!(output[0], 0);
            }
            if carrier == Carrier::Low24In32Le {
                assert_eq!(output[3], if index % 2 == 0 { 0 } else { 0xff });
            }
        }
    }
}

#[test]
fn rejected_frame_sizes_leave_output_and_phase_intact() {
    let mut encoder = Encoder::new(Format::new(2_822_400, 2).unwrap(), Carrier::Packed24Le);
    let mut output = [42; 7];
    assert!(encoder.write_frame(&[Payload(0)], &mut output).is_err());
    assert!(encoder.write_frame(&[Payload(0); 2], &mut output).is_err());
    assert_eq!(output, [42; 7]);
    encoder.write_silence(&mut output[..6]).unwrap();
    assert_eq!(output, [0x69, 0x69, 5, 0x69, 0x69, 5, 42]);
}

fn payload(path: &Path) -> (Info, Vec<Payload>) {
    let mut reader = Reader::open(path).unwrap();
    let info = reader.info();
    let mut samples = 0;
    let mut result = Vec::new();
    while let Some(block) = reader.next_chunk(|| Ok(())).unwrap() {
        assert!(block.payload.len() <= 2048 * info.channels);
        assert_eq!(block.first_frame, samples / 16);
        samples += block.valid_samples;
        result.extend(block.payload);
    }
    assert_eq!(samples, info.samples);
    assert_eq!(
        result.len(),
        info.samples.div_ceil(16) as usize * info.channels
    );
    (info, result)
}

#[test]
fn original_source_bits_survive_dop_across_block_and_channel_boundaries() {
    for channels in 1..=2 {
        let data: Vec<Vec<u8>> = (0..channels)
            .map(|c| (0..9001).map(|i| (i * 73 + c * 41) as u8).collect())
            .collect();
        for rate in [2_822_400, 5_644_800] {
            for encoded in [
                dsf(&data, rate, 72008, true),
                dsf(&data, rate, 72008, false),
                dff(&data, rate),
            ] {
                let file = source(&encoded);
                let (info, words) = payload(&file.0);
                for bit in 0..info.samples as usize {
                    for channel in 0..channels {
                        let actual =
                            (words[bit / 16 * channels + channel].0 >> (15 - bit % 16)) & 1;
                        let expected = (data[channel][bit / 8] >> (bit % 8)) & 1;
                        assert_eq!(actual as u8, expected, "bit {bit}, channel {channel}");
                    }
                }
                assert_eq!(std::fs::read(&file.0).unwrap(), encoded);
            }
        }
    }
}

#[test]
fn every_partial_final_word_keeps_only_original_bits_and_declared_silence() {
    for count in 1..=31u64 {
        let data = vec![0xff; count.div_ceil(8) as usize];
        let file = source(&dsf(&[data], 2_822_400, count, true));
        let (_, words) = payload(&file.0);
        for bit in 0..words.len() * 16 {
            let actual = (words[bit / 16].0 >> (15 - bit % 16)) & 1;
            let expected = if bit < count as usize {
                1
            } else {
                (0x69 >> (7 - bit % 8)) & 1
            };
            assert_eq!(actual, expected, "length {count}, bit {bit}");
        }
    }
}

#[test]
fn native_bit_seeks_have_no_pcm_preroll_or_marker_reset() {
    let data: Vec<u8> = (0..9001).map(|i| (i * 73) as u8).collect();
    let file = source(&dsf(&[data], 2_822_400, 72003, true));
    let (_, expected) = payload(&file.0);
    let mut reader = Reader::open(&file.0).unwrap();
    for at in [0u64, 1, 2047, 2048, 4500, reader.frames()] {
        reader.seek(at).unwrap();
        let mut actual = Vec::new();
        while let Some(block) = reader.next_chunk(|| Ok(())).unwrap() {
            actual.extend(block.payload);
        }
        assert_eq!(actual, expected[at as usize..]);
    }
    reader.seek(1).unwrap();
    assert!(reader.seek(reader.frames() + 1).is_err());
    assert_eq!(
        reader.next_chunk(|| Ok(())).unwrap().unwrap().first_frame,
        1
    );
}

#[test]
fn cancelled_dop_reads_retire_without_publishing_or_reviving_the_source() {
    let file = source(&dsf(&[vec![0x69; 9001]], 2_822_400, 72008, true));
    let mut reader = Reader::open(&file.0).unwrap();
    let mut calls = 0;
    assert!(
        reader
            .next_chunk(|| {
                calls += 1;
                if calls == 4 {
                    Err(DsdError("retired".into()))
                } else {
                    Ok(())
                }
            })
            .is_err()
    );
    assert!(reader.next_chunk(|| Ok(())).is_err());
    assert!(reader.seek(0).is_err());
    for (rate, channels) in [
        (0, 2),
        (4_096_000, 2),
        (11_289_600, 2),
        (2_822_400, 0),
        (2_822_400, 6),
    ] {
        assert!(Format::new(rate, channels).is_err());
    }
}
