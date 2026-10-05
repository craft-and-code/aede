use super::*;

#[test]
fn cast_requires_a_trusted_certificate_and_refuses_protocol_specific_options() {
    let pin = "0123456789abcdef".repeat(4);
    let words = [
        "cast",
        "--protocol",
        "googlecast",
        "--device",
        "127.0.0.1:8009",
        "--bind",
        "127.0.0.1",
        "--device-certificate",
        pin.as_str(),
    ];
    let parsed = options(&args(&words)).unwrap();
    assert_eq!(parsed.protocol, DeviceProtocol::Googlecast);
    assert_eq!(parsed.certificate_sha256.as_deref(), Some(pin.as_str()));
    for bad in ["", "g", &"0".repeat(63)] {
        let mut changed = words.to_vec();
        changed[8] = bad;
        assert!(options(&args(&changed)).is_err());
    }
    assert!(options(&args(&words[..8])).is_err());
    for suffix in [["--device-volume", "20"], ["--port", "8009"]] {
        let mut changed = words.to_vec();
        changed.extend_from_slice(&suffix);
        assert!(options(&args(&changed)).is_err());
    }
    let mut changed = words.to_vec();
    changed[2] = "slimproto";
    changed[4] = "127.0.0.1";
    assert!(options(&args(&changed)).is_err());
    let mut replacement = words.to_vec();
    replacement.push("--replace");
    assert!(options(&args(&replacement)).unwrap().replace);
    let mut missing = words[..7].to_vec();
    missing[2] = "slimproto";
    missing[4] = "127.0.0.1";
    missing.push("--device-certificate");
    assert!(options(&args(&missing)).is_err());
    for words in [
        vec!["devices", "--bind", "127.0.0.1", "--protocol"],
        vec![
            "devices",
            "--bind",
            "127.0.0.1",
            "--protocol",
            "googlecast",
            "--device",
        ],
    ] {
        assert!(devices(&args(&words)).is_err());
    }
}

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
