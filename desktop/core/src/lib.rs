//! Framework-independent launcher for one bundled Aède server and an isolated private profile.
#![warn(missing_docs)]
mod configuration;
mod diagnostics;
mod http;
mod logs;
mod process;
use configuration::{read, save};
pub use diagnostics::Diagnostics;
pub use logs::Snapshot as LogSnapshot;
use serde::Serialize;
use serde_json::{Value, json};
use std::path::PathBuf;

// Keep desktop selections within the administrative scan endpoint's body budget,
// including the duplicated paths if every selected volume becomes unavailable.
const SCAN_BODY_LIMIT: usize = 16 * 1024;

fn scan_payload(roots: &[String]) -> Result<Value, String> {
    let mut payload = json!({
        "folders":roots,"replace":true,"threads":1,"unavailable_folders":roots
    });
    if payload.to_string().len() > SCAN_BODY_LIMIT {
        return Err("Sélection trop volumineuse pour scanner. Réduisez le nombre ou la longueur des dossiers.".into());
    }
    payload["unavailable_folders"] = json!(
        roots
            .iter()
            .filter(|path| !std::path::Path::new(path).is_dir())
            .collect::<Vec<_>>()
    );
    Ok(payload)
}

/// Public state, excluding passwords, session tokens and TLS private keys.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    running: bool,
    address: Option<String>,
    port: u16,
    roots: Vec<Root>,
    username: &'static str,
    data_directory: String,
}
#[derive(Serialize)]
struct Root {
    path: String,
    available: bool,
}
/// Owns a fixed executable and its lifetime lease; dropping it stops only its own child.
pub struct Host {
    root: PathBuf,
    binary: PathBuf,
    process: Option<process::Server>,
    logs: logs::Logs,
    _lock: configuration::ProfileLock,
}
impl Host {
    /// Acquire exclusive ownership of a private application profile, without starting its server.
    pub fn new(root: PathBuf, binary: PathBuf) -> Result<Self, String> {
        let lock = configuration::lock(&root)?;
        Ok(Self {
            root,
            binary,
            process: None,
            logs: logs::Logs::default(),
            _lock: lock,
        })
    }
    /// Read configuration and observe process exit without starting or resetting anything.
    pub fn status(&mut self) -> Result<Status, String> {
        if let Some(server) = self.process.as_mut()
            && !server.running()?
        {
            self.process = None;
            self.logs
                .push("warn", "Le processus serveur s’est arrêté.".into());
        }
        let configuration = read(&self.root)?;
        Ok(Status {
            running: self.process.is_some(),
            address: self.process.as_ref().map(|server| server.address.clone()),
            port: configuration.port,
            roots: configuration
                .roots
                .into_iter()
                .map(|path| Root {
                    available: std::path::Path::new(&path).is_dir(),
                    path,
                })
                .collect(),
            username: self.username(),
            data_directory: self.root.join("engine").display().to_string(),
        })
    }
    /// Persist selected roots/port atomically. No scan occurs until explicitly requested.
    /// Roots can change live; changing the listening port requires stopping the server.
    pub fn configure(&mut self, port: u16, roots: Vec<String>) -> Result<Status, String> {
        let current = self.status()?;
        if current.running && port != current.port {
            return Err("Arrêtez le serveur avant de modifier son port.".into());
        }
        let mut configuration = read(&self.root)?;
        // Oversized legacy profiles need to support the UI's one-folder-at-a-time recovery.
        let legacy_reduction = roots.len() < configuration.roots.len()
            && roots.iter().all(|root| configuration.roots.contains(root))
            && scan_payload(&configuration.roots).is_err();
        configuration.port = port;
        configuration.roots = roots;
        configuration.validate()?;
        if !legacy_reduction {
            scan_payload(&configuration.roots)?;
        }
        save(&self.root, &configuration)?;
        self.status()
    }
    /// Start the exact bundled binary over HTTPS loopback, retaining its identity and credentials.
    pub fn start(&mut self) -> Result<Status, String> {
        if self.status()?.running {
            return self.status();
        }
        let mut configuration = read(&self.root)?;
        if configuration.identity.is_none() {
            configuration.identity = Some(configuration::Identity::generate()?);
            save(&self.root, &configuration)?;
        }
        self.logs
            .push("info", "Démarrage du serveur HTTPS local.".into());
        self.process = Some(
            process::Server::start(&self.root, &self.binary, &configuration, self.logs.clone())
                .inspect_err(|error| self.logs.push("error", error.clone()))?,
        );
        self.logs.push(
            "info",
            "Serveur HTTPS prêt ; compte administrateur authentifié.".into(),
        );
        self.status()
    }
    /// Release the owned child, waiting briefly for graceful shutdown before terminating it.
    pub fn stop(&mut self) -> Result<Status, String> {
        if let Some(mut server) = self.process.take() {
            server.stop()?;
            self.logs.push("info", "Serveur arrêté.".into());
        }
        self.status()
    }
    /// Read bounded, process-local diagnostic summaries without private request content.
    pub fn logs(&self, after: u64) -> Result<LogSnapshot, String> {
        self.logs.snapshot(after)
    }
    /// Export the retained sanitized log tail, independently of a view's incremental cursor.
    pub fn export_logs(&self) -> Result<String, String> {
        self.logs.export_json()
    }
    /// Return the fixed installation account name without starting the server or exposing secrets.
    pub fn username(&self) -> &'static str {
        "aede-desktop"
    }
    /// Clear only in-memory diagnostics; catalog and account data are untouched.
    pub fn clear_logs(&self) -> Result<(), String> {
        self.logs.clear()
    }
    /// Return the address only while the owned process is alive.
    pub fn address(&mut self) -> Result<String, String> {
        self.status()?
            .address
            .ok_or_else(|| "Démarrez le serveur avant de copier son adresse.".into())
    }
    /// Reveal only the generated installation administrator password, on explicit user request.
    pub fn password(&self) -> Result<String, String> {
        read(&self.root)?
            .identity
            .map(|value| value.password)
            .ok_or_else(|| "Démarrez le serveur pour créer ses accès.".into())
    }
    /// Export a public local-connection profile, containing no password or private key.
    pub fn connection_profile(&self) -> Result<String, String> {
        let configuration = read(&self.root)?;
        let server = self
            .process
            .as_ref()
            .ok_or("Démarrez le serveur avant l’export.")?;
        let identity = configuration.identity.ok_or("Identité TLS indisponible.")?;
        Ok(
            json!({"format":"aede-local-server","version":1,"address":server.address,
            "username":"aede-desktop","certificate":identity.certificate_der})
            .to_string(),
        )
    }
    /// Start an asynchronous catalog scan; unavailable roots are retained without traversal.
    pub fn scan(&mut self) -> Result<Value, String> {
        let configuration = read(&self.root)?;
        if configuration.roots.is_empty() {
            return Err("Ajoutez au moins un dossier avant de scanner.".into());
        }
        let payload = scan_payload(&configuration.roots)?;
        self.request("POST", "/api/admin/v1/scan", Some(&payload))
    }
    /// Follow one admitted administrative task using its numeric identifier only.
    pub fn task(&mut self, id: u64, cancel: bool) -> Result<Value, String> {
        if id == 0 {
            return Err("Identifiant de tâche invalide.".into());
        }
        self.request(
            if cancel { "POST" } else { "GET" },
            &format!(
                "/api/admin/v1/tasks/{id}{}",
                if cancel { "/cancel" } else { "" }
            ),
            None,
        )
    }
    fn request(&mut self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        if !self.status()?.running {
            return Err("Le serveur est arrêté.".into());
        }
        let config = read(&self.root)?;
        let identity = config.identity.ok_or("Identité TLS indisponible.")?;
        let server = self.process.as_mut().ok_or("Le serveur est arrêté.")?;
        // Reauthentication is bounded by Aède. A token never leaves native memory.
        server.request(&identity, method, path, body)
    }
}
#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
