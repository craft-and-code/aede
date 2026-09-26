//! Starts the local API from the command line.

use super::Res;
use crate::args::Args;

#[path = "server_jobs.rs"]
mod server_jobs;

const DEFAULT_PORT: u16 = 8787;

pub fn serve(args: &Args) -> Res {
    let raw_port = args.number_or("port", DEFAULT_PORT as usize)?;
    let port = u16::try_from(raw_port).map_err(|_| "--port must be between 0 and 65535")?;
    let admin_token = match std::env::var("AEDE_ADMIN_TOKEN") {
        Ok(value) if value.len() >= 32 && value.is_ascii() => Some(value),
        Ok(_) => return Err("AEDE_ADMIN_TOKEN must contain at least 32 ASCII characters".into()),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("AEDE_ADMIN_TOKEN must be valid ASCII".into());
        }
    };
    let scan_args = args.clone();
    let job_data = super::data_dir(args);
    aede_server::serve(
        &super::data_dir(args),
        port,
        admin_token.clone(),
        move |progress| {
            super::scan::rescan_with_progress(&scan_args, progress)
                .map_err(|error| error.to_string())
        },
        crate::delegation::allowed,
        move |request, cancellation| server_jobs::run(&job_data, request, cancellation),
        |address| {
            println!("Aède API: http://{address}/api/v1/status");
            if admin_token.is_some() {
                println!("Administrative jobs: POST http://{address}/api/admin/v1/scan or /fetch");
                println!("Server route guide: crates/aede-server/README.md");
            }
        },
    )
}
