//! Parent-owned desktop bootstrap with bounded stdin credentials and an EOF lifetime lease.
use aede_core::{accounts, clock, json, model, store, store_lock::StoreLock};
use std::io::{BufRead, Read};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn bootstrap(data: &Path, input: &str) -> Result<(), String> {
    let request = json::parse(input).map_err(|_| "invalid desktop bootstrap")?;
    let json::Json::Obj(fields) = &request else {
        return Err("invalid desktop bootstrap".into());
    };
    if fields.len() != 2 {
        return Err("invalid desktop bootstrap".into());
    }
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
    Ok(())
}

pub(super) fn prepare(data: &Path) -> Result<Arc<AtomicBool>, String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(2049)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "desktop bootstrap input failed")?;
    if bytes.len() > 2048 || bytes.last() != Some(&b'\n') {
        return Err("desktop bootstrap input is oversized or incomplete".into());
    }
    bootstrap(
        data,
        std::str::from_utf8(&bytes).map_err(|_| "desktop bootstrap is not UTF-8")?,
    )?;
    let closed = Arc::new(AtomicBool::new(false));
    let flag = closed.clone();
    std::thread::spawn(move || {
        let mut buffer = [0; 256];
        loop {
            match std::io::stdin().read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
        flag.store(true, Ordering::Release);
    });
    Ok(closed)
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;
