//! Private, same-machine command channel for the CLI.
//!
//! The HTTP API stays read-only by default. A local Unix socket delegates
//! writes to the process that owns the server lifecycle. The CLI subprocess
//! retains its ordinary parser, output and store lock, so interrupted fetches
//! keep their existing save-after-each-answer behaviour.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Read, Write};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{ApiState, CancelOutcome, CatalogEvent, CommandValidator, refresh_catalog, store};
use serde::{Deserialize, Serialize};

const SOCKET: &str = "command.sock";
const SERVER_LOCK: &str = ".aede-server.lock";
const MAX_REQUEST: usize = 1024 * 1024;
const MAX_CONNECTIONS: usize = 32;
const MAX_COMMANDS: usize = 16;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const OUTPUT_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_POLL: Duration = Duration::from_millis(50);

#[derive(Serialize, Deserialize)]
struct CommandRequest {
    args: Vec<String>,
    cwd: PathBuf,
    command: String,
    stdin_terminal: bool,
    stdout_terminal: bool,
    acoustid_key: Option<String>,
    fanarttv_key: Option<String>,
    no_color: Option<String>,
    #[serde(default)]
    cancel_task_id: Option<u64>,
}

#[derive(Default)]
pub struct TaskRegistry {
    running: Mutex<BTreeMap<u64, Arc<AtomicBool>>>,
}

impl TaskRegistry {
    fn insert(&self, task_id: u64, requested: Arc<AtomicBool>) -> io::Result<()> {
        self.running
            .lock()
            .map_err(|_| io::Error::other("task registry poisoned"))?
            .insert(task_id, requested);
        Ok(())
    }

    fn request_cancel(&self, task_id: u64) -> io::Result<bool> {
        let running = self
            .running
            .lock()
            .map_err(|_| io::Error::other("task registry poisoned"))?;
        if let Some(requested) = running.get(&task_id) {
            requested.store(true, Ordering::Release);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn remove(&self, task_id: u64) -> io::Result<()> {
        self.running
            .lock()
            .map_err(|_| io::Error::other("task registry poisoned"))?
            .remove(&task_id);
        Ok(())
    }
}

pub struct ServerLock {
    _file: std::fs::File,
}

impl ServerLock {
    pub fn acquire(data_dir: &Path) -> io::Result<Self> {
        let metadata = std::fs::metadata(data_dir)?;
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Aède data directory must not be writable by other users",
            ));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(data_dir.join(SERVER_LOCK))?;
        file.try_lock()?;
        Ok(Self { _file: file })
    }
}

pub fn bind_command_socket(data_dir: &Path) -> io::Result<(tokio::net::UnixListener, PathBuf)> {
    let path = command_socket_path(data_dir)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("invalid socket path"))?;
    if !parent.exists() {
        std::fs::DirBuilder::new().mode(0o700).create(parent)?;
    }
    let directory = std::fs::symlink_metadata(parent)?;
    let data = std::fs::metadata(data_dir)?;
    if !directory.file_type().is_dir()
        || directory.uid() != data.uid()
        || directory.permissions().mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Aède command directory is not private to the data owner",
        ));
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if !metadata.file_type().is_socket() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Aède command socket path is occupied by a non-socket file",
            ));
        }
        // The server lock excludes a second live Aède server. A socket left by
        // a crashed process is therefore safe to remove at this exact path.
        std::fs::remove_file(&path)?;
    }
    let listener = tokio::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok((listener, path))
}

fn command_socket_path(data_dir: &Path) -> io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(data_dir)?;
    let owner = std::fs::metadata(&canonical)?.uid();
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in canonical.as_os_str().as_encoded_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    Ok(PathBuf::from(format!("/tmp/aede-ipc-{owner}-{hash:016x}")).join(SOCKET))
}

fn verify_command_socket(data_dir: &Path, socket: &Path) -> io::Result<bool> {
    let owner = std::fs::metadata(data_dir)?.uid();
    let parent = socket
        .parent()
        .ok_or_else(|| io::Error::other("invalid socket path"))?;
    let directory = match std::fs::symlink_metadata(parent) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !directory.file_type().is_dir()
        || directory.uid() != owner
        || directory.permissions().mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Aède command directory is not private to the data owner",
        ));
    }
    let endpoint = match std::fs::symlink_metadata(socket) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !endpoint.file_type().is_socket()
        || endpoint.uid() != owner
        || endpoint.permissions().mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Aède command socket is not private to the data owner",
        ));
    }
    Ok(true)
}

pub fn accept_commands(
    listener: tokio::net::UnixListener,
    state: ApiState,
    validator: Arc<CommandValidator>,
) -> impl std::future::Future<Output = ()> + Send {
    // Subscribe before the future is spawned: an immediate stop must not be
    // lost just because the command listener has not received its first poll.
    let shutdown = state.shutdown.subscribe();
    accept_commands_until(listener, state, validator, shutdown)
}

async fn accept_commands_until(
    listener: tokio::net::UnixListener,
    state: ApiState,
    validator: Arc<CommandValidator>,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    let command_slots = Arc::new(tokio::sync::Semaphore::new(MAX_COMMANDS));
    let mut handlers = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = shutdown.recv() => break,
            Some(result) = handlers.join_next(), if !handlers.is_empty() => {
                report_handler(result);
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        if handlers.len() >= MAX_CONNECTIONS {
                            // Refuse before allocating a blocking worker. A closed
                            // connection is an error to the CLI, never a local retry.
                            drop(stream);
                            continue;
                        }
                        let state = state.clone();
                        let validator = validator.clone();
                        let command_slots = command_slots.clone();
                        let shutdown = state.shutdown.subscribe();
                        handlers.spawn_blocking(move || {
                            handle_command(stream, state, validator, shutdown, command_slots)
                                .map_err(|error| error.to_string())
                        });
                    }
                    Err(error) => eprintln!("Aède command socket accept failed: {error}"),
                }
            }
        }
    }
    drop(listener);
    // Keep the runtime alive while accepted work finishes. Incomplete requests
    // observe the shutdown signal; a client's disappearance never kills its work.
    while let Some(result) = handlers.join_next().await {
        report_handler(result);
    }
}

fn report_handler(result: Result<Result<(), String>, tokio::task::JoinError>) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("Aède delegated command failed: {error}"),
        Err(error) => eprintln!("Aède command handler failed: {error}"),
    }
}

fn handle_command(
    stream: tokio::net::UnixStream,
    state: ApiState,
    validator: Arc<CommandValidator>,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
    command_slots: Arc<tokio::sync::Semaphore>,
) -> Result<(), Box<dyn Error>> {
    let stream = stream.into_std()?;
    stream.set_nonblocking(false)?;
    let mut reader = stream.try_clone()?;
    let request_bytes = read_request(&mut RequestReader {
        stream: &mut reader,
        shutdown: &mut shutdown,
        deadline: Instant::now() + REQUEST_TIMEOUT,
    })?;
    reader.set_read_timeout(Some(SHUTDOWN_POLL))?;
    stream.set_write_timeout(Some(OUTPUT_TIMEOUT))?;
    let request: CommandRequest = serde_json::from_slice(&request_bytes)?;
    let output = Arc::new(Mutex::new(stream));
    if request.command == "cancel" {
        if let Some(task_id) = request.cancel_task_id
            && request.args.is_empty()
            && state.tasks.request_cancel(task_id)?
        {
            write_frame(&output, b'X', &0_i32.to_be_bytes())?;
        } else {
            write_frame(&output, b'X', &3_i32.to_be_bytes())?;
        }
        return Ok(());
    }
    if request.cancel_task_id.is_some() {
        send_error(&output, "unexpected cancellation ID in command request")?;
        return Ok(());
    }
    if !validator(&request.args, &request.command) || !request.cwd.is_dir() {
        send_error(&output, "invalid delegated command or working directory")?;
        return Ok(());
    }
    let Ok(_command_slot) = command_slots.try_acquire_owned() else {
        send_error(
            &output,
            "server command capacity reached; retry after a command finishes",
        )?;
        return Ok(());
    };
    if shutdown.try_recv() != Err(tokio::sync::broadcast::error::TryRecvError::Empty) {
        send_error(&output, "server is shutting down")?;
        return Ok(());
    }
    let data_dir = std::fs::canonicalize(&state.data_dir)?;
    let kind = match request.command.as_str() {
        "fetch" => "identification",
        "scan" => "scan",
        _ => "command",
    };
    let exe = std::env::current_exe()?;
    let mut process = Command::new(exe);
    process
        .args(&request.args)
        .current_dir(&request.cwd)
        .env("AEDE_DELEGATED_CHILD", "1")
        .env("AEDE_DELEGATED_DATA_DIR", &data_dir)
        .env_remove("AEDE_ADMIN_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if request.stdin_terminal {
        process.env("AEDE_DELEGATED_STDIN_TTY", "1");
    } else {
        process.env_remove("AEDE_DELEGATED_STDIN_TTY");
    }
    if request.stdout_terminal {
        process.env("AEDE_DELEGATED_STDOUT_TTY", "1");
    } else {
        process.env_remove("AEDE_DELEGATED_STDOUT_TTY");
    }
    for (name, value) in [
        ("AEDE_ACOUSTID_KEY", request.acoustid_key),
        ("AEDE_FANARTTV_KEY", request.fanarttv_key),
        ("NO_COLOR", request.no_color),
    ] {
        if let Some(value) = value {
            process.env(name, value);
        } else {
            process.env_remove(name);
        }
    }
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            send_error(&output, &format!("cannot start delegated command: {error}"))?;
            return Ok(());
        }
    };
    let task_id = state
        .next_task_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let cancellation =
        matches!(kind, "scan" | "identification").then(|| Arc::new(AtomicBool::new(false)));
    if let Some(requested) = &cancellation {
        state.tasks.insert(task_id, requested.clone())?;
    }
    let _ = state.events.send(CatalogEvent::TaskStarted {
        task_id,
        task_kind: kind,
    });
    if cancellation.is_some() {
        let _ = write_frame(&output, b'T', &task_id.to_be_bytes());
    }
    let child_stdin = child.stdin.take().ok_or("missing command stdin")?;
    let input_finished = Arc::new(AtomicBool::new(false));
    let input_signal = input_finished.clone();
    let stdin_thread = std::thread::spawn(move || {
        relay_input(reader, child_stdin, input_signal);
    });
    let stdout = child.stdout.take().ok_or("missing command stdout")?;
    let stderr = child.stderr.take().ok_or("missing command stderr")?;
    let out = output.clone();
    let stdout_thread = std::thread::spawn(move || relay_output(stdout, b'O', out));
    let err = output.clone();
    let stderr_thread = std::thread::spawn(move || relay_output(stderr, b'E', err));
    let (status, cancelled) = wait_for_command(&mut child, cancellation.as_deref())?;
    if cancellation.is_some() {
        state.tasks.remove(task_id)?;
    }
    // A CLI may keep stdin open after its child has finished. Polling the
    // input side lets this worker stop without relying on `shutdown(Read)` on
    // a cloned Unix socket: Linux does not reliably wake the other clone, so
    // the terminal status frame could otherwise remain blocked forever.
    input_finished.store(true, Ordering::Release);
    let _ = stdin_thread.join();
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();

    let path = store::catalog_path(&data_dir);
    let mut known = *tokio::runtime::Handle::current().block_on(state.loaded_stamp.read());
    let mut reported_failure = None;
    tokio::runtime::Handle::current().block_on(refresh_catalog(
        &path,
        &state,
        &mut known,
        &mut reported_failure,
    ));
    let scanned_at = tokio::runtime::Handle::current()
        .block_on(state.catalog.read())
        .as_ref()
        .map(|catalog| catalog.scanned_at);
    if cancelled {
        let _ = state.events.send(CatalogEvent::TaskFailed {
            task_id,
            task_kind: kind,
            code: "task_cancelled",
            message: "task cancelled by user".into(),
        });
    } else if status.success() {
        let _ = state.events.send(CatalogEvent::TaskCompleted {
            task_id,
            task_kind: kind,
            scanned_at,
        });
    } else {
        let _ = state.events.send(CatalogEvent::TaskFailed {
            task_id,
            task_kind: kind,
            code: "command_failed",
            message: format!("command exited with {status}"),
        });
    }
    let code = if cancelled {
        130
    } else {
        status.code().unwrap_or(1)
    };
    write_frame(&output, b'X', &code.to_be_bytes())?;
    Ok(())
}

fn wait_for_command(
    child: &mut Child,
    cancellation: Option<&AtomicBool>,
) -> io::Result<(ExitStatus, bool)> {
    let mut cancelled = false;
    loop {
        if cancellation.is_some_and(|requested| requested.load(Ordering::Acquire))
            && child.kill().is_ok()
        {
            cancelled = true;
        }
        if let Some(status) = child.try_wait()? {
            return Ok((status, cancelled));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn relay_output(mut input: impl Read, kind: u8, output: Arc<Mutex<UnixStream>>) {
    let mut buf = [0_u8; 8192];
    loop {
        match input.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(size) => {
                let _ = write_frame(&output, kind, &buf[..size]);
            }
        }
    }
}

fn relay_input(mut input: UnixStream, mut child_stdin: impl Write, finished: Arc<AtomicBool>) {
    let mut bytes = [0_u8; 8192];
    while !finished.load(Ordering::Acquire) {
        match input.read(&mut bytes) {
            Ok(0) | Err(_) if finished.load(Ordering::Acquire) => break,
            Ok(0) => break,
            Ok(size) if child_stdin.write_all(&bytes[..size]).is_err() => break,
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
}

fn send_error(output: &Arc<Mutex<UnixStream>>, message: &str) -> io::Result<()> {
    write_frame(output, b'E', message.as_bytes())?;
    write_frame(output, b'X', &2_i32.to_be_bytes())
}

fn write_frame(output: &Arc<Mutex<UnixStream>>, kind: u8, bytes: &[u8]) -> io::Result<()> {
    let mut stream = output
        .lock()
        .map_err(|_| io::Error::other("socket poisoned"))?;
    let timeout = stream.write_timeout()?.unwrap_or(OUTPUT_TIMEOUT);
    let deadline = Instant::now() + timeout;
    let mut header = [kind, 0, 0, 0, 0];
    header[1..].copy_from_slice(&(bytes.len() as u32).to_be_bytes());
    let result = write_until(&mut stream, &header, deadline)
        .and_then(|()| write_until(&mut stream, bytes, deadline));
    if result.is_err() {
        // A partial frame cannot be recovered. Disconnect once, then continue
        // draining both child pipes without waiting repeatedly on a stalled CLI.
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
    stream.set_write_timeout(Some(timeout))?;
    result
}

fn write_until(stream: &mut UnixStream, mut bytes: &[u8], deadline: Instant) -> io::Result<()> {
    while !bytes.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "command output timed out"))?;
        stream.set_write_timeout(Some(remaining))?;
        match stream.write(bytes) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "command socket closed",
                ));
            }
            Ok(size) => bytes = &bytes[size..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

struct RequestReader<'a> {
    stream: &'a mut UnixStream,
    shutdown: &'a mut tokio::sync::broadcast::Receiver<()>,
    deadline: Instant,
}

impl Read for RequestReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.shutdown.try_recv() != Err(tokio::sync::broadcast::error::TryRecvError::Empty) {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "server is shutting down",
                ));
            }
            let remaining = self
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::TimedOut, "command request timed out")
                })?;
            self.stream
                .set_read_timeout(Some(remaining.min(SHUTDOWN_POLL)))?;
            match self.stream.read(bytes) {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                result => return result,
            }
        }
    }
}

fn read_request(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut size = [0_u8; 4];
    reader.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size > MAX_REQUEST {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "command too large",
        ));
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn connect_command_socket(data_dir: &Path) -> Result<Option<UnixStream>, Box<dyn Error>> {
    let socket = match command_socket_path(data_dir) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !verify_command_socket(data_dir, &socket)? {
        return Ok(None);
    }
    let stream = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    Ok(Some(stream))
}

fn send_request(stream: &mut UnixStream, request: &CommandRequest) -> Result<(), Box<dyn Error>> {
    let request = serde_json::to_vec(request)?;
    if request.len() > MAX_REQUEST {
        return Err("delegated command is too large".into());
    }
    stream.write_all(&(request.len() as u32).to_be_bytes())?;
    stream.write_all(&request)?;
    Ok(())
}

pub fn delegate_command(
    data_dir: &Path,
    args: Vec<String>,
    command: &str,
) -> Result<Option<i32>, Box<dyn Error>> {
    let Some(mut stream) = connect_command_socket(data_dir)? else {
        return Ok(None);
    };
    send_request(
        &mut stream,
        &CommandRequest {
            args,
            cwd: std::env::current_dir()?,
            command: command.to_string(),
            stdin_terminal: io::stdin().is_terminal(),
            stdout_terminal: io::stdout().is_terminal(),
            acoustid_key: std::env::var("AEDE_ACOUSTID_KEY").ok(),
            fanarttv_key: std::env::var("AEDE_FANARTTV_KEY").ok(),
            no_color: std::env::var("NO_COLOR").ok(),
            cancel_task_id: None,
        },
    )?;
    let mut input_stream = stream.try_clone()?;
    std::thread::spawn(move || {
        let mut stdin = io::stdin();
        let _ = io::copy(&mut stdin, &mut input_stream);
        let _ = input_stream.shutdown(std::net::Shutdown::Write);
    });
    loop {
        let mut header = [0_u8; 5];
        stream.read_exact(&mut header)?;
        let size = u32::from_be_bytes(header[1..5].try_into()?) as usize;
        if size > MAX_REQUEST {
            return Err("server sent an oversized frame".into());
        }
        let mut bytes = vec![0_u8; size];
        stream.read_exact(&mut bytes)?;
        match header[0] {
            b'O' => io::stdout().write_all(&bytes)?,
            b'E' => io::stderr().write_all(&bytes)?,
            b'T' if size == 8 => {
                let mut id = [0_u8; 8];
                id.copy_from_slice(&bytes);
                let id = u64::from_be_bytes(id);
                eprintln!("Server task {id} started. Cancel it with: aede cancel {id}");
            }
            b'X' if size == 4 => {
                let mut code = [0_u8; 4];
                code.copy_from_slice(&bytes);
                return Ok(Some(i32::from_be_bytes(code)));
            }
            _ => return Err("server sent an invalid command frame".into()),
        }
    }
}

pub fn cancel_task(data_dir: &Path, task_id: u64) -> Result<CancelOutcome, Box<dyn Error>> {
    let Some(mut stream) = connect_command_socket(data_dir)? else {
        return Ok(CancelOutcome::NoServer);
    };
    send_request(
        &mut stream,
        &CommandRequest {
            args: Vec::new(),
            cwd: std::env::current_dir()?,
            command: "cancel".into(),
            stdin_terminal: false,
            stdout_terminal: false,
            acoustid_key: None,
            fanarttv_key: None,
            no_color: None,
            cancel_task_id: Some(task_id),
        },
    )?;
    let mut detail = String::new();
    loop {
        let mut header = [0_u8; 5];
        stream.read_exact(&mut header)?;
        let size = u32::from_be_bytes(header[1..5].try_into()?) as usize;
        if size > MAX_REQUEST {
            return Err("server sent an oversized frame".into());
        }
        let mut bytes = vec![0_u8; size];
        stream.read_exact(&mut bytes)?;
        match header[0] {
            b'E' => detail.push_str(&String::from_utf8_lossy(&bytes)),
            b'X' if size == 4 => {
                let mut code = [0_u8; 4];
                code.copy_from_slice(&bytes);
                return match i32::from_be_bytes(code) {
                    0 => Ok(CancelOutcome::Requested),
                    3 => Ok(CancelOutcome::NotFound),
                    _ => Err(if detail.is_empty() {
                        "server refused cancellation".into()
                    } else {
                        detail.into()
                    }),
                };
            }
            _ => return Err("server sent an invalid cancellation frame".into()),
        }
    }
}

#[cfg(test)]
#[path = "delegation_tests.rs"]
mod tests;
