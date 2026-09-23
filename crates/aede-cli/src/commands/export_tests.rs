use super::*;

#[test]
fn a_field_is_quoted_only_when_it_has_to_be() {
    assert_eq!(escape("So What", ','), "So What");
    assert_eq!(escape("Freedom, Pt. 2", ','), "\"Freedom, Pt. 2\"");
    // The separator decides: the same title needs nothing under `;`.
    assert_eq!(escape("Freedom, Pt. 2", ';'), "Freedom, Pt. 2");
    // Quotes are doubled, not escaped with a backslash.
    assert_eq!(escape("Say \"Hello\"", ','), "\"Say \"\"Hello\"\"\"");
    assert_eq!(escape("Two\nlines", ','), "\"Two\nlines\"");
}
