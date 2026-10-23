//! Per-profile filesystem rendezvous for the initial-publication regression.
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) struct PublicationRendezvous(PathBuf);

impl PublicationRendezvous {
    pub(crate) fn new(root: &Path) -> Self {
        let path = rendezvous_path(root);
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub(crate) fn wait_for_creator(&self) {
        wait_for(&self.0.join("claimed")).unwrap();
    }

    pub(crate) fn release(&self) {
        fs::write(self.0.join("released"), []).unwrap();
    }
}

impl Drop for PublicationRendezvous {
    fn drop(&mut self) {
        // An assertion failure must not strand the suspended creator.
        let _ = fs::write(self.0.join("released"), []);
    }
}

fn rendezvous_path(root: &Path) -> PathBuf {
    root.with_extension("initial-publication-rendezvous")
}

fn wait_for(path: &Path) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "initial-publication rendezvous timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

pub(crate) fn before_initial_publication(root: &Path) -> io::Result<()> {
    let rendezvous = rendezvous_path(root);
    if root.join("server.json").exists() || !rendezvous.is_dir() {
        return Ok(());
    }
    match fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(rendezvous.join("claimed"))
    {
        Ok(_) => wait_for(&rendezvous.join("released")),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}
