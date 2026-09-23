use super::*;

#[test]
fn reads_fields_across_byte_boundaries() {
    // 1011_0011 0101_1100
    let data = [0b1011_0011, 0b0101_1100];
    let mut r = BitReader::new(&data);
    assert_eq!(r.bits(3), Some(0b101));
    assert_eq!(r.bits(7), Some(0b1001101));
    assert_eq!(r.bits(6), Some(0b011100));
    assert_eq!(r.bits(1), None, "past the end");
}

#[test]
fn unary_counts_zeros_then_eats_the_one() {
    let data = [0b0001_0000];
    let mut r = BitReader::new(&data);
    assert_eq!(r.unary(), Some(3));
    assert_eq!(r.bits(4), Some(0));
}

#[test]
fn signed_values_use_twos_complement() {
    let data = [0b1111_0001];
    let mut r = BitReader::new(&data);
    assert_eq!(r.signed_bits(4), Some(-1));
    assert_eq!(r.signed_bits(4), Some(1));
}

#[test]
fn alignment_moves_to_the_next_whole_byte() {
    let data = [0b1010_1010, 0b1100_0000];
    let mut r = BitReader::new(&data);
    r.bits(3);
    r.align_to_byte();
    assert_eq!(r.bits(2), Some(0b11), "reading resumes on the second byte");
}
