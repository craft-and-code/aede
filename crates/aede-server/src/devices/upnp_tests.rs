use std::path::PathBuf;

use aede_core::tags::AudioProperties;
use tokio::net::TcpListener;

use super::*;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn track(title: &str) -> DeviceTrack {
    DeviceTrack {
        path: PathBuf::from("unused.flac"),
        title: title.into(),
        mime: "audio/flac",
        properties: AudioProperties::default(),
    }
}

fn device_description(port: u16, openhome: bool) -> String {
    let service = if openhome {
        OPENHOME_PLAYLIST
    } else {
        "urn:schemas-upnp-org:service:AVTransport:1"
    };
    format!(
        "<?xml version=\"1.0\"?><root xmlns=\"{DESCRIPTION_NAMESPACE}\"><URLBase>http://127.0.0.1:{port}/</URLBase><device><friendlyName>Test &amp; Renderer</friendlyName><serviceList><service><serviceType>{service}</serviceType><controlURL>/control</controlURL></service><service><serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType><controlURL>/connection</controlURL></service></serviceList></device></root>"
    )
}

fn reply(service: &str, action: &str, fields: &str) -> String {
    format!(
        "<s:Envelope xmlns:s=\"{SOAP_NAMESPACE}\"><s:Body><u:{action}Response xmlns:u=\"{service}\">{fields}</u:{action}Response></s:Body></s:Envelope>"
    )
}

async fn scripted_device(listener: TcpListener, replies: Vec<(String, String)>) -> Vec<String> {
    scripted_device_trigger(listener, replies, None).await
}

async fn scripted_device_trigger(
    listener: TcpListener,
    replies: Vec<(String, String)>,
    mut trigger: Option<(String, watch::Sender<bool>)>,
) -> Vec<String> {
    let mut requests = Vec::new();
    for (expected, body) in replies {
        let (socket, _) = timeout(Duration::from_secs(4), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut socket = BufReader::new(socket);
        let request = String::from_utf8(line(&mut socket, 8192).await.unwrap()).unwrap();
        let mut length = 0;
        let mut action = None;
        loop {
            let header = String::from_utf8(line(&mut socket, 8192).await.unwrap()).unwrap();
            if header.is_empty() {
                break;
            }
            let (name, value) = header.split_once(':').unwrap();
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().unwrap();
            }
            if name.eq_ignore_ascii_case("soapaction") {
                action = Some(
                    value
                        .trim()
                        .trim_matches('"')
                        .split('#')
                        .nth(1)
                        .unwrap()
                        .to_owned(),
                );
            }
        }
        let mut payload = vec![0; length];
        socket.read_exact(&mut payload).await.unwrap();
        if expected == "GET" {
            assert!(request.starts_with("GET /description.xml "));
        } else {
            assert_eq!(action.as_deref(), Some(expected.as_str()));
        }
        requests.push(String::from_utf8(payload).unwrap());
        socket
            .get_mut()
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        if trigger
            .as_ref()
            .is_some_and(|(action, _)| action == &expected)
        {
            trigger.take().unwrap().1.send(true).unwrap();
        }
    }
    requests
}

#[test]
fn cancellation_stops_an_owned_renderer_without_advancing_the_queue() {
    runtime().block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let avt = "urn:schemas-upnp-org:service:AVTransport:1";
        let replies = vec![
            ("GET".into(), device_description(port, false)),
            (
                "GetProtocolInfo".into(),
                reply(
                    "urn:schemas-upnp-org:service:ConnectionManager:1",
                    "GetProtocolInfo",
                    "<Sink>http-get:*:audio/flac:*</Sink>",
                ),
            ),
            (
                "GetTransportInfo".into(),
                reply(
                    avt,
                    "GetTransportInfo",
                    "<CurrentTransportState>STOPPED</CurrentTransportState>",
                ),
            ),
            (
                "SetAVTransportURI".into(),
                reply(avt, "SetAVTransportURI", ""),
            ),
            ("Play".into(), reply(avt, "Play", "")),
            ("Stop".into(), reply(avt, "Stop", "")),
        ];
        let (shutdown, signal) = watch::channel(false);
        let mock = tokio::spawn(scripted_device_trigger(
            listener,
            replies,
            Some(("Play".into(), shutdown)),
        ));
        run(
            DeviceProtocol::Upnp,
            &format!("http://127.0.0.1:{port}/description.xml"),
            &[track("First"), track("Second")],
            &[
                "http://127.0.0.1:9876/token/0".into(),
                "http://127.0.0.1:9876/token/1".into(),
            ],
            false,
            signal,
        )
        .await
        .unwrap();
        assert_eq!(mock.await.unwrap().len(), 6);
    });
}

#[test]
fn lan_urls_refuse_dns_public_addresses_redirects_and_encoded_traversal() {
    for url in [
        "https://127.0.0.1/",
        "http://localhost/",
        "http://8.8.8.8/",
        "http://0.0.0.0/",
        "http://239.255.255.250/",
        "http://127.0.0.1:0/",
        "http://user@127.0.0.1/",
        "http://127.0.0.1/a/%2e%2e/b",
        "http://127.0.0.1/a%2f..%2fb",
        "http://127.0.0.1/a%00",
        "http://127.0.0.1/a\r\nHeader:x",
    ] {
        assert!(device_ip(url).is_err(), "accepted {url}");
    }
    let base = DeviceUrl::parse("http://192.168.1.5:4567/device/root.xml").unwrap();
    assert_eq!(base.resolve("control").unwrap().path, "/device/control");
    assert_eq!(base.resolve("/control").unwrap().path, "/control");
    for target in [
        "http://192.168.1.6:4567/control",
        "http://192.168.1.5:4568/control",
        "//evil/control",
        "../control",
        "https://192.168.1.5/control",
    ] {
        assert!(base.resolve(target).is_err());
    }
}

#[test]
fn discovery_locations_must_match_the_response_peer() {
    let response = b"HTTP/1.1 200 OK\r\nLOCATION: http://192.168.1.5:80/description.xml\r\n\r\n";
    assert!(ssdp_location(response, "192.168.1.5:1900".parse().unwrap()).is_ok());
    assert!(ssdp_location(response, "192.168.1.6:1900".parse().unwrap()).is_err());
    assert!(ssdp_location(b"HTTP/1.1 200 OK\r\nLOCATION: http://127.0.0.1/\r\nLOCATION: http://127.0.0.1/\r\n\r\n", "127.0.0.1:1900".parse().unwrap()).is_err());
}

#[test]
fn descriptions_refuse_cross_origin_controls_and_mime_negotiation_preserves_codec() {
    let location = DeviceUrl::parse("http://127.0.0.1:4567/description.xml").unwrap();
    let valid = device_description(4567, false);
    assert_eq!(
        description(&valid, &location).unwrap().name,
        "Test & Renderer"
    );
    assert!(
        description(
            &valid.replace("/control", "http://127.0.0.2:4567/control"),
            &location
        )
        .is_err()
    );
    assert!(
        description(
            &valid.replace("http://127.0.0.1:4567/", "http://127.0.0.1:4568/"),
            &location
        )
        .is_err()
    );
    assert!(advertised_mime("http-get:*:audio/mpeg:*", "audio/flac").is_none());
    assert!(
        advertised_mime(
            "http-get:*:audio/mpeg:*,http-get:*:audio/flac:*",
            "audio/flac"
        )
        .is_some()
    );
    assert_eq!(
        advertised_mime("http-get:*:audio/x-flac:*", "audio/flac"),
        Some("audio/x-flac")
    );
}

#[test]
fn soap_refuses_foreign_envelopes_action_namespaces_and_response_fields() {
    runtime().block_on(async {
        for body in [
            reply("urn:foreign", "Play", ""),
            reply(OPENHOME_PLAYLIST, "OtherAction", ""),
            reply(
                OPENHOME_PLAYLIST,
                "Play",
                "<f:Value xmlns:f=\"urn:foreign\">1</f:Value>",
            ),
            reply(OPENHOME_PLAYLIST, "Play", "").replace(SOAP_NAMESPACE, "urn:foreign"),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let mock = tokio::spawn(scripted_device(listener, vec![("Play".into(), body)]));
            let service = Service {
                kind: OPENHOME_PLAYLIST.into(),
                control: DeviceUrl::parse(&format!("http://127.0.0.1:{port}/control")).unwrap(),
                device_scope: 0,
            };
            assert!(soap(&service, "Play", &[]).await.is_err());
            assert_eq!(mock.await.unwrap().len(), 1);
        }
    });
}

#[test]
fn upnp_preserves_original_urls_and_waits_for_playing_before_advancing() {
    runtime().block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let avt = "urn:schemas-upnp-org:service:AVTransport:1";
        let cm = "urn:schemas-upnp-org:service:ConnectionManager:1";
        let state = |value: &str| reply(avt, "GetTransportInfo", &format!("<CurrentTransportState>{value}</CurrentTransportState><CurrentTransportStatus>OK</CurrentTransportStatus>"));
        let urls = vec!["http://127.0.0.1:9876/capability/0?a=1&b=2".to_string(), "http://127.0.0.1:9876/capability/1".to_string()];
        let position = |index: usize| reply(avt, "GetPositionInfo", &format!("<TrackURI>{}</TrackURI>", xml::escape(&urls[index]).unwrap()));
        let replies = vec![
            ("GET".into(), device_description(port, false)),
            ("GetProtocolInfo".into(), reply(cm, "GetProtocolInfo", "<Sink>http-get:*:audio/flac:*</Sink>")),
            ("GetTransportInfo".into(), state("STOPPED")),
            ("SetAVTransportURI".into(), reply(avt, "SetAVTransportURI", "")),
            ("Play".into(), reply(avt, "Play", "")),
            ("GetTransportInfo".into(), state("STOPPED")),
            ("GetPositionInfo".into(), position(0)),
            ("GetTransportInfo".into(), state("TRANSITIONING")),
            ("GetPositionInfo".into(), position(0)),
            ("GetTransportInfo".into(), state("PLAYING")),
            ("GetPositionInfo".into(), position(0)),
            ("GetTransportInfo".into(), state("STOPPED")),
            ("GetPositionInfo".into(), position(0)),
            ("SetAVTransportURI".into(), reply(avt, "SetAVTransportURI", "")),
            ("Play".into(), reply(avt, "Play", "")),
            ("GetTransportInfo".into(), state("PLAYING")),
            ("GetPositionInfo".into(), position(1)),
            ("GetTransportInfo".into(), state("STOPPED")),
            ("GetPositionInfo".into(), position(1)),
        ];
        let mock = tokio::spawn(scripted_device(listener, replies));
        let (_keep, signal) = watch::channel(false);
        run(DeviceProtocol::Upnp, &format!("http://127.0.0.1:{port}/description.xml"), &[track("A & B <live>"), track("Second")], &urls, false, signal).await.unwrap();
        let requests = mock.await.unwrap();
        let set = xml::parse(&requests[3]).unwrap();
        let action = &set.child("Body").unwrap().children[0];
        assert_eq!(action.value("CurrentURI").unwrap(), urls[0]);
        let didl = xml::parse(action.value("CurrentURIMetaData").unwrap()).unwrap();
        let item = didl.child("item").unwrap();
        assert_eq!(item.value("title").unwrap(), "A & B <live>");
        assert_eq!(item.value("res").unwrap(), urls[0]);
    });
}

#[test]
fn stopped_foreign_transport_never_advances_or_stops_another_controllers_audio() {
    runtime().block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let avt = "urn:schemas-upnp-org:service:AVTransport:1";
        let state = |value: &str| reply(avt, "GetTransportInfo", &format!("<CurrentTransportState>{value}</CurrentTransportState><CurrentTransportStatus>OK</CurrentTransportStatus>"));
        let own_url = "http://127.0.0.1:9876/token/0";
        let replies = vec![
            ("GET".into(), device_description(port, false)),
            ("GetProtocolInfo".into(), reply("urn:schemas-upnp-org:service:ConnectionManager:1", "GetProtocolInfo", "<Sink>http-get:*:audio/flac:*</Sink>")),
            ("GetTransportInfo".into(), state("STOPPED")),
            ("SetAVTransportURI".into(), reply(avt, "SetAVTransportURI", "")),
            ("Play".into(), reply(avt, "Play", "")),
            ("GetTransportInfo".into(), state("PLAYING")),
            ("GetPositionInfo".into(), reply(avt, "GetPositionInfo", &format!("<TrackURI>{own_url}</TrackURI>"))),
            ("GetTransportInfo".into(), state("STOPPED")),
            ("GetPositionInfo".into(), reply(avt, "GetPositionInfo", "<TrackURI>http://127.0.0.1/another-controller.flac</TrackURI>")),
        ];
        let mock = tokio::spawn(scripted_device(listener, replies));
        let (_keep, signal) = watch::channel(false);
        let error = run(DeviceProtocol::Upnp, &format!("http://127.0.0.1:{port}/description.xml"), &[track("First"), track("Second")], &[own_url.into(), "http://127.0.0.1:9876/token/1".into()], false, signal).await.unwrap_err();
        assert!(error.contains("another controller"), "{error}");
        assert_eq!(mock.await.unwrap().len(), 9);
    });
}

#[test]
fn embedded_renderers_negotiate_only_with_their_own_connection_manager() {
    runtime().block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let device = format!("<root xmlns=\"{DESCRIPTION_NAMESPACE}\"><device><friendlyName>Composite device</friendlyName><serviceList><service><serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType><controlURL>/parent-manager</controlURL></service></serviceList><deviceList><device><friendlyName>Child renderer</friendlyName><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>/child-transport</controlURL></service></serviceList></device></deviceList></device></root>");
        let mock = tokio::spawn(scripted_device(listener, vec![("GET".into(), device)]));
        let (_keep, signal) = watch::channel(false);
        let error = run(DeviceProtocol::Upnp, &format!("http://127.0.0.1:{port}/description.xml"), &[track("First")], &["http://127.0.0.1:9876/token/0".into()], false, signal).await.unwrap_err();
        assert!(error.contains("ConnectionManager"), "{error}");
        assert_eq!(mock.await.unwrap().len(), 1);
    });
}

#[test]
fn openhome_rejects_nonempty_queue_without_stopping_or_deleting_it() {
    runtime().block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let replies = vec![
            ("GET".into(), device_description(port, true)),
            (
                "ProtocolInfo".into(),
                reply(
                    OPENHOME_PLAYLIST,
                    "ProtocolInfo",
                    "<Value>http-get:*:audio/flac:*</Value>",
                ),
            ),
            (
                "TracksMax".into(),
                reply(OPENHOME_PLAYLIST, "TracksMax", "<Value>1000</Value>"),
            ),
            (
                "IdArray".into(),
                reply(
                    OPENHOME_PLAYLIST,
                    "IdArray",
                    "<Token>1</Token><Array>AAAAAQ==</Array>",
                ),
            ),
        ];
        let mock = tokio::spawn(scripted_device(listener, replies));
        let (_keep, signal) = watch::channel(false);
        let error = run(
            DeviceProtocol::Openhome,
            &format!("http://127.0.0.1:{port}/description.xml"),
            &[track("A")],
            &["http://127.0.0.1:9876/token/0".into()],
            false,
            signal,
        )
        .await
        .unwrap_err();
        assert!(error.contains("not empty"), "{error}");
        assert_eq!(mock.await.unwrap().len(), 4);
    });
}

#[test]
fn openhome_loads_ordered_ids_and_retains_completed_queue() {
    runtime().block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let oh = OPENHOME_PLAYLIST;
        let replies = vec![
            ("GET".into(), device_description(port, true)),
            (
                "ProtocolInfo".into(),
                reply(oh, "ProtocolInfo", "<Value>http-get:*:audio/flac:*</Value>"),
            ),
            (
                "TracksMax".into(),
                reply(oh, "TracksMax", "<Value>1000</Value>"),
            ),
            (
                "IdArray".into(),
                reply(oh, "IdArray", "<Token>1</Token><Array>AAAAAQ==</Array>"),
            ),
            ("Repeat".into(), reply(oh, "Repeat", "<Value>false</Value>")),
            (
                "Shuffle".into(),
                reply(oh, "Shuffle", "<Value>false</Value>"),
            ),
            (
                "TransportState".into(),
                reply(oh, "TransportState", "<Value>Stopped</Value>"),
            ),
            ("Stop".into(), reply(oh, "Stop", "")),
            ("DeleteAll".into(), reply(oh, "DeleteAll", "")),
            ("Insert".into(), reply(oh, "Insert", "<NewId>42</NewId>")),
            ("Insert".into(), reply(oh, "Insert", "<NewId>99</NewId>")),
            (
                "IdArray".into(),
                reply(oh, "IdArray", "<Token>3</Token><Array>AAAAKgAAAGM=</Array>"),
            ),
            ("SeekIndex".into(), reply(oh, "SeekIndex", "")),
            ("Play".into(), reply(oh, "Play", "")),
            (
                "IdArrayChanged".into(),
                reply(oh, "IdArrayChanged", "<Value>false</Value>"),
            ),
            (
                "TransportState".into(),
                reply(oh, "TransportState", "<Value>Playing</Value>"),
            ),
            ("Id".into(), reply(oh, "Id", "<Value>42</Value>")),
            (
                "IdArrayChanged".into(),
                reply(oh, "IdArrayChanged", "<Value>false</Value>"),
            ),
            (
                "TransportState".into(),
                reply(oh, "TransportState", "<Value>Stopped</Value>"),
            ),
            ("Id".into(), reply(oh, "Id", "<Value>99</Value>")),
        ];
        let mock = tokio::spawn(scripted_device(listener, replies));
        let (_keep, signal) = watch::channel(false);
        run(
            DeviceProtocol::Openhome,
            &format!("http://127.0.0.1:{port}/description.xml"),
            &[track("First"), track("Second")],
            &[
                "http://127.0.0.1:9876/token/0".into(),
                "http://127.0.0.1:9876/token/1".into(),
            ],
            true,
            signal,
        )
        .await
        .unwrap();
        let requests = mock.await.unwrap();
        for (index, expected) in [(9, "0"), (10, "42")] {
            let request = xml::parse(&requests[index]).unwrap();
            assert_eq!(
                request.child("Body").unwrap().children[0]
                    .value("AfterId")
                    .unwrap(),
                expected
            );
        }
    });
}

#[test]
fn device_http_handles_chunked_descriptions_and_refuses_ambiguous_framing() {
    runtime().block_on(async {
        for response in [
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\ntest\r\n0\r\nX-Test: yes\r\n\r\n",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 4\r\n\r\n4\r\ntest\r\n0\r\n\r\n",
            "HTTP/1.1 302 Found\r\nLocation: http://8.8.8.8/\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let mock = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = [0; 4096];
                assert!(stream.read(&mut buffer).await.unwrap() > 0);
                stream.write_all(response.as_bytes()).await.unwrap();
            });
            let target = DeviceUrl::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
            let result = request(&target, "GET", None, None).await;
            if response.contains("X-Test") { assert_eq!(result.unwrap(), "test"); } else { assert!(result.is_err()); }
            mock.await.unwrap();
        }
    });
}
