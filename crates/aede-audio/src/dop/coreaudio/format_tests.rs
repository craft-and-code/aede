use super::*;
const FLOAT: u32 = 1;
const BIG_ENDIAN: u32 = 2;
const PLANAR: u32 = 32;

fn description() -> Description {
    Description {
        rate: 176400.0,
        id: u32::from_be_bytes(*b"lpcm"),
        flags: SIGNED | PACKED | NON_MIXABLE,
        bytes_per_packet: 6,
        frames_per_packet: 1,
        bytes_per_frame: 6,
        channels: 2,
        bits: 24,
        reserved: 0,
    }
}

#[test]
fn dop_admits_only_exact_integer_storage_and_non_mixable_virtual_io() {
    let target = Format::new(2_822_400, 2).unwrap();
    let base = description();
    assert_eq!(carrier(base, target, true).unwrap(), Carrier::Packed24Le);
    for changed in [
        Description {
            flags: base.flags | FLOAT,
            ..base
        },
        Description {
            flags: base.flags | BIG_ENDIAN,
            ..base
        },
        Description {
            flags: base.flags | PLANAR,
            ..base
        },
        Description {
            flags: base.flags & !NON_MIXABLE,
            ..base
        },
        Description {
            rate: 192000.0,
            ..base
        },
        Description { bits: 32, ..base },
        Description { bits: 16, ..base },
        Description {
            channels: 1,
            ..base
        },
        Description {
            bytes_per_packet: 8,
            ..base
        },
        Description {
            frames_per_packet: 2,
            ..base
        },
        Description {
            bytes_per_frame: 5,
            bytes_per_packet: 5,
            ..base
        },
    ] {
        assert!(carrier(changed, target, true).is_err(), "{changed:?}");
    }
    let mut low = base;
    low.flags &= !PACKED;
    low.bytes_per_packet = 8;
    low.bytes_per_frame = 8;
    assert_eq!(carrier(low, target, true).unwrap(), Carrier::Low24In32Le);
    low.flags |= HIGH;
    assert_eq!(carrier(low, target, true).unwrap(), Carrier::High24In32Le);
    low.flags |= PACKED;
    assert!(carrier(low, target, true).is_err());
}

#[test]
fn physical_and_virtual_carriers_must_match_without_rate_or_storage_conversion() {
    let target = Format::new(2_822_400, 2).unwrap();
    let ranged = Ranged {
        format: description(),
        minimum: 176400.0,
        maximum: 352800.0,
    };
    assert!(plan(&[ranged], &[ranged], target).is_ok());
    let mut low = ranged;
    low.format.flags &= !PACKED;
    low.format.bytes_per_packet = 8;
    low.format.bytes_per_frame = 8;
    assert!(plan(&[ranged], &[low], target).is_err());
    for (min, max) in [
        (192000.0, 384000.0),
        (0.0, 176400.0),
        (352800.0, 176400.0),
        (f64::NAN, 352800.0),
    ] {
        assert!(
            plan(
                &[Ranged {
                    minimum: min,
                    maximum: max,
                    ..ranged
                }],
                &[ranged],
                target
            )
            .is_err()
        );
    }
    assert!(plan(&[], &[ranged], target).is_err());
}
