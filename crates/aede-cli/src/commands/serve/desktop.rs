//! Parent-owned desktop bootstrap with bounded stdin credentials and an EOF lifetime lease.
use aede_core::{accounts, clock, json, model, store, store_lock::StoreLock};
use std::io::{BufRead, Read};
use std::path::Path;
use std::sync::Arc;

fn bootstrap(data: &Path, input: &str) -> Result<Option<aede_server::DesktopTls>, String> {
    let request = json::parse(input).map_err(|_| "invalid desktop bootstrap")?;
    let json::Json::Obj(fields) = &request else {
        return Err("invalid desktop bootstrap".into());
    };
    if fields.len() != 2 && !(fields.len() == 3 && request.get("tls").is_some()) {
        return Err("invalid desktop bootstrap".into());
    }
    let tls = request
        .get("tls")
        .map(|value| {
            let json::Json::Obj(fields) = value else {
                return Err("invalid desktop TLS identity".into());
            };
            if fields.len() != 2 {
                return Err("invalid desktop TLS identity".into());
            }
            aede_server::DesktopTls::from_pem(
                value
                    .get("certificate")
                    .and_then(json::Json::as_str)
                    .ok_or("missing desktop TLS certificate")?,
                value
                    .get("private_key")
                    .and_then(json::Json::as_str)
                    .ok_or("missing desktop TLS private key")?,
            )
        })
        .transpose()?;
    let username = request
        .get("username")
        .and_then(json::Json::as_str)
        .ok_or("missing desktop username")?;
    let password = request
        .get("password")
        .and_then(json::Json::as_str)
        .ok_or("missing desktop password")?;
    let _guard = StoreLock::acquire(data).map_err(|_| "desktop data is busy")?;
    let path = store::catalog_path(data);
    if store::load(&path)
        .map_err(|_| "desktop catalog is unreadable")?
        .is_none()
    {
        store::save(
            &model::build(Vec::new(), Vec::new(), clock::now_seconds(), &[]),
            &path,
        )
        .map_err(|_| "desktop catalog could not be created")?;
    }
    let path = accounts::accounts_path(data);
    match accounts::load(&path).map_err(|_| "desktop credentials are unreadable")? {
        Some(current) => {
            if !current
                .authenticate(username, password)
                .is_some_and(|account| account.role == accounts::Role::Administrator)
            {
                return Err("desktop credentials do not match the retained administrator".into());
            }
        }
        None => {
            let current = accounts::Accounts::bootstrap(username, password, clock::now_seconds())?;
            accounts::save(&current, &path)
                .map_err(|_| "desktop credentials could not be saved")?;
        }
    }
    Ok(tls)
}

pub(super) fn prepare(
    data: &Path,
) -> Result<
    (
        Arc<aede_server::DesktopControl>,
        Option<aede_server::DesktopTls>,
    ),
    String,
> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(32769)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "desktop bootstrap input failed")?;
    if bytes.len() > 32768 || bytes.last() != Some(&b'\n') {
        return Err("desktop bootstrap input is oversized or incomplete".into());
    }
    let tls = bootstrap(
        data,
        std::str::from_utf8(&bytes).map_err(|_| "desktop bootstrap is not UTF-8")?,
    )?;
    let control = Arc::new(aede_server::DesktopControl::default());
    let monitor = control.clone();
    std::thread::spawn(move || {
        if let Err(error) = read_control(std::io::stdin().lock(), |force| {
            if force {
                monitor.force_stop();
            } else {
                monitor.shutdown();
            }
        }) {
            eprintln!("Aède desktop control failed: {error}");
            monitor.shutdown();
        }
    });
    Ok((control, tls))
}

fn read_control(mut input: impl BufRead, mut apply: impl FnMut(bool)) -> Result<(), String> {
    loop {
        let mut line = Vec::new();
        input
            .by_ref()
            .take(129)
            .read_until(b'\n', &mut line)
            .map_err(|_| "desktop control input failed")?;
        if line.is_empty() {
            apply(false);
            return Ok(());
        }
        match line.as_slice() {
            b"{\"desktopCommand\":\"shutdown\"}\n" => apply(false),
            b"{\"desktopCommand\":\"force-stop\"}\n" => apply(true),
            _ => return Err("invalid or oversized desktop control command".into()),
        }
    }
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;
