use super::*;

#[test]
fn private_command_path_stays_short_for_a_deep_data_directory() {
    let root = std::env::temp_dir().join(format!(
        "aede_delegation_path_{}_{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let deep = root.join("a_really_long_folder_name_repeated_for_a_unix_socket_path_test");
    std::fs::create_dir_all(&deep).unwrap();
    let path = command_socket_path(&deep).unwrap();
    assert!(path.as_os_str().as_encoded_bytes().len() < 100);
    assert_eq!(path, command_socket_path(&deep).unwrap());
    std::fs::remove_dir(deep).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn oversized_command_request_is_rejected_before_allocation() {
    let header = ((MAX_REQUEST + 1) as u32).to_be_bytes();
    let mut frame = header.as_slice();
    let error = read_request(&mut frame).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn command_client_rejects_a_socket_other_users_could_access() {
    let data = std::env::temp_dir().join(format!(
        "aede_delegation_permissions_{}_{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    std::fs::create_dir(&data).unwrap();
    let socket = command_socket_path(&data).unwrap();
    let parent = socket.parent().unwrap();
    std::fs::create_dir(parent).unwrap();
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!verify_command_socket(&data, &socket).unwrap());
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(verify_command_socket(&data, &socket).unwrap());
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert_eq!(
        verify_command_socket(&data, &socket).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    drop(listener);
    std::fs::remove_file(&socket).unwrap();
    std::fs::remove_dir(parent).unwrap();
    std::fs::remove_dir(data).unwrap();
}

#[test]
fn cancelling_only_marks_a_registered_task() {
    let tasks = TaskRegistry::default();
    let requested = Arc::new(AtomicBool::new(false));
    assert!(!tasks.request_cancel(17).unwrap());
    tasks.insert(17, requested.clone()).unwrap();
    assert!(tasks.request_cancel(17).unwrap());
    assert!(requested.load(Ordering::Acquire));
    tasks.remove(17).unwrap();
    assert!(!tasks.request_cancel(17).unwrap());
}

#[test]
fn a_running_child_stops_after_cancellation() {
    let requested = Arc::new(AtomicBool::new(false));
    let signal = requested.clone();
    let mut child = Command::new("sleep").arg("10").spawn().unwrap();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        signal.store(true, Ordering::Release);
    });
    let started = std::time::Instant::now();
    let (status, cancelled) = wait_for_command(&mut child, Some(&requested)).unwrap();
    assert!(cancelled);
    assert!(!status.success());
    assert!(started.elapsed() < Duration::from_secs(2));
}

fn command_test_state() -> ApiState {
    let (events, _) = tokio::sync::broadcast::channel(8);
    let (shutdown, _) = tokio::sync::broadcast::channel(1);
    ApiState {
        data_dir: std::env::temp_dir(),
        catalog: Arc::new(tokio::sync::RwLock::new(None)),
        loaded_stamp: Arc::new(tokio::sync::RwLock::new(None)),
        reload_gate: Arc::new(tokio::sync::Mutex::new(())),
        events,
        shutdown,
        admin: None,
        next_task_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        tasks: Arc::new(TaskRegistry::default()),
        websocket_slots: Arc::new(tokio::sync::Semaphore::new(64)),
        inspection_slots: Arc::new(tokio::sync::Semaphore::new(2)),
        jobs: Arc::new(crate::jobs::JobRegistry::default()),
    }
}

fn command_test_listener() -> (tokio::net::UnixListener, PathBuf) {
    static NEXT_SOCKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let path = PathBuf::from("/tmp").join(format!(
        "aede_ipc_test_{}_{}",
        std::process::id(),
        NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
    ));
    (tokio::net::UnixListener::bind(&path).unwrap(), path)
}

#[test]
fn an_idle_command_connection_is_closed_during_server_shutdown() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let state = command_test_state();
        let shutdown = state.shutdown.clone();
        let (listener, path) = command_test_listener();
        let commands = tokio::spawn(accept_commands(listener, state, Arc::new(|_, _| false)));
        let mut client = UnixStream::connect(&path).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        shutdown.send(()).unwrap();
        let stopped = tokio::time::timeout(Duration::from_secs(2), commands).await;
        let read = client.read(&mut [0]);
        // Close even on failure so an old blocking handler cannot hang the test runtime.
        drop(client);
        std::fs::remove_file(path).unwrap();
        assert!(stopped.is_ok(), "command listener did not stop");
        assert_eq!(read.unwrap(), 0, "the idle connection must be closed");
    });
}

#[test]
fn shutdown_sent_before_the_command_listener_first_runs_is_observed() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let state = command_test_state();
        let shutdown = state.shutdown.clone();
        let (listener, path) = command_test_listener();
        let accepting = accept_commands(listener, state, Arc::new(|_, _| false));
        let signalled = shutdown.send(());
        let stopped = tokio::time::timeout(Duration::from_millis(200), accepting).await;
        std::fs::remove_file(path).unwrap();
        assert!(
            signalled.is_ok(),
            "shutdown must be observed before the first poll"
        );
        assert!(
            stopped.is_ok(),
            "the already-requested shutdown must stop acceptance"
        );
    });
}

#[test]
fn an_incomplete_command_frame_expires_without_a_shutdown() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let state = command_test_state();
        let shutdown = state.shutdown.clone();
        let (listener, path) = command_test_listener();
        let commands = tokio::spawn(accept_commands(listener, state, Arc::new(|_, _| false)));
        let mut client = UnixStream::connect(&path).unwrap();
        client.write_all(&100_u32.to_be_bytes()).unwrap();
        client.write_all(b"{").unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(7)))
            .unwrap();
        let read = client.read(&mut [0]);
        drop(client);
        shutdown.send(()).unwrap();
        commands.await.unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            read.unwrap(),
            0,
            "incomplete frames need an absolute deadline"
        );
    });
}

#[test]
fn a_stalled_output_client_does_not_stop_draining_command_output() {
    let (server, client) = UnixStream::pair().unwrap();
    server
        .set_write_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let (completed, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        relay_output(
            std::io::Cursor::new(vec![b'x'; 4 * 1024 * 1024]),
            b'O',
            Arc::new(Mutex::new(server)),
        );
        completed.send(()).unwrap();
    });
    let drained = result.recv_timeout(Duration::from_secs(1));
    drop(client);
    worker.join().unwrap();
    assert!(
        drained.is_ok(),
        "a blocked client must not stall completed work"
    );
}

#[test]
fn excess_command_connections_are_refused_without_stopping_the_listener() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let state = command_test_state();
        let shutdown = state.shutdown.clone();
        let (listener, path) = command_test_listener();
        let commands = tokio::spawn(accept_commands(listener, state, Arc::new(|_, _| false)));
        let mut clients = Vec::new();
        for _ in 0..MAX_CONNECTIONS {
            clients.push(UnixStream::connect(&path).unwrap());
            tokio::task::yield_now().await;
        }
        let mut excess = UnixStream::connect(&path).unwrap();
        excess
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let refused = excess.read(&mut [0]);
        drop(excess);
        drop(clients);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut next = UnixStream::connect(&path).unwrap();
        next.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        send_request(&mut next, &test_request("cancel", Some(123))).unwrap();
        assert_eq!(
            read_exit(&mut next),
            3,
            "the listener must recover capacity"
        );
        shutdown.send(()).unwrap();
        commands.await.unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(refused.unwrap(), 0, "excess clients must be disconnected");
    });
}

#[test]
fn full_command_capacity_still_accepts_cancellation() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let state = command_test_state();
        let cancelled = Arc::new(AtomicBool::new(false));
        state.tasks.insert(42, cancelled.clone()).unwrap();
        let full = Arc::new(tokio::sync::Semaphore::new(0));
        for (request, expected) in [
            (test_request("scan", None), 2),
            (test_request("cancel", Some(42)), 0),
        ] {
            let (server, mut client) = UnixStream::pair().unwrap();
            server.set_nonblocking(true).unwrap();
            let stream = tokio::net::UnixStream::from_std(server).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let handler_state = state.clone();
            let shutdown = state.shutdown.subscribe();
            let slots = full.clone();
            let handler = tokio::task::spawn_blocking(move || {
                handle_command(
                    stream,
                    handler_state,
                    Arc::new(|_, _| true),
                    shutdown,
                    slots,
                )
                .map_err(|error| error.to_string())
            });
            send_request(&mut client, &request).unwrap();
            assert_eq!(read_exit(&mut client), expected);
            handler.await.unwrap().unwrap();
        }
        assert!(cancelled.load(Ordering::Acquire));
    });
}

fn test_request(command: &str, cancel_task_id: Option<u64>) -> CommandRequest {
    CommandRequest {
        args: if cancel_task_id.is_some() {
            Vec::new()
        } else {
            vec![
                "--exact".into(),
                "delegation::tests::delegated_child_fixture".into(),
                "--nocapture".into(),
            ]
        },
        cwd: std::env::current_dir().unwrap(),
        command: command.into(),
        stdin_terminal: false,
        stdout_terminal: false,
        acoustid_key: None,
        fanarttv_key: None,
        no_color: None,
        cancel_task_id,
    }
}

fn read_exit(client: &mut UnixStream) -> i32 {
    loop {
        let mut header = [0; 5];
        client.read_exact(&mut header).unwrap();
        let size = u32::from_be_bytes(header[1..].try_into().unwrap()) as usize;
        assert!(size <= MAX_REQUEST);
        let mut bytes = vec![0; size];
        client.read_exact(&mut bytes).unwrap();
        if header[0] == b'X' {
            return i32::from_be_bytes(bytes.try_into().unwrap());
        }
    }
}

fn read_task(client: &mut UnixStream) -> u64 {
    let mut frame = [0; 13];
    client.read_exact(&mut frame).unwrap();
    assert_eq!(frame[0], b'T');
    assert_eq!(u32::from_be_bytes(frame[1..5].try_into().unwrap()), 8);
    u64::from_be_bytes(frame[5..].try_into().unwrap())
}

async fn wait_for_saved_work(data: &Path) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !data.join("saved").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child must start");
}

// Invoked in a separate copy of the test executable by the real delegation
// handler. The environment belongs only to that child, never the test process.
#[test]
fn delegated_child_fixture() {
    if std::env::var_os("AEDE_DELEGATED_CHILD").is_none() {
        return;
    }
    let data = PathBuf::from(std::env::var_os("AEDE_DELEGATED_DATA_DIR").unwrap());
    std::fs::write(data.join("saved"), b"already saved").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !data.join("release").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        data.join("release").exists(),
        "parent must release the test child"
    );
    std::fs::write(data.join("completed"), b"completed").unwrap();
}

#[test]
fn shutdown_finishes_accepted_work_after_its_client_disconnects() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (listener, path) = command_test_listener();
        let data = path.with_extension("data");
        std::fs::create_dir(&data).unwrap();
        let mut state = command_test_state();
        state.data_dir = data.clone();
        let shutdown = state.shutdown.clone();
        let mut commands = tokio::spawn(accept_commands(listener, state, Arc::new(|_, _| true)));
        let mut client = UnixStream::connect(&path).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        send_request(&mut client, &test_request("scan", None)).unwrap();
        let _task = read_task(&mut client);
        wait_for_saved_work(&data).await;
        shutdown.send(()).unwrap();
        let waited = tokio::time::timeout(Duration::from_millis(100), &mut commands).await;
        drop(client);
        std::fs::write(data.join("release"), b"go").unwrap();
        tokio::time::timeout(Duration::from_secs(3), commands)
            .await
            .unwrap()
            .unwrap();
        let completed = data.join("completed").exists();
        std::fs::remove_dir_all(data).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(waited.is_err(), "shutdown must wait for accepted work");
        assert!(
            completed,
            "client disconnection must not cancel the operation"
        );
    });
}

#[test]
fn cancellation_preserves_work_saved_before_the_request() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (listener, path) = command_test_listener();
        let data = path.with_extension("data");
        std::fs::create_dir(&data).unwrap();
        let mut state = command_test_state();
        state.data_dir = data.clone();
        let shutdown = state.shutdown.clone();
        let commands = tokio::spawn(accept_commands(listener, state, Arc::new(|_, _| true)));
        let mut client = UnixStream::connect(&path).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        send_request(&mut client, &test_request("fetch", None)).unwrap();
        let task_id = read_task(&mut client);
        wait_for_saved_work(&data).await;
        let mut cancellation = UnixStream::connect(&path).unwrap();
        cancellation
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        send_request(&mut cancellation, &test_request("cancel", Some(task_id))).unwrap();
        assert_eq!(read_exit(&mut cancellation), 0);
        assert_eq!(read_exit(&mut client), 130);
        assert_eq!(
            client.read(&mut [0]).unwrap(),
            0,
            "completion must release the input relay even while the CLI keeps stdin open"
        );
        assert_eq!(std::fs::read(data.join("saved")).unwrap(), b"already saved");
        assert!(!data.join("completed").exists());
        shutdown.send(()).unwrap();
        commands.await.unwrap();
        std::fs::remove_dir_all(data).unwrap();
        std::fs::remove_file(path).unwrap();
    });
}
