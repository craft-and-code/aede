//! Bounded owner-private profile; unreadable state never becomes a fresh installation.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
const LIMIT: u64 = 128 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Configuration {
    version: u8,
    pub(crate) port: u16,
    pub(crate) roots: Vec<String>,
    pub(crate) identity: Option<Identity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pending_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) data_profile: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub(crate) certificate_pem: String,
    pub(crate) private_key_pem: String,
    pub(crate) certificate_der: Vec<u8>,
    pub(crate) password: String,
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            version: 1,
            port: 8787,
            roots: Vec::new(),
            identity: None,
            pending_password: None,
            data_profile: None,
        }
    }
}
impl Configuration {
    pub(crate) fn empty_profile(port: u16, id: String) -> Self {
        Self {
            port,
            data_profile: Some(id),
            ..Self::default()
        }
    }
    pub(crate) fn data_directory(&self, root: &Path) -> PathBuf {
        root.join(
            self.data_profile
                .as_ref()
                .map_or_else(|| "engine".into(), |id| format!("engine-{id}")),
        )
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        let valid_id =
            |value: &String| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit());
        if self
            .pending_password
            .as_ref()
            .is_some_and(|value| !valid_id(value))
            || self
                .data_profile
                .as_ref()
                .is_some_and(|value| !valid_id(value))
            || self.pending_password.is_some() && self.identity.is_none()
        {
            return Err("Profil de maintenance invalide ; conservé sans modification.".into());
        }
        if self.version != 1
            || self.port == 0
            || self.roots.len() > 64
            || self.roots.iter().any(|path| {
                !Path::new(path).is_absolute()
                    || path.len() > 4096
                    || path.chars().any(char::is_control)
            })
        {
            return Err(
                "Configuration invalide : port de 1 à 65535 et 64 dossiers absolus maximum.".into(),
            );
        }
        let unique: std::collections::BTreeSet<_> = self.roots.iter().collect();
        if unique.len() != self.roots.len() {
            return Err("Un dossier ne peut être ajouté deux fois.".into());
        }
        if self.identity.as_ref().is_some_and(|id| {
            id.password.len() != 64
                || !id.password.bytes().all(|b| b.is_ascii_hexdigit())
                || id.certificate_der.is_empty()
                || id.certificate_der.len() > 8192
                || id.certificate_pem.len() > 16384
                || id.private_key_pem.len() > 8192
        }) {
            return Err("Identité locale invalide ; aucune réinitialisation automatique.".into());
        }
        Ok(())
    }
}
impl Identity {
    pub(crate) fn generate() -> Result<Self, String> {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()])
                .map_err(|_| "Impossible de créer le certificat local.")?;
        Ok(Self {
            certificate_pem: cert.pem(),
            certificate_der: cert.der().to_vec(),
            private_key_pem: signing_key.serialize_pem(),
            password: random()?,
        })
    }
}
pub(crate) fn random() -> Result<String, String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| "Générateur sécurisé indisponible.")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn admit(path: &Path, directory: bool) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|_| "Profil serveur illisible.")?;
    if (directory && !meta.is_dir()) || (!directory && (!meta.is_file() || meta.len() > LIMIT)) {
        return Err("Profil serveur invalide ou lien symbolique refusé.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err("Le profil serveur doit rester privé (0700/0600).".into());
        }
    }
    Ok(())
}
pub(crate) fn read(root: &Path) -> Result<Configuration, String> {
    match fs::symlink_metadata(root) {
        Ok(_) => admit(root, true)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Configuration::default());
        }
        Err(_) => return Err("Profil serveur illisible.".into()),
    }
    let path = root.join("server.json");
    match fs::symlink_metadata(&path) {
        Ok(_) => admit(&path, false)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Configuration::default());
        }
        Err(_) => return Err("Profil serveur illisible.".into()),
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|_| "Profil serveur illisible.")?;
    if bytes.len() > LIMIT as usize {
        return Err("Profil serveur trop volumineux.".into());
    }
    let value: Configuration = serde_json::from_slice(&bytes)
        .map_err(|_| "Profil serveur corrompu ; conservé sans modification.")?;
    value.validate()?;
    Ok(value)
}
pub(crate) fn save(root: &Path, config: &Configuration) -> Result<(), String> {
    config.validate()?;
    prepare_root(root)?;
    let path = root.join("server.json");
    if path.exists() {
        admit(&path, false)?;
    }
    let bytes = serde_json::to_vec(config).map_err(|_| "Configuration invalide.")?;
    if bytes.len() > LIMIT as usize {
        return Err("Configuration trop volumineuse.".into());
    }
    let temporary: PathBuf = root.join(format!(".server-{}.tmp", random()?));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "Écriture du profil impossible.")?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        #[cfg(test)]
        test_support::before_initial_publication(root)?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result.map_err(|_| "Publication du profil impossible.".into())
}

pub(crate) fn prepare_root(root: &Path) -> Result<(), String> {
    if !root.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(root)
            .map_err(|_| "Création du profil impossible.")?;
    }
    admit(root, true)
}

pub(crate) struct ProfileLock(fs::File);
impl Drop for ProfileLock {
    fn drop(&mut self) {
        // Release explicitly: a descriptor inherited briefly by a spawning child must not extend ownership.
        let _ = self.0.unlock();
    }
}
pub(crate) fn lock(root: &Path) -> Result<ProfileLock, String> {
    // Validate an existing profile before creating any file within it.
    read(root)?;
    prepare_root(root)?;
    let path = root.join("desktop.lock");
    match fs::symlink_metadata(&path) {
        Ok(_) => admit(&path, false)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Verrou du profil illisible.".into()),
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(path)
        .map_err(|_| "Verrou du profil indisponible.")?;
    file.try_lock()
        .map_err(|_| "Une autre application Aède utilise déjà ce profil.".to_string())?;
    let lock = ProfileLock(file);
    // Only the exclusive owner may publish even the first default profile.
    // Re-read after acquiring ownership rather than saving an earlier snapshot.
    if !root.join("server.json").exists() {
        save(root, &read(root)?)?;
    }
    Ok(lock)
}

#[cfg(test)]
#[path = "configuration_test_support.rs"]
pub(crate) mod test_support;
