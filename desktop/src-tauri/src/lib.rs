//! Minimal native bridge for the server launcher. No shell or arbitrary HTTP/file API.
#![warn(missing_docs)]
use aede_desktop_core::Host;
use serde_json::Value;
use std::{
    io::Write,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
mod maintenance;
struct Server(Arc<Mutex<Host>>);
fn bridge(version: u32) -> Result<(), String> {
    if version == 1 {
        Ok(())
    } else {
        Err("Pont desktop incompatible. Mettez l’application à jour.".into())
    }
}
async fn work<T: Send + 'static>(
    state: &Server,
    action: impl FnOnce(&mut Host) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let host = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut host = host.lock().map_err(|_| "État du serveur indisponible.")?;
        action(&mut host)
    })
    .await
    .map_err(|_| "Opération native interrompue.")?
}
fn status(host: &mut Host) -> Result<Value, String> {
    let mut value =
        serde_json::to_value(host.status()?).map_err(|_| "État du serveur illisible.")?;
    value["version"] = env!("CARGO_PKG_VERSION").into();
    value["engines"] = serde_json::from_str(include_str!("../binaries/versions.json"))
        .map_err(|_| "Versions du moteur illisibles.")?;
    Ok(value)
}
#[tauri::command]
async fn host_status(version: u32, state: State<'_, Server>) -> Result<Value, String> {
    bridge(version)?;
    work(&state, status).await
}
#[tauri::command]
async fn host_start(version: u32, state: State<'_, Server>) -> Result<Value, String> {
    bridge(version)?;
    work(&state, |host| {
        host.start()?;
        status(host)
    })
    .await
}
#[tauri::command]
async fn host_stop(version: u32, state: State<'_, Server>) -> Result<Value, String> {
    bridge(version)?;
    work(&state, |host| {
        host.stop()?;
        status(host)
    })
    .await
}
#[tauri::command]
async fn host_configure(
    version: u32,
    state: State<'_, Server>,
    port: u16,
    roots: Vec<String>,
) -> Result<Value, String> {
    bridge(version)?;
    work(&state, move |host| {
        host.configure(port, roots)?;
        status(host)
    })
    .await
}
#[tauri::command]
async fn host_folders(version: u32, app: AppHandle) -> Result<Vec<String>, String> {
    bridge(version)?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(files) = app
            .dialog()
            .file()
            .set_title("Dossiers de musique")
            .blocking_pick_folders()
        else {
            return Ok(Vec::new());
        };
        files
            .into_iter()
            .map(|file| {
                file.into_path()
                    .map_err(|_| "Dossier invalide.".to_string())?
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Le chemin doit être UTF-8.".into())
            })
            .collect()
    })
    .await
    .map_err(|_| "Sélection interrompue.")?
}
#[tauri::command]
async fn host_scan(version: u32, state: State<'_, Server>) -> Result<Value, String> {
    bridge(version)?;
    work(&state, Host::scan).await
}
#[tauri::command]
async fn host_task(
    version: u32,
    state: State<'_, Server>,
    id: u64,
    cancel: bool,
) -> Result<Value, String> {
    bridge(version)?;
    work(&state, move |host| host.task(id, cancel)).await
}
#[tauri::command]
async fn host_password(version: u32, state: State<'_, Server>) -> Result<String, String> {
    bridge(version)?;
    work(&state, |host| host.password()).await
}
#[tauri::command]
async fn host_logs(
    version: u32,
    state: State<'_, Server>,
    after: u64,
) -> Result<aede_desktop_core::LogSnapshot, String> {
    bridge(version)?;
    work(&state, move |host| host.logs(after)).await
}
#[tauri::command]
async fn host_clear_logs(version: u32, state: State<'_, Server>) -> Result<(), String> {
    bridge(version)?;
    work(&state, |host| host.clear_logs()).await
}
#[tauri::command]
async fn host_diagnostics(
    version: u32,
    state: State<'_, Server>,
) -> Result<aede_desktop_core::Diagnostics, String> {
    bridge(version)?;
    work(&state, Host::diagnostics).await
}
#[tauri::command]
async fn host_copy_address(
    version: u32,
    state: State<'_, Server>,
    app: AppHandle,
) -> Result<(), String> {
    bridge(version)?;
    let address = work(&state, Host::address).await?;
    app.clipboard()
        .write_text(address)
        .map_err(|_| "Copie de l’adresse impossible.".into())
}
#[tauri::command]
async fn host_copy_password(
    version: u32,
    state: State<'_, Server>,
    app: AppHandle,
) -> Result<(), String> {
    bridge(version)?;
    let password = work(&state, |host| host.password()).await?;
    app.clipboard()
        .write_secret(password)
        .map_err(|_| "Copie du mot de passe impossible.".into())
}
#[tauri::command]
async fn host_copy_username(
    version: u32,
    state: State<'_, Server>,
    app: AppHandle,
) -> Result<(), String> {
    bridge(version)?;
    let username = work(&state, |host| Ok(host.username().to_owned())).await?;
    app.clipboard()
        .write_text(username)
        .map_err(|_| "Copie du login impossible.".into())
}
#[tauri::command]
async fn host_export_logs(
    version: u32,
    state: State<'_, Server>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    bridge(version)?;
    let text = work(&state, |host| host.export_logs()).await?;
    save_export(app, text, "Journaux Aède", "aede-journaux.json").await
}
#[tauri::command]
async fn host_export(
    version: u32,
    state: State<'_, Server>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    bridge(version)?;
    let profile = work(&state, |host| host.connection_profile()).await?;
    save_export(app, profile, "Connexion Aède", "aede-connection.json").await
}
// Only fixed native-generated exports reach this helper; no webview path or content is admitted.
async fn save_export(
    app: AppHandle,
    text: String,
    label: &'static str,
    filename: &'static str,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app
            .dialog()
            .file()
            .set_title(label)
            .add_filter(label, &["json"])
            .set_file_name(filename);
        if let Ok(folder) = app.path().download_dir() {
            dialog = dialog.set_directory(folder);
        }
        let Some(file) = dialog.blocking_save_file() else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|_| "Destination invalide.")?;
        let folder = path.parent().ok_or("Destination invalide.")?;
        let temporary = folder.join(format!(".{filename}-{}.tmp", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| "Export occupé ou impossible.".to_string())?;
        let result = (|| {
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, &path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(|_| "Export impossible.".to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|_| "Export interrompu.")?
}
/// Run the server application; application exit drops its owned server lifetime lease.
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let binary = std::env::current_exe()?
                .parent()
                .ok_or("Dossier de l’application indisponible")?
                .join(if cfg!(windows) {
                    "aede-engine.exe"
                } else {
                    "aede-engine"
                });
            let profile = app.path().app_data_dir()?.join("server");
            let host = Host::new(profile, binary).map_err(std::io::Error::other)?;
            app.manage(Server(Arc::new(Mutex::new(host))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            host_status,
            host_start,
            host_stop,
            host_configure,
            host_folders,
            host_scan,
            host_task,
            host_password,
            host_export,
            host_logs,
            host_clear_logs,
            host_diagnostics,
            host_copy_address,
            host_copy_password,
            host_copy_username,
            host_export_logs,
            maintenance::host_open_data,
            maintenance::host_regenerate_password,
            maintenance::host_new_server
        ])
        .run(tauri::generate_context!())
}
