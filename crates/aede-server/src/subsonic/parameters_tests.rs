use super::*;

#[test]
fn strict_form_decoding_preserves_utf8_and_rejects_lossy_or_duplicate_inputs() {
    let mut params = Parameters::default();
    params
        .append("query=A%C3%A8de+%26+Jazz&songCount=0")
        .unwrap();
    assert_eq!(params.get("query"), Some("Aède & Jazz"));
    assert_eq!(params.number("songCount", 20, 500).unwrap(), 0);
    for invalid in [
        "query=%",
        "query=%GG",
        "query=%FF",
        "query=x&query=y",
        "=x",
        "bad-name=x",
    ] {
        assert!(Parameters::default().append(invalid).is_err(), "{invalid}");
    }
    assert!(params.append("query=duplicate").is_err());
    assert!(
        Parameters::default()
            .append(&format!("query={}", "x".repeat(2049)))
            .is_err()
    );
}

#[test]
fn common_parameters_refuse_jsonp_invalid_versions_and_unknown_options() {
    for version in [
        "",
        "1",
        "1.16",
        "x.16.1",
        "1.99999999999999999.1",
        "1.16.4294967296",
        "1.16.-1",
        "1.16.999x",
        "1.16.999.0",
        "0.1.0",
        "2.0.0",
        "1.17.0",
    ] {
        let params = Parameters::from_pairs(&[("v", version), ("c", "tests")]);
        assert!(params.common(false).is_err(), "{version}");
    }
    let params = Parameters::from_pairs(&[("v", "1.16.1"), ("c", "tests"), ("query", "")]);
    params.common(false).unwrap();
    params.allowed(&["query"]).unwrap();
    assert!(params.allowed(&[]).is_err());
    Parameters::default().common(true).unwrap();
    assert!(
        Parameters::from_pairs(&[("f", "jsonp"), ("callback", "alert")])
            .format()
            .is_err()
    );
    assert!(
        Parameters::from_pairs(&[("size", "184467440737095516160")])
            .number("size", 10, 500)
            .is_err()
    );
}

#[test]
fn api_version_patch_changes_do_not_require_a_client_or_server_upgrade() {
    for version in ["1.16.2", "1.16.999", "1.16.4294967295", "1.15.4294967295"] {
        let params = Parameters::from_pairs(&[("v", version), ("c", "tests")]);
        assert!(
            params.common(false).is_ok(),
            "compatible patch version {version}"
        );
    }
    for (version, code) in [("0.999.999", 20), ("2.0.0", 30), ("1.17.0", 30)] {
        let params = Parameters::from_pairs(&[("v", version), ("c", "tests")]);
        assert_eq!(params.common(false).unwrap_err().code, code, "{version}");
    }
}

#[test]
fn repeated_fields_preserve_playlist_order_but_never_repeat_credentials_or_scalar_options() {
    let mut playlist = Parameters::for_method("createPlaylist");
    playlist
        .append("name=Evening&songId=first&songId=second&songId=first&apiKey=one")
        .unwrap();
    playlist.append("songId=third").unwrap();
    assert_eq!(
        playlist.values("songId"),
        ["first", "second", "first", "third"]
    );
    assert!(playlist.append("apiKey=two").is_err());
    assert!(playlist.append("name=Other").is_err());
    assert!(
        Parameters::for_method("getSong")
            .append("id=a&id=b")
            .is_err()
    );
    let mut stars = Parameters::for_method("star");
    stars.append("id=a&id=b&artistId=c&artistId=d").unwrap();
    assert_eq!(stars.values("id"), ["a", "b"]);
    assert_eq!(stars.values("artistId"), ["c", "d"]);
}

#[test]
fn repeated_fields_keep_request_work_bounded() {
    let mut playlist = Parameters::for_method("createPlaylist");
    playlist.append(&vec!["songId=a"; 256].join("&")).unwrap();
    assert!(playlist.append("songId=b").is_err());
    let mut scalars = Parameters::default();
    scalars
        .append(
            &(0..32)
                .map(|id| format!("field{id}=x"))
                .collect::<Vec<_>>()
                .join("&"),
        )
        .unwrap();
    assert!(scalars.append("more=x").is_err());
}
