//! Explicit protocol options preserve the legacy UPnP request contract.
use serde::Deserialize;
use std::net::Ipv4Addr;
#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Protocol {
    #[default]
    Upnp,
    Openhome,
    Googlecast,
    Slimproto,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Playback {
    pub bind: Ipv4Addr,
    pub device: String,
    pub tracks: Vec<String>,
    #[serde(default)]
    protocol: Protocol,
    #[serde(default)]
    replace: bool,
    certificate_sha256: Option<String>,
}
impl Playback {
    pub(super) fn options(&self) -> Result<aede_devices::CastOptions, String> {
        let options = aede_devices::CastOptions {
            bind: self.bind,
            device: self.device.clone(),
            protocol: match self.protocol {
                Protocol::Upnp => aede_devices::DeviceProtocol::Upnp,
                Protocol::Openhome => aede_devices::DeviceProtocol::Openhome,
                Protocol::Googlecast => aede_devices::DeviceProtocol::Googlecast,
                Protocol::Slimproto => aede_devices::DeviceProtocol::Slimproto,
            },
            replace: self.replace,
            certificate_sha256: self.certificate_sha256.clone(),
            port: None,
            volume: None,
        };
        options.validate()?;
        Ok(options)
    }
}
#[cfg(test)]
#[path = "network_audio_profile_tests.rs"]
mod tests;
