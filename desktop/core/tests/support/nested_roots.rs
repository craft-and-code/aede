use aede_desktop_core::Host;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(super) fn assert_retained_identity(before: &Value, after: &Value, path: &Path) {
    let file = before["file"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| Path::new(file["path"].as_str().unwrap()) == path)
        .unwrap();
    assert!(
        after["file"].as_array().unwrap().contains(file),
        "cached file identity and metadata were lost"
    );
    let track = before["track"]
        .as_array()
        .unwrap()
        .iter()
        .find(|track| track["file_id"] == file["id"])
        .unwrap();
    assert!(
        after["track"].as_array().unwrap().contains(track),
        "cached track identity was lost"
    );
}

pub(super) fn assert_selected_roots(host: &mut Host, expected: &[(&Path, bool)]) {
    let status = serde_json::to_value(host.status().unwrap()).unwrap();
    assert_eq!(status["running"], true);
    let actual: BTreeMap<PathBuf, bool> = status["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|root| {
            (
                PathBuf::from(root["path"].as_str().unwrap()),
                root["available"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|(path, available)| (path.to_path_buf(), *available))
            .collect()
    );
}
