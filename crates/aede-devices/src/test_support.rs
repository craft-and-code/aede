//! Runtime shared by the device-only network fixtures.

pub(crate) fn test_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

pub(crate) fn heartbeat_message() -> crate::cast_wire::Message {
    crate::cast_wire::Message {
        source: "sender-1".into(),
        destination: "receiver-0".into(),
        namespace: "urn:x-cast:com.google.cast.tp.heartbeat".into(),
        payload: serde_json::json!({"type":"PING"}),
    }
}

pub(crate) fn track() -> crate::DeviceTrack {
    crate::DeviceTrack {
        path: std::path::PathBuf::from("Example.flac"),
        title: "Example title".into(),
        mime: "audio/flac",
        properties: aede_core::tags::AudioProperties {
            codec: "flac".into(),
            container: "flac".into(),
            sample_rate: Some(44_100),
            bit_depth: Some(16),
            channels: Some(2),
            duration_ms: Some(1000),
            lossless: true,
            ..Default::default()
        },
    }
}

pub(crate) fn tls_material() -> (
    rustls::pki_types::CertificateDer<'static>,
    rustls::pki_types::PrivateKeyDer<'static>,
) {
    use rustls::pki_types::pem::PemObject;
    (
        rustls::pki_types::CertificateDer::from_pem_slice(include_bytes!(
            "../../aede-server/testdata/tls-cert.pem"
        ))
        .unwrap(),
        rustls::pki_types::PrivateKeyDer::from_pem_slice(include_bytes!(
            "../../aede-server/testdata/tls-key.pem"
        ))
        .unwrap(),
    )
}
