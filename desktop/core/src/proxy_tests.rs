use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

struct ProxyProbe {
    address: String,
    requests: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl ProxyProbe {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_requests = requests.clone();
        let worker_stop = stop.clone();
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        worker_requests.fetch_add(1, Ordering::Relaxed);
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut bytes = [0; 4096];
                        let _ = stream.read(&mut bytes);
                        let _ = stream.write_all(
                            b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("loopback proxy listener failed: {error}"),
                }
            }
        });
        Self {
            address,
            requests,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for ProxyProbe {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.0.try_wait().is_ok_and(|status| status.is_some()) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn exercise_owned_https() {
    let root = PathBuf::from(std::env::var_os("AEDE_DESKTOP_PROXY_FIXTURE").unwrap());
    let mut host = Host::new(root.clone(), engine()).unwrap();
    host.configure(available_port(), vec![]).unwrap();
    assert!(host.start().unwrap().running);
    assert!(host.request("GET", "/api/v1/status", None).is_ok());
    let address = host.status().unwrap().address.unwrap();
    let config = read(&root).unwrap();
    let correct = crate::http::agent(config.identity.as_ref().unwrap());
    assert_eq!(
        correct
            .get(format!("{address}/"))
            .call()
            .unwrap()
            .status()
            .as_u16(),
        200
    );
    let wrong_identity = configuration::Identity::generate().unwrap();
    assert!(
        crate::http::agent(&wrong_identity)
            .get(format!("{address}/"))
            .call()
            .is_err()
    );
    assert!(!host.stop().unwrap().running);
}

#[test]
fn owned_https_ignores_environment_proxies_and_keeps_exact_certificate_verification() {
    // The same test performs real work in the ordinary suite and in its isolated
    // subprocess; no process-global environment is mutated by parallel tests.
    if std::env::var_os("AEDE_DESKTOP_PROXY_FIXTURE").is_some() {
        exercise_owned_https();
        return;
    }
    let fixture = Fixture::new();
    let proxy = ProxyProbe::new();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        "tests::engine::proxy::owned_https_ignores_environment_proxies_and_keeps_exact_certificate_verification",
        "--nocapture",
    ]);
    for variable in [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "NO_PROXY",
        "no_proxy",
    ] {
        command.env_remove(variable);
    }
    command
        .env("HTTPS_PROXY", &proxy.address)
        .env("ALL_PROXY", &proxy.address)
        .env("AEDE_DESKTOP_PROXY_FIXTURE", &fixture.0)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = ChildGuard(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(45);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "owned HTTPS subprocess timed out"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(
        proxy.requests.load(Ordering::Relaxed),
        0,
        "owned loopback HTTPS reached an environment proxy"
    );
    assert!(
        status.success(),
        "isolated owned HTTPS or certificate control failed"
    );
}
