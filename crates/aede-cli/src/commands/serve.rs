//! Starts the catalog API from the command line.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use super::Res;
use crate::args::Args;

#[path = "server_jobs.rs"]
mod server_jobs;

const DEFAULT_PORT: u16 = 8787;

fn server_options(args: &Args) -> Result<aede_server::ServerOptions, String> {
    let raw_port = args.number_or("port", DEFAULT_PORT as usize)?;
    let port = u16::try_from(raw_port).map_err(|_| "--port must be between 0 and 65535")?;
    let bind = args
        .value("bind")
        .unwrap_or("127.0.0.1")
        .parse::<IpAddr>()
        .map_err(|_| "--bind expects one literal IPv4 or IPv6 address")?;
    let bind = SocketAddr::new(bind, port);
    let certificate = args.value("tls-cert");
    let private_key = args.value("tls-key");
    let authority = args.value("authority");
    let tls = match (certificate, private_key, authority) {
        (None, None, None) => {
            if !bind.ip().is_loopback() {
                return Err(
                    "a non-loopback --bind requires --tls-cert, --tls-key and --authority".into(),
                );
            }
            None
        }
        (Some(certificate), Some(private_key), Some(authority)) => {
            if port == 0 {
                return Err("--port=0 cannot be used with TLS; choose a fixed HTTPS port".into());
            }
            let certificate = path_option("tls-cert", certificate)?;
            let private_key = path_option("tls-key", private_key)?;
            validate_authority(authority)?;
            Some(aede_server::TlsOptions {
                certificate,
                private_key,
                authority: authority.into(),
            })
        }
        _ => {
            return Err("--tls-cert, --tls-key and --authority must be provided together".into());
        }
    };
    Ok(aede_server::ServerOptions { bind, tls })
}

fn path_option(name: &str, value: &str) -> Result<PathBuf, String> {
    if value.is_empty() {
        return Err(format!("--{name} needs a nonempty path"));
    }
    Ok(PathBuf::from(value))
}

/// Add command-line context to the server's canonical Host/Origin rule.
fn validate_authority(value: &str) -> Result<(), String> {
    aede_server::TlsOptions::validate_authority(value).map_err(|failure| {
        format!("--authority expects an exact HOST:PORT without a scheme or path: {failure}")
    })
}

pub fn serve(args: &Args) -> Res {
    let options = server_options(args)?;
    let admin_token = match std::env::var("AEDE_ADMIN_TOKEN") {
        Ok(value) if value.len() >= 32 && value.is_ascii() => Some(value),
        Ok(_) => return Err("AEDE_ADMIN_TOKEN must contain at least 32 ASCII characters".into()),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("AEDE_ADMIN_TOKEN must be valid ASCII".into());
        }
    };
    let scan_args = args.clone();
    let data_dir = super::data_dir(args);
    let job_data = data_dir.clone();
    let tls_authority = options.tls.as_ref().map(|tls| tls.authority.clone());
    let local_administration = tls_authority.is_none() && admin_token.is_some();
    aede_server::serve_with_options(
        &data_dir,
        options,
        admin_token,
        move |progress| {
            super::scan::rescan_with_progress(&scan_args, progress)
                .map_err(|error| error.to_string())
        },
        crate::delegation::allowed,
        move |request, cancellation| server_jobs::run(&job_data, request, cancellation),
        |address| {
            if let Some(authority) = tls_authority {
                println!("Aède API: https://{authority}/api/v1/status");
                println!("Server route guide: crates/aede-server/README.md");
            } else {
                println!("Aède API: http://{address}/api/v1/status");
            }
            if local_administration {
                println!("Administrative jobs: POST http://{address}/api/admin/v1/scan or /fetch");
                println!("Server route guide: crates/aede-server/README.md");
            }
        },
    )
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
