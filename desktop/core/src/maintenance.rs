//! Explicit stopped-server maintenance, with recoverable credentials and retained data profiles.
use crate::{Host, configuration};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

impl Host {
    /// Return only the fixed application-owned directory containing the private profile and stores.
    pub fn profile_directory(&self) -> Result<PathBuf, String> {
        configuration::read(&self.root)?;
        std::fs::canonicalize(&self.root).map_err(|_| "Dossier des données inaccessible.".into())
    }

    /// Generate new administrator credentials while stopped; an interrupted change can be retried.
    pub fn regenerate_password(&mut self) -> Result<(), String> {
        self.require_stopped()?;
        let mut config = configuration::read(&self.root)?;
        if config.identity.is_none() {
            return Err("Démarrez une première fois le serveur pour créer ses accès.".into());
        }
        if !std::fs::symlink_metadata(config.data_directory(&self.root).join("accounts.json"))
            .is_ok_and(|metadata| metadata.is_file())
        {
            return Err(
                "Démarrez une première fois le serveur pour initialiser son compte.".into(),
            );
        }
        if config.pending_password.is_none() {
            config.pending_password = Some(configuration::random()?);
            // Retain both candidates before touching accounts. Restart can finish this exact intent.
            configuration::save(&self.root, &config)?;
        }
        self.finish_password_change(&mut config)?;
        self.logs
            .push("info", "Mot de passe administrateur renouvelé.".into());
        Ok(())
    }

    pub(crate) fn finish_password_change(
        &self,
        config: &mut configuration::Configuration,
    ) -> Result<(), String> {
        let Some(password) = config.pending_password.as_ref() else {
            return Ok(());
        };
        let mut command = Command::new(&self.binary);
        command
            .arg(format!(
                "--data={}",
                config.data_directory(&self.root).display()
            ))
            .args(["accounts", "password", self.username(), "--password-stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for key in [
            "AEDE_DESKTOP_LEASE",
            "AEDE_ADMIN_TOKEN",
            "AEDE_DELEGATED_CHILD",
            "AEDE_DELEGATED_DATA_DIR",
        ] {
            command.env_remove(key);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|_| "Renouvellement en attente : moteur indisponible. Réessayez avant de vous connecter.")?;
        let written = child
            .stdin
            .take()
            .ok_or(())
            .and_then(|mut stdin| writeln!(stdin, "{password}").map_err(|_| ()));
        if written.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Renouvellement en attente : transmission impossible. Réessayez.".into());
        }
        let deadline = Instant::now() + Duration::from_secs(12);
        let success = loop {
            match child.try_wait() {
                Ok(Some(result)) => break result.success(),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break false;
                }
            }
        };
        if !success {
            return Err("Renouvellement en attente : le moteur n’a pas confirmé la modification. Réessayez ; les données sont conservées.".into());
        }
        config
            .identity
            .as_mut()
            .ok_or("Identité locale absente.")?
            .password = password.clone();
        config.pending_password = None;
        // A failed publication leaves the durable pending password available for an idempotent retry.
        configuration::save(&self.root, config)
    }

    /// Archive the connection configuration, then atomically select an empty independent store.
    /// Previous catalogue/account files remain in their original directory and are never deleted.
    pub fn new_server(&mut self) -> Result<PathBuf, String> {
        self.require_stopped()?;
        let previous = configuration::read(&self.root)?;
        let id = configuration::random()?;
        let archives = self.root.join("archives");
        configuration::prepare_root(&archives)?;
        let archive = archives.join(&id);
        // create_dir is exclusive: never replace an existing backup, even on an improbable collision.
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&archive)
            .map_err(|_| "Création de la sauvegarde impossible ; profil conservé.")?;
        configuration::save(&archive, &previous)?;
        let fresh = configuration::Configuration::empty_profile(previous.port, id);
        if fresh
            .data_directory(&self.root)
            .try_exists()
            .map_err(|_| "Nouveau dossier inaccessible.")?
        {
            return Err("Le nouveau dossier existe déjà ; profil conservé.".into());
        }
        // This single publication is the switch. Failure or a crash cannot partly move the old store.
        configuration::save(&self.root, &fresh)?;
        self.logs.push(
            "info",
            "Nouveau profil vide sélectionné ; anciennes données conservées.".into(),
        );
        Ok(archive)
    }

    fn require_stopped(&mut self) -> Result<(), String> {
        if self.status()?.running {
            return Err(
                "Arrêtez le serveur avant de modifier ses accès ou de créer un profil vide.".into(),
            );
        }
        Ok(())
    }
}
