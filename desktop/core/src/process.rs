//! Fixed sidecar execution. Secrets travel through stdin, never arguments or environment.
use crate::configuration::Configuration;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant},
};
pub(crate) struct Server {
    child: Child,
    lease: Option<ChildStdin>,
    control: bool,
    stopping: bool,
    stopped: std::sync::mpsc::Receiver<bool>,
    confirmed: bool,
    pub(crate) address: String,
    pub(crate) token: Option<String>,
    pub(crate) agent: ureq::Agent,
}
impl Server {
    pub(crate) fn start(
        root: &Path,
        binary: &Path,
        config: &Configuration,
        logs: crate::logs::Logs,
    ) -> Result<Self, String> {
        let identity = config.identity.as_ref().ok_or("Identité locale absente.")?;
        let mut command = Command::new(binary);
        command
            .arg(format!("--data={}", config.data_directory(root).display()))
            .args(["serve", &format!("--port={}", config.port)])
            .env("AEDE_DESKTOP_LEASE", "1")
            .env("NO_COLOR", "1")
            .env("AEDE_REQUEST_LOG", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for key in [
            "AEDE_ADMIN_TOKEN",
            "AEDE_FANARTTV_KEY",
            "AEDE_ACOUSTID_KEY",
            "AEDE_DELEGATED_CHILD",
            "AEDE_DELEGATED_DATA_DIR",
            "AEDE_UNAVAILABLE_ROOTS",
        ] {
            command.env_remove(key);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|_| "Le moteur Aède embarqué est introuvable ou ne peut démarrer.")?;
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || logs.drain(BufReader::new(stderr)));
        }
        let bootstrap = json!({"username":"aede-desktop","password":identity.password,
            "tls":{"certificate":identity.certificate_pem,"private_key":identity.private_key_pem}});
        let mut stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Entrée du moteur indisponible.".into());
            }
        };
        if writeln!(stdin, "{bootstrap}").is_err() {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            return Err("Initialisation du moteur impossible.".into());
        }
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                drop(stdin);
                let _ = child.kill();
                let _ = child.wait();
                return Err("Sortie du moteur indisponible.".into());
            }
        };
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let (stopped_tx, stopped) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let result = Read::take(&mut reader, 4097)
                .read_line(&mut line)
                .map(|_| line);
            let line_start = result.as_ref().is_ok_and(|line| line.ends_with('\n'));
            let _ = tx.send(result);
            let acknowledgement = matches!(read_stop_ack(reader, line_start), Ok(true));
            let _ = stopped_tx.send(acknowledgement);
        });
        let ready = rx
            .recv_timeout(Duration::from_secs(12))
            .ok()
            .and_then(Result::ok)
            .and_then(|line| serde_json::from_str::<Value>(&line).ok());
        let expected = format!("https://127.0.0.1:{}", config.port);
        if !ready
            .as_ref()
            .is_some_and(|v| v["desktopProtocol"] == 2 && v["address"] == expected)
        {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            return Err("Aède n’a pas confirmé son démarrage. Vérifiez le port (déjà utilisé ?), le profil et la version du moteur.".into());
        }
        let mut server = Self {
            child,
            lease: Some(stdin),
            control: ready.as_ref().is_some_and(|v| v["desktopControl"] == 1),
            stopping: false,
            stopped,
            confirmed: false,
            address: expected,
            token: None,
            agent: crate::http::agent(identity),
        };
        // Do not advertise a running server until its retained administrator really authenticates.
        if let Err(error) = server.authenticate(identity) {
            let _ = server.stop();
            return Err(error);
        }
        Ok(server)
    }
    pub(crate) fn running(&mut self) -> Result<bool, String> {
        self.child
            .try_wait()
            .map(|v| v.is_none())
            .map_err(|_| "État du serveur indisponible.".into())
    }
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        if self.confirmed {
            return Ok(());
        }
        if !self.stopping {
            if self.control {
                self.send_control(b"{\"desktopCommand\":\"shutdown\"}\n")?;
            } else {
                self.lease.take();
            }
            self.stopping = true;
        }
        let until = Instant::now() + Duration::from_secs(3);
        while self.running()? && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        if self.running()? {
            if !self.control {
                return Err("Le moteur ne confirme pas l’arrêt de ses tâches. Arrêt gracieux en cours ; propriété conservée.".into());
            }
            self.send_control(b"{\"desktopCommand\":\"force-stop\"}\n")?;
            let until = Instant::now() + Duration::from_secs(5);
            while self.running()? {
                if Instant::now() >= until {
                    return Err("Arrêt des tâches non confirmé ; le moteur reste possédé. Réessayez l’arrêt.".into());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        self.child
            .wait()
            .map_err(|_| "Confirmation de l’arrêt impossible.")?;
        if self.control && self.stopped.recv_timeout(Duration::from_secs(1)) != Ok(true) {
            return Err(
                "Le moteur n’a pas confirmé l’arrêt de toutes ses tâches ; propriété conservée."
                    .into(),
            );
        }
        self.confirmed = true;
        self.lease.take();
        self.token = None;
        Ok(())
    }

    fn send_control(&mut self, line: &[u8]) -> Result<(), String> {
        if !self.running()? {
            return Ok(());
        }
        self.lease
            .as_mut()
            .ok_or("Contrôle du moteur indisponible ; propriété conservée.")?
            .write_all(line)
            .map_err(|_| "Commande d’arrêt impossible ; propriété conservée.".into())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn read_stop_ack(mut reader: impl BufRead, mut line_start: bool) -> std::io::Result<bool> {
    let mut acknowledged = false;
    let mut line = Vec::with_capacity(4097);
    loop {
        line.clear();
        if reader.by_ref().take(4097).read_until(b'\n', &mut line)? == 0 {
            return Ok(acknowledged);
        }
        // Diagnostics share stdout. Never treat an oversized line's later fragment as a frame.
        acknowledged |= line_start && line == b"{\"desktopStopped\":1}\n";
        line_start = line.last() == Some(&b'\n');
    }
}

#[cfg(test)]
#[path = "process_output_tests.rs"]
mod output_tests;
#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
