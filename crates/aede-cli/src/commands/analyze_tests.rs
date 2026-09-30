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

#[test]
fn invalid_analysis_options_fail_before_catalog_loading() {
    for options in [
        vec!["analyze", "--json-layout", "wrong"],
        vec!["analyze", "--json-layout"],
        vec!["analyze", "--force=wrong"],
    ] {
        let args = Args::parse(options.into_iter().map(str::to_string));
        assert!(analyze(&args).is_err());
    }
}
