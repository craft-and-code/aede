//! Related local editions and credit filenames retain their literal spelling.

#![cfg(unix)]

#[path = "cli_literal_related_paths_support/mod.rs"]
mod literal_paths;
#[path = "annotation_command_support/mod.rs"]
#[allow(dead_code)]
mod personal;

#[test]
fn album_release_group_edition_folder_is_literal_without_color() {
    literal_paths::assert_release_group_folder(false);
}

#[test]
fn album_release_group_edition_folder_is_literal_on_a_real_terminal() {
    literal_paths::assert_release_group_folder(true);
}

#[test]
fn album_duplicate_folder_is_literal_without_color() {
    literal_paths::assert_related_folder(false, false);
}

#[test]
fn album_duplicate_folder_is_literal_on_a_real_terminal() {
    literal_paths::assert_related_folder(true, false);
}

#[test]
fn album_encoded_differently_folder_is_literal_without_color() {
    literal_paths::assert_related_folder(false, true);
}

#[test]
fn album_encoded_differently_folder_is_literal_on_a_real_terminal() {
    literal_paths::assert_related_folder(true, true);
}

#[test]
fn credits_file_basename_is_literal_without_color() {
    literal_paths::assert_credit_basename(false);
}

#[test]
fn credits_file_basename_is_literal_on_a_real_terminal() {
    literal_paths::assert_credit_basename(true);
}
