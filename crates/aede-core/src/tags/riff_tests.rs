use super::*;

#[test]
fn wav_info_list() {
    let mut data = Vec::new();
    for (id, value) in [(b"INAM", "So What"), (b"IART", "Miles Davis")] {
        data.extend_from_slice(id);
        data.extend_from_slice(&(value.len() as u32).to_le_bytes());
        data.extend_from_slice(value.as_bytes());
        if value.len() % 2 == 1 {
            data.push(0);
        }
    }
    let mut tags = RawTags::default();
    read_info_list(&data, &mut tags);
    assert_eq!(tags.first("title"), Some("So What"));
    assert_eq!(tags.first("artist"), Some("Miles Davis"));
}

#[test]
fn truncated_info_list_does_not_panic() {
    let mut data = b"INAM".to_vec();
    data.extend_from_slice(&999u32.to_le_bytes()); // dishonest size
    data.extend_from_slice(b"short");
    let mut tags = RawTags::default();
    read_info_list(&data, &mut tags);
    assert!(tags.is_empty());
}
