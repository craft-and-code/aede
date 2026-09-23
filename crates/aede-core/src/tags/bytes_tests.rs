use super::*;

#[test]
fn cursor_bounds_reads() {
    let data = [1u8, 2, 3];
    let mut c = Cursor::new(&data);
    assert_eq!(c.u16_be(), Some(0x0102));
    assert_eq!(c.remaining(), 1);
    assert_eq!(c.u32_be(), None, "an out-of-bounds read returns None");
    assert_eq!(c.remaining(), 1, "and consumes nothing");
}

#[test]
fn syncsafe_id3() {
    // 0x00 0x00 0x02 0x01 => 257
    assert_eq!(syncsafe(&[0x00, 0x00, 0x02, 0x01]), 257);
    assert_eq!(syncsafe(&[0x00, 0x00, 0x00, 0x7F]), 127);
    assert_eq!(syncsafe(&[0x00, 0x00, 0x01, 0x00]), 128);
}

#[test]
fn extended80() {
    // 44100 Hz encoded as an 80-bit extended float.
    let bytes = [0x40, 0x0E, 0xAC, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let value = extended80_to_f64(&bytes).unwrap();
    assert!((value - 44100.0).abs() < 0.001, "got {value}");
}

#[test]
fn magic_does_not_consume_when_absent() {
    let data = b"fLaC";
    let mut c = Cursor::new(data);
    assert!(!c.expect_magic(b"OggS"));
    assert_eq!(c.position(), 0);
    assert!(c.expect_magic(b"fLaC"));
    assert_eq!(c.position(), 4);
}
