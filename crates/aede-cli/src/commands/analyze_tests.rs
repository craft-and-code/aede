use super::*;

#[test]
fn report_path_uses_the_album_folder_name_without_a_tool_suffix() {
    assert_eq!(
        report_path(Path::new("/music/Artist/Album")),
        PathBuf::from("/music/Artist/Album/Album.json")
    );
}

#[test]
fn report_path_keeps_dots_in_an_album_name() {
    assert_eq!(
        report_path(Path::new("/music/Album.v1")),
        PathBuf::from("/music/Album.v1/Album.v1.json")
    );
}
