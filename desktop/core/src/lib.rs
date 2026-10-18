//! Framework-independent launcher for one bundled Aède server and an isolated private profile.
#![warn(missing_docs)]
mod configuration;
mod http;
mod process;
use configuration::{read, save};
use serde::Serialize;
use serde_json::{Value, json};
use std::path::PathBuf;

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
            _lock: lock,
        })
    }
    /// Read configuration and observe process exit without starting or resetting anything.
    pub fn status(&mut self) -> Result<Status, String> {
        if let Some(server) = self.process.as_mut()
            && !server.running()?
        {
            self.process = None;
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
            username: "aede-desktop",
            data_directory: self.root.join("engine").display().to_string(),
        })
    }
    /// Persist selected roots/port atomically. No scan occurs until explicitly requested.
    /// A stopped server is required to avoid a port changing beneath an active connection.
    pub fn configure(&mut self, port: u16, roots: Vec<String>) -> Result<Status, String> {
        let current = self.status()?;
        if current.running {
            return Err("Arrêtez le serveur avant de modifier sa configuration.".into());
        }
        let mut configuration = read(&self.root)?;
        configuration.port = port;
        configuration.roots = roots;
        configuration.validate()?;
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
        self.process = Some(process::Server::start(
            &self.root,
            &self.binary,
            &configuration,
        )?);
        self.status()
    }
    /// Release the owned child, waiting briefly for graceful shutdown before terminating it.
    pub fn stop(&mut self) -> Result<Status, String> {
        if let Some(mut server) = self.process.take() {
            server.stop()?;
        }
        self.status()
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
        self.request("POST", "/api/admin/v1/scan", Some(&json!({
            "folders":configuration.roots,"replace":true,"threads":1,
            "unavailable_folders":configuration.roots.iter().filter(|p| !std::path::Path::new(p).is_dir()).collect::<Vec<_>>()
        })))
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
