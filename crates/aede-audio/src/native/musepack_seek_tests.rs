use super::*;

#[test]
fn positive_and_negative_seek_residuals_respect_the_source_extent() {
    for (bytes, last) in [
        (&[3, 0, 0xa1, 0x48, 1, 0x80][..], 29),
        (&[3, 0, 0xa1, 0x48, 2, 0][..], 32),
    ] {
        assert!(table(bytes, 0, last + 1).is_ok());
        assert!(table(bytes, 0, last).is_err());
    }
}

#[test]
fn seek_fields_and_unary_runs_cannot_consume_the_following_packet() {
    for bytes in [&[1, 0x08][..], &[2, 0, 0][..], &[40, 0, 0, 0, 0, 0][..]] {
        assert!(table(bytes, 0, 44113).is_err());
    }
}
