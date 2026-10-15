//! Administrative children own a separate process tree; cancellation cannot signal the server or its player.
use std::io;
use std::process::{Child, Command, Stdio};

pub(super) fn isolate(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = command;
}

/// Only a still-running Child supplied by this runner is accepted; no client PID or shell text is used.
pub(super) fn terminate(child: &mut Child) -> io::Result<()> {
    #[cfg(unix)]
    let status = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{}", child.id())])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    #[cfg(windows)]
    let status = Command::new("taskkill.exe")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    #[cfg(any(unix, windows))]
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            "system refused to terminate the administrative process tree",
        ))
    }
    #[cfg(not(any(unix, windows)))]
    child.kill()
}
