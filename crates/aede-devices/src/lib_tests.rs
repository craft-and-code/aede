use super::*;

#[tokio::test]
async fn signal_registration_failures_do_not_report_successful_playback() {
    for transport in [Ok(()), Err("transport failed".to_owned())] {
        let task = tokio::spawn(async { Err("cannot monitor Ctrl-C".to_owned()) });
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
        let error = finish_controller(transport.clone(), task)
            .await
            .unwrap_err();
        assert!(error.contains("cannot monitor Ctrl-C"), "{error}");
        if let Err(transport_error) = transport {
            assert!(error.contains(&transport_error), "{error}");
        }
    }
    let pending = tokio::spawn(std::future::pending::<Result<(), String>>());
    assert!(finish_controller(Ok(()), pending).await.is_ok());
    let pending = tokio::spawn(std::future::pending::<Result<(), String>>());
    assert_eq!(
        finish_controller(Err("transport failed".to_owned()), pending).await,
        Err("transport failed".to_owned())
    );
}

#[test]
fn device_addresses_and_protocol_options_are_explicit_and_local() {
    for address in ["0.0.0.0", "8.8.8.8", "239.255.255.250", "255.255.255.255"] {
        assert!(validate_ip(address.parse().unwrap()).is_err(), "{address}");
    }
    let mut options = CastOptions {
        bind: Ipv4Addr::LOCALHOST,
        protocol: DeviceProtocol::Slimproto,
        device: "127.0.0.1".into(),
        port: Some(3483),
        replace: false,
        volume: None,
        certificate_sha256: None,
    };
    assert_eq!(options.validate().unwrap(), Ipv4Addr::LOCALHOST);
    options.port = Some(0);
    assert!(options.validate().is_err());
    options.port = None;
    options.replace = true;
    assert!(options.validate().is_err());
    options.replace = false;
    options.protocol = DeviceProtocol::Upnp;
    options.device = "http://127.0.0.1:1234/device.xml".into();
    assert_eq!(options.validate().unwrap(), Ipv4Addr::LOCALHOST);
    options.port = Some(3483);
    assert!(options.validate().is_err());
    options.port = None;
    options.replace = true;
    assert!(options.validate().is_err());
    options.protocol = DeviceProtocol::Openhome;
    assert!(options.validate().is_ok());
}
