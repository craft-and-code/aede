//! TLS listener configuration and bounded PEM loading.

use std::fs;
use std::io::{self, Read as _};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::ServerConfig;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::TlsAcceptor;

use super::security::RemoteAuthority;

pub(super) const MAX_PEM_BYTES: u64 = 256 * 1024;
const MAX_CERTIFICATES: usize = 16;

/// Listener configuration for the Aède HTTP API.
///
/// A listener without [`TlsOptions`] is deliberately restricted to a
/// loopback-only API. Supplying TLS options creates the authenticated HTTPS
/// listener used for remote clients and requires initialized accounts, even
/// when its socket itself is loopback-only.
#[derive(Clone, Debug)]
pub struct ServerOptions {
    /// Socket address where the API accepts TCP connections.
    pub bind: SocketAddr,
    /// Certificate, private key and public authority for HTTPS, when enabled.
    pub tls: Option<TlsOptions>,
}

impl ServerOptions {
    /// Construct the backwards-compatible local HTTP listener.
    pub fn loopback(port: u16) -> Self {
        Self {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
            tls: None,
        }
    }
}

/// Explicit certificate material and browser authority for an HTTPS listener.
///
/// Certificate and key files must be regular PEM files no larger than 256 KiB.
/// On Unix, the private key must not grant any group or other-user permission.
#[derive(Clone, Debug)]
pub struct TlsOptions {
    /// PEM certificate chain sent to clients, with the leaf certificate first.
    pub certificate: PathBuf,
    /// PEM private key matching the leaf certificate.
    pub private_key: PathBuf,
    /// Exact public HTTPS authority, including its non-zero port, such as
    /// `music-box.home:8443`.
    ///
    /// It accepts a DNS name or bracketed IP literal, never a URL, path,
    /// wildcard or user information. It can differ from [`ServerOptions::bind`]
    /// only when the surrounding network preserves TLS while forwarding to the
    /// listener.
    pub authority: String,
}

impl TlsOptions {
    /// Validate the exact `HOST:PORT` authority presented to HTTPS clients.
    ///
    /// This accepts a DNS name or bracketed IP literal with a non-zero,
    /// canonical decimal port. It rejects URLs, paths, wildcards, user
    /// information and alternate port spellings, so command-line validation
    /// uses precisely the same rule as the HTTP Host/Origin boundary.
    pub fn validate_authority(authority: &str) -> Result<(), String> {
        RemoteAuthority::parse(authority).map(|_| ())
    }
}

pub(super) struct TlsServer {
    pub(super) acceptor: TlsAcceptor,
    pub(super) authority: RemoteAuthority,
}

pub(super) fn configure(options: &ServerOptions) -> io::Result<Option<TlsServer>> {
    let Some(tls) = &options.tls else {
        if !options.bind.ip().is_loopback() {
            return Err(invalid(
                "an unencrypted server may only bind a loopback address; configure TLS for remote access",
            ));
        }
        return Ok(None);
    };
    if options.bind.port() == 0 {
        return Err(invalid(
            "an HTTPS listener needs an explicit port so its authority can be checked",
        ));
    }
    let authority = RemoteAuthority::parse(&tls.authority).map_err(invalid)?;
    let certificates = certificates(&tls.certificate)?;
    let private_key = private_key(&tls.private_key)?;
    let mut configuration = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .map_err(|error| {
            invalid(format!(
                "TLS certificate and private key do not match: {error}"
            ))
        })?;
    // The API currently speaks the existing HTTP/1 and WebSocket contract.
    // Advertising only HTTP/1.1 prevents a client from selecting unsupported
    // HTTP/2 after a successful TLS handshake.
    configuration.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Some(TlsServer {
        acceptor: TlsAcceptor::from(Arc::new(configuration)),
        authority,
    }))
}

fn certificates(path: &Path) -> io::Result<Vec<CertificateDer<'static>>> {
    let input = read_pem(path, "certificate", false)?;
    let certificates = CertificateDer::pem_slice_iter(&input)
        .take(MAX_CERTIFICATES + 1)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| invalid(format!("TLS certificate is not valid PEM: {error}")))?;
    if certificates.is_empty() {
        return Err(invalid("TLS certificate file contains no certificate"));
    }
    if certificates.len() > MAX_CERTIFICATES {
        return Err(invalid("TLS certificate chain has too many certificates"));
    }
    Ok(certificates)
}

fn private_key(path: &Path) -> io::Result<PrivateKeyDer<'static>> {
    let input = read_pem(path, "private key", true)?;
    PrivateKeyDer::from_pem_slice(&input)
        .map_err(|error| invalid(format!("TLS private key is not valid PEM: {error}")))
}

fn read_pem(path: &Path, kind: &str, secret: bool) -> io::Result<Vec<u8>> {
    let link_metadata = fs::symlink_metadata(path).map_err(|error| {
        invalid(format!(
            "TLS {kind} cannot be inspected at {}: {error}",
            path.display()
        ))
    })?;
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return Err(invalid(format!("TLS {kind} must be a regular file")));
    }
    #[cfg(unix)]
    if secret {
        use std::os::unix::fs::PermissionsExt as _;
        if link_metadata.permissions().mode() & 0o077 != 0 {
            return Err(invalid(
                "TLS private key must not be readable or writable by group or other users",
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = secret;
    if link_metadata.len() > MAX_PEM_BYTES {
        return Err(invalid(format!("TLS {kind} exceeds the 256 KiB limit")));
    }
    let mut file = fs::File::open(path)
        .map_err(|error| invalid(format!("TLS {kind} cannot be opened: {error}")))?;
    let metadata = file
        .metadata()
        .map_err(|error| invalid(format!("TLS {kind} cannot be inspected: {error}")))?;
    if !metadata.is_file() || metadata.len() > MAX_PEM_BYTES {
        return Err(invalid(format!("TLS {kind} is not a bounded regular file")));
    }
    // Reinspect the opened descriptor as well as the path. This catches a
    // replacement between `symlink_metadata` and `open`, including a
    // replacement by a symlink. The descriptor is what will actually be
    // read, so its access mode is the relevant one for private keys.
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
        if link_metadata.dev() != metadata.dev() || link_metadata.ino() != metadata.ino() {
            return Err(invalid(format!("TLS {kind} changed while it was opened")));
        }
        if secret && metadata.permissions().mode() & 0o077 != 0 {
            return Err(invalid(
                "TLS private key must not be readable or writable by group or other users",
            ));
        }
    }
    let mut input = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.by_ref()
        .take(MAX_PEM_BYTES + 1)
        .read_to_end(&mut input)
        .map_err(|error| invalid(format!("TLS {kind} cannot be read: {error}")))?;
    if input.len() > MAX_PEM_BYTES as usize {
        return Err(invalid(format!("TLS {kind} exceeds the 256 KiB limit")));
    }
    Ok(input)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
