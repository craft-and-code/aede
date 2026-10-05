//! Explicit discovery and finite playback on one LAN audio device.

use std::net::Ipv4Addr;

use aede_devices::{CastOptions, DeviceProtocol};

use super::Res;
use crate::args::Args;
use crate::ui::{self, Table};

fn bind(args: &Args) -> Result<Ipv4Addr, String> {
    args.value("bind")
        .ok_or("select this machine's LAN IPv4 address with --bind")?
        .parse()
        .map_err(|_| "--bind must be a literal IPv4 address".into())
}

fn options(args: &Args) -> Result<CastOptions, String> {
    let protocol = match args.value("protocol") {
        Some("slimproto") => DeviceProtocol::Slimproto,
        Some("upnp") => DeviceProtocol::Upnp,
        Some("openhome") => DeviceProtocol::Openhome,
        _ => return Err("--protocol must be slimproto, upnp or openhome".into()),
    };
    let port = if args.has("port") {
        Some(
            args.value("port")
                .ok_or("--port needs a value")?
                .parse::<u16>()
                .map_err(|_| "--port needs a non-zero TCP port")?,
        )
    } else {
        None
    };
    let options = CastOptions {
        bind: bind(args)?,
        protocol,
        device: args
            .value("device")
            .ok_or("--device needs the player IP or device-description URL")?
            .into(),
        port,
        replace: args.has("replace"),
        volume: if args.has("device-volume") {
            Some(
                args.value("device-volume")
                    .ok_or("--device-volume needs a percentage")?
                    .parse::<u8>()
                    .map_err(|_| "--device-volume must be an integer between 0 and 100")?,
            )
        } else {
            None
        },
    };
    options.validate()?;
    Ok(options)
}

pub fn devices(args: &Args) -> Res {
    if !args.positionals.is_empty() {
        return Err("aede devices only lists discovered LAN audio devices".into());
    }
    let devices = aede_devices::discover(bind(args)?)?;
    println!("{}", ui::section("Network audio devices"));
    if devices.is_empty() {
        println!("  no UPnP/OpenHome audio device answered on this interface");
    } else {
        let mut table = Table::new(&["Device", "Protocols", "Description URL"]);
        for device in devices {
            table.push(vec![
                ui::literal(&device.name),
                device.protocols.join(", "),
                device.location,
            ]);
        }
        print!("{}", table.render());
    }
    Ok(())
}

pub fn cast(args: &Args) -> Res {
    let options = options(args)?;
    let paths = super::play::selected_paths(args)?;
    println!(
        "Casting {} selection occurrence(s); original audio, device decoding",
        paths.len()
    );
    aede_devices::cast(options, paths).map_err(Into::into)
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod tests;
