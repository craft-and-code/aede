//! Real local HTTP listeners for runtime shutdown and admission tests.

use crate::*;
use tokio::io::AsyncReadExt;

pub(super) async fn response_headers(client: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut headers = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !headers.windows(4).any(|part| part == b"\r\n\r\n") {
            let mut bytes = [0; 512];
            let read = client.read(&mut bytes).await.unwrap();
            assert_ne!(read, 0, "connection closed before response headers");
            headers.extend_from_slice(&bytes[..read]);
        }
    })
    .await
    .expect("the response headers must finish");
    headers
}

pub(super) async fn start_http(
    state: ApiState,
) -> (
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<(), std::io::Error>>,
) {
    let catalog_path = store::catalog_path(&state.data_dir);
    // Match normal startup: the fixture's snapshot was already loaded and
    // saved. An unknown stamp would make the watcher take the writer lock for
    // an unnecessary reload and race with the first synchronous scan.
    *state.loaded_stamp.write().await = runtime::stamp(&catalog_path);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    #[cfg(unix)]
    let (command_listener, command_socket) =
        delegation::bind_command_socket(&state.data_dir).unwrap();
    let (shutdown, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        runtime::run_http(
            listener,
            catalog_path,
            state,
            #[cfg(unix)]
            command_listener,
            #[cfg(unix)]
            command_socket,
            #[cfg(unix)]
            Arc::new(|_, _| true),
            async move {
                let _ = stopped.await;
            },
        )
        .await
    });
    (address, shutdown, server)
}
