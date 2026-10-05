//! Exact certificate pins authenticate the selected LAN endpoint, without Web PKI.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error, SignatureScheme};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

pub(super) fn parse_pin(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("--device-certificate needs exactly 64 SHA-256 hexadecimal digits".into());
    }
    let mut pin = [0; 32];
    for (pair, output) in value.as_bytes().chunks_exact(2).zip(&mut pin) {
        let high = (pair[0] as char)
            .to_digit(16)
            .ok_or("invalid certificate pin")?;
        let low = (pair[1] as char)
            .to_digit(16)
            .ok_or("invalid certificate pin")?;
        *output = (high * 16 + low) as u8;
    }
    Ok(pin)
}

fn digest(input: &[u8]) -> Result<[u8; 32], Error> {
    let suite = rustls::crypto::ring::cipher_suite::TLS13_AES_128_GCM_SHA256;
    let hash = suite
        .tls13()
        .ok_or_else(|| Error::General("SHA-256 unavailable".into()))?
        .common
        .hash_provider
        .hash(input);
    hash.as_ref()
        .try_into()
        .map_err(|_| Error::General("invalid SHA-256 length".into()))
}

#[derive(Debug)]
enum PinnedCertificate {
    Trusted([u8; 32]),
    Observe(Arc<Mutex<Option<[u8; 32]>>>),
}

impl ServerCertVerifier for PinnedCertificate {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let fingerprint = digest(end_entity.as_ref())?;
        let pin = match self {
            Self::Trusted(pin) => pin,
            Self::Observe(observed) => {
                *observed
                    .lock()
                    .map_err(|_| Error::General("certificate observer failed".into()))? =
                    Some(fingerprint);
                return Err(Error::General(
                    "certificate inspection only; handshake deliberately refused".into(),
                ));
            }
        };
        if fingerprint != *pin {
            return Err(Error::General("Cast certificate does not match --device-certificate; verify the receiver before updating its pin".into()));
        }
        // The exact user-trusted leaf replaces CA/hostname validation. The TLS
        // handshake still proves possession of its private key below.
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            signature,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            signature,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub(super) async fn connect(
    bind: Ipv4Addr,
    peer: Ipv4Addr,
    port: u16,
    pin: [u8; 32],
) -> Result<TlsStream<TcpStream>, String> {
    connect_with(bind, peer, port, PinnedCertificate::Trusted(pin)).await
}

pub(super) async fn inspect(bind: Ipv4Addr, peer: Ipv4Addr, port: u16) -> Result<String, String> {
    let observed = Arc::new(Mutex::new(None));
    let result = connect_with(
        bind,
        peer,
        port,
        PinnedCertificate::Observe(observed.clone()),
    )
    .await;
    let pin = *observed.lock().map_err(|_| "certificate observer failed")?;
    match pin {
        Some(pin) => Ok(pin.iter().map(|byte| format!("{byte:02x}")).collect()),
        None => Err(result
            .err()
            .unwrap_or_else(|| "Cast receiver supplied no certificate".into())),
    }
}

async fn connect_with(
    bind: Ipv4Addr,
    peer: Ipv4Addr,
    port: u16,
    verifier: PinnedCertificate,
) -> Result<TlsStream<TcpStream>, String> {
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|error| error.to_string())?
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(verifier))
    .with_no_client_auth();
    let socket = tokio::net::TcpSocket::new_v4().map_err(|error| error.to_string())?;
    socket
        .bind(SocketAddrV4::new(bind, 0).into())
        .map_err(|error| error.to_string())?;
    tokio::time::timeout(Duration::from_secs(8), async {
        let stream = socket
            .connect(SocketAddrV4::new(peer, port).into())
            .await
            .map_err(|error| format!("Cast connection: {error}"))?;
        let name = ServerName::IpAddress(peer.into());
        TlsConnector::from(Arc::new(config))
            .connect(name, stream)
            .await
            .map_err(|error| format!("Cast TLS: {error}"))
    })
    .await
    .map_err(|_| "Cast connection/TLS timed out".to_owned())?
}

#[cfg(test)]
#[path = "cast_tls_tests.rs"]
mod tests;
