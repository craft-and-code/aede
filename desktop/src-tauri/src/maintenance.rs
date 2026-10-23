//! Fixed native maintenance actions; paths and secrets are never supplied by the webview.
use super::{Server, bridge, work};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

#[tauri::command]
pub(crate) async fn host_open_data(version: u32, state: State<'_, Server>) -> Result<(), String> {
    bridge(version)?;
    let path = work(&state, |host| host.profile_directory()).await?;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        let mut command = std::process::Command::new("/usr/bin/open");
        #[cfg(target_os = "linux")]
        let mut command = std::process::Command::new("xdg-open");
        #[cfg(windows)]
        let mut command = {
            use std::os::windows::process::CommandExt;
            let mut command = std::process::Command::new("explorer.exe");
            command.creation_flags(0x08000000);
            command
        };
        let mut child = command
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| "Ouverture du dossier impossible.")?;
        // A file manager can stay open; reap it separately without blocking the console.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })
    .await
    .map_err(|_| "Ouverture interrompue.")?
}

async fn confirm(
    app: AppHandle,
    title: &'static str,
    message: &'static str,
    action: &'static str,
) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .message(message)
            .title(title)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                action.into(),
                "Annuler".into(),
            ))
            .blocking_show()
    })
    .await
    .map_err(|_| "Confirmation interrompue.".into())
}

#[tauri::command]
pub(crate) async fn host_regenerate_password(
    version: u32,
    state: State<'_, Server>,
    app: AppHandle,
) -> Result<bool, String> {
    bridge(version)?;
    if !confirm(app, "Renouveler le mot de passe ?", "Le compte administrateur recevra un nouveau mot de passe aléatoire. Ses sessions et ses clés d’application seront invalidées. Vous devrez saisir le nouveau mot de passe dans Phémios. Le catalogue et le certificat restent inchangés. Le serveur doit être arrêté.", "Générer un nouveau mot de passe").await? {
        return Ok(false);
    }
    work(&state, |host| host.regenerate_password()).await?;
    Ok(true)
}

#[tauri::command]
pub(crate) async fn host_new_server(
    version: u32,
    state: State<'_, Server>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    bridge(version)?;
    if !confirm(app, "Créer un nouveau serveur vide ?", "La configuration actuelle sera archivée et ses données conservées dans leur dossier d’origine. Un profil vide sera sélectionné, sans dossier de musique ni compte existant. Au prochain démarrage, de nouveaux accès et un nouveau certificat seront créés ; il faudra réexporter la connexion vers Phémios. Vos fichiers audio ne seront pas modifiés. Le serveur doit être arrêté.", "Créer le serveur vide").await? {
        return Ok(None);
    }
    work(&state, |host| {
        host.new_server()
            .map(|path| Some(path.display().to_string()))
    })
    .await
}
