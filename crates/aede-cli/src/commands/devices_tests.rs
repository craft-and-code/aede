use super::*;

fn args(words: &[&str]) -> Args {
    Args::parse(words.iter().map(|word| word.to_string()))
}

#[test]
fn casting_options_preserve_the_selection_and_refuse_ignored_settings() {
    let arguments = args(&[
        "cast",
        "Example Album",
        "--protocol",
        "slimproto",
        "--device",
        "127.0.0.1",
        "--bind",
        "127.0.0.1",
    ]);
    assert_eq!(arguments.positionals, ["Example Album"]);
    assert_eq!(
        options(&arguments).unwrap().protocol,
        DeviceProtocol::Slimproto
    );
    let volume = args(&[
        "cast",
        "--protocol",
        "slimproto",
        "--device",
        "127.0.0.1",
        "--bind",
        "127.0.0.1",
        "--device-volume",
        "20",
    ]);
    assert_eq!(options(&volume).unwrap().volume, Some(20));
    for words in [
        vec![
            "cast",
            "--protocol",
            "slimproto",
            "--device",
            "127.0.0.1",
            "--bind",
            "127.0.0.1",
            "--device-volume",
            "101",
        ],
        vec![
            "cast",
            "--protocol",
            "openhome",
            "--device",
            "http://127.0.0.1:1234/device.xml",
            "--bind",
            "127.0.0.1",
            "--device-volume",
            "20",
        ],
        vec![
            "cast",
            "--protocol",
            "other",
            "--device",
            "127.0.0.1",
            "--bind",
            "127.0.0.1",
        ],
        vec!["cast", "--protocol", "slimproto", "--device", "127.0.0.1"],
        vec![
            "cast",
            "--protocol",
            "slimproto",
            "--device",
            "localhost",
            "--bind",
            "127.0.0.1",
        ],
        vec![
            "cast",
            "--protocol",
            "slimproto",
            "--device",
            "127.0.0.1",
            "--bind",
            "127.0.0.1",
            "--replace",
        ],
        vec![
            "cast",
            "--protocol",
            "upnp",
            "--device",
            "http://127.0.0.1:1234/device.xml",
            "--bind",
            "127.0.0.1",
            "--port",
            "3483",
        ],
    ] {
        assert!(options(&args(&words)).is_err(), "{words:?}");
    }
}
