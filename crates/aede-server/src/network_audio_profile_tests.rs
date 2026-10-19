use super::*;
#[test]
fn profiles_validate_before_transport_and_preserve_legacy_upnp() {
    for (protocol, device, pin, accepted) in [
        ("upnp", "http://127.0.0.1/device.xml", None, true),
        ("openhome", "http://127.0.0.1/device.xml", None, true),
        ("slimproto", "127.0.0.1", None, true),
        ("googlecast", "127.0.0.1:8009", None, false),
        ("googlecast", "127.0.0.1:8009", Some("a".repeat(64)), true),
        ("slimproto", "8.8.8.8", None, false),
    ] {
        let input: Playback = serde_json::from_value(serde_json::json!({"bind":"127.0.0.1", "device":device,"tracks":["track:a"],"protocol":protocol,"certificate_sha256":pin})).unwrap();
        assert_eq!(input.options().is_ok(), accepted, "{protocol}");
    }
    let legacy: Playback = serde_json::from_value(serde_json::json!({"bind":"127.0.0.1", "device":"http://127.0.0.1/device.xml","tracks":["track:a"]})).unwrap();
    assert_eq!(
        legacy.options().unwrap().protocol,
        aede_devices::DeviceProtocol::Upnp
    );
}
#[test]
fn replacement_and_protocol_specific_options_are_never_silently_ignored() {
    for (protocol, accepted) in [
        ("upnp", false),
        ("openhome", true),
        ("slimproto", false),
        ("googlecast", true),
    ] {
        let device = if matches!(protocol, "googlecast" | "slimproto") {
            "127.0.0.1"
        } else {
            "http://127.0.0.1/device.xml"
        };
        let mut raw = serde_json::json!({"bind":"127.0.0.1","device":device,"tracks":["track:a"],"protocol":protocol,"replace":true});
        if protocol == "googlecast" {
            raw["certificate_sha256"] = serde_json::json!("a".repeat(64));
        }
        let input: Playback = serde_json::from_value(raw).unwrap();
        assert_eq!(input.options().is_ok(), accepted);
    }
}
