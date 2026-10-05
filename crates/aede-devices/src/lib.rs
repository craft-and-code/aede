//! Explicit, finite LAN playback on SlimProto, UPnP AV, OpenHome and Cast devices.
//!
//! This controller is separate from the authenticated catalog API. A local
//! caller selects up to 64 original files and exactly one private IPv4 device.
//! The temporary media listener exposes only that selection, with a random
//! session capability and a matching peer address. No transfer creates a listen
//! or applies server DSP. Physical device compatibility needs real acceptance.
//!
//! [`discover`] and [`cast`] are blocking, explicit operations. They use their
//! own bounded Tokio runtime; [`cast`] monitors Ctrl-C and reports listener
//! addresses on standard output. This crate has no server API dependency or
//! catalog/account mutation. See the crate README and device guide for LAN
//! restrictions and the initial protocol profiles.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::PathBuf;

use aede_core::tags::AudioProperties;
use tokio::sync::watch;

mod cast_discovery;
mod cast_tls;
mod cast_wire;
mod googlecast;
mod media;
/// Shared original-file MIME and HTTP byte-range policy, without API routes.
pub mod original;
mod slimproto;
mod upnp;

pub use upnp::DeviceDescription;

/// The device's control language, independent of the audio file format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceProtocol {
    /// A Squeezebox-compatible player explicitly connects to Aède's TCP port.
    Slimproto,
    /// Aède controls an advertised UPnP AVTransport renderer.
    Upnp,
    /// Aède supplies a finite queue to an OpenHome Playlist renderer.
    Openhome,
    /// Aède supplies original-file URLs to a certificate-pinned Cast receiver.
    Googlecast,
}

/// An original local audio file offered for this one casting session.
#[derive(Clone, Debug)]
pub(crate) struct DeviceTrack {
    pub path: PathBuf,
    pub title: String,
    pub mime: &'static str,
    pub properties: AudioProperties,
}

/// Explicit LAN addresses and queue replacement policy for [`cast`].
#[derive(Clone, Debug)]
pub struct CastOptions {
    /// A specific local private or loopback IPv4 address, never `0.0.0.0`.
    pub bind: Ipv4Addr,
    /// The chosen transport.
    pub protocol: DeviceProtocol,
    /// SlimProto peer IP, Cast `IP[:port]`, or UPnP/OpenHome HTTP description URL.
    pub device: String,
    /// SlimProto listener port; use 3483 normally. Other protocols require None.
    pub port: Option<u16>,
    /// Explicit permission to replace an OpenHome playlist or active Cast session.
    pub replace: bool,
    /// Optional explicit SlimProto linear digital gain percentage, 0 to 100.
    /// None preserves device gain; fresh software players may be muted.
    pub volume: Option<u8>,
    /// Exact trusted receiver leaf-certificate SHA-256, for Google Cast only.
    /// This explicit pin replaces CA/hostname trust; no insecure TLS mode exists.
    pub certificate_sha256: Option<String>,
}

pub(crate) fn validate_ip(ip: Ipv4Addr) -> Result<(), String> {
    if ip.is_unspecified()
        || ip.is_broadcast()
        || !(ip.is_private() || ip.is_loopback() || ip.is_link_local())
    {
        return Err(
            "device playback requires a specific private, link-local or loopback IPv4 address"
                .into(),
        );
    }
    Ok(())
}

impl CastOptions {
    /// Validate every option before opening files or network listeners.
    pub fn validate(&self) -> Result<Ipv4Addr, String> {
        validate_ip(self.bind)?;
        if self.volume.is_some_and(|volume| volume > 100) {
            return Err("--device-volume must be between 0 and 100".into());
        }
        if self.protocol != DeviceProtocol::Googlecast && self.certificate_sha256.is_some() {
            return Err("--device-certificate is only supported by Google Cast".into());
        }
        match self.protocol {
            DeviceProtocol::Slimproto => {
                if self.replace {
                    return Err("--replace is only supported by OpenHome".into());
                }
                if self.port == Some(0) {
                    return Err("the SlimProto listener needs a non-zero port".into());
                }
                let peer = self
                    .device
                    .parse::<Ipv4Addr>()
                    .map_err(|_| "SlimProto --device must be a literal IPv4 address")?;
                validate_ip(peer)?;
                Ok(peer)
            }
            DeviceProtocol::Upnp | DeviceProtocol::Openhome => {
                if self.volume.is_some() {
                    return Err("--device-volume is only supported by SlimProto".into());
                }
                if self.port.is_some() {
                    return Err("--port is only supported by SlimProto".into());
                }
                if self.replace && self.protocol != DeviceProtocol::Openhome {
                    return Err("--replace is only supported by OpenHome".into());
                }
                let peer = upnp::device_ip(&self.device)?;
                validate_ip(peer)?;
                Ok(peer)
            }
            DeviceProtocol::Googlecast => {
                if self.port.is_some() || self.volume.is_some() {
                    return Err("Google Cast uses IP:PORT in --device and preserves volume; --port and --device-volume are SlimProto-only".into());
                }
                cast_tls::parse_pin(self.certificate_sha256.as_deref().ok_or(
                    "Google Cast requires --device-certificate SHA256; inspect it with aede devices --protocol googlecast --bind IP --device IP and verify it on a trusted LAN")?)?;
                Ok(*googlecast::endpoint(&self.device)?.ip())
            }
        }
    }
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|error| format!("cannot start the device controller: {error}"))
}

async fn finish_controller(
    result: Result<(), String>,
    signal_task: tokio::task::JoinHandle<Result<(), String>>,
) -> Result<(), String> {
    signal_task.abort();
    let signal_result = match signal_task.await {
        Ok(result) => result,
        Err(error) if error.is_cancelled() => Ok(()),
        Err(error) => Err(format!("Ctrl-C monitor failed: {error}")),
    };
    match (result, signal_result) {
        (Err(transport), Err(signal)) => Err(format!("{transport}; {signal}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

/// Perform one bounded SSDP discovery on an explicitly selected interface.
///
/// Network activity occurs only during this call. Devices outside the LAN IP
/// policy, cross-host descriptions and unsupported services are not followed.
pub fn discover(bind: Ipv4Addr) -> Result<Vec<DeviceDescription>, String> {
    validate_ip(bind)?;
    runtime()?.block_on(upnp::discover(bind))
}

/// Perform one bounded mDNS discovery of Google Cast endpoints on this interface.
///
/// Discovery is unauthenticated and never grants trust to an advertised device.
/// Complete responses must bind their SRV/A records to the responding LAN IP.
pub fn discover_cast(bind: Ipv4Addr) -> Result<Vec<DeviceDescription>, String> {
    validate_ip(bind)?;
    runtime()?.block_on(cast_discovery::discover(bind))
}

/// Observe a Google Cast endpoint's leaf-certificate SHA-256 without casting.
///
/// The TLS handshake is deliberately rejected before application data. This is
/// an unauthenticated observation, not automatic pairing or manufacturer trust;
/// verify the endpoint on a trusted LAN before using this value as a cast pin.
pub fn inspect_cast_certificate(bind: Ipv4Addr, device: &str) -> Result<String, String> {
    validate_ip(bind)?;
    let peer = googlecast::endpoint(device)?;
    runtime()?.block_on(cast_tls::inspect(bind, *peer.ip(), peer.port()))
}

/// Play a finite selection on exactly one LAN device until completion or Ctrl-C.
///
/// Files are opened read-only and sent unchanged, without normalization or
/// transcoding. Adapters preflight advertised or conservatively allowed formats. This method
/// does not create listening history: transport/device status is not a native
/// PCM consumption acknowledgement. A request to stop is sent on Ctrl-C;
/// a powered-off or disconnected device cannot acknowledge that request.
pub fn cast(options: CastOptions, paths: Vec<PathBuf>) -> Result<(), String> {
    let peer = options.validate()?;
    let sources = media::prepare(paths)?;
    let tracks = sources
        .iter()
        .map(|source| source.track.clone())
        .collect::<Vec<_>>();
    if options.protocol == DeviceProtocol::Googlecast {
        googlecast::preflight(&tracks)?;
    }
    let runtime = runtime()?;
    let result = runtime.block_on(async move {
        let media = media::MediaServer::start(options.bind, peer, sources).await?;
        let (shutdown, signal) = watch::channel(false);
        let signal_task = tokio::spawn(async move {
            let result = tokio::signal::ctrl_c()
                .await
                .map_err(|error| format!("cannot monitor Ctrl-C: {error}"));
            let _ = shutdown.send(true);
            result
        });
        println!(
            "Device media: {} (selected originals only; Ctrl-C stops playback)",
            media.address()
        );
        let result = match options.protocol {
            DeviceProtocol::Slimproto => {
                match tokio::net::TcpListener::bind(SocketAddrV4::new(
                    options.bind,
                    options.port.unwrap_or(3483),
                ))
                .await
                {
                    Ok(listener) => {
                        println!(
                            "SlimProto: {} — connect the selected player to this address",
                            listener.local_addr().map_err(|error| error.to_string())?
                        );
                        slimproto::run(
                            listener,
                            peer,
                            &tracks,
                            media.urls(),
                            options.volume,
                            signal,
                        )
                        .await
                    }
                    Err(error) => Err(format!("cannot open the SlimProto listener: {error}")),
                }
            }
            DeviceProtocol::Googlecast => {
                googlecast::run(
                    options.bind,
                    &options.device,
                    options
                        .certificate_sha256
                        .as_deref()
                        .ok_or("missing Cast certificate pin")?,
                    &tracks,
                    media.urls(),
                    options.replace,
                    signal,
                )
                .await
            }
            protocol => {
                upnp::run(
                    protocol,
                    &options.device,
                    &tracks,
                    media.urls(),
                    options.replace,
                    signal,
                )
                .await
            }
        };
        let result = finish_controller(result, signal_task).await;
        media.stop().await;
        result
    });
    // A stuck OS/filesystem operation must not prevent explicit shutdown.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    result
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "test_support.rs"]
mod test_support;
