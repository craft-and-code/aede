//! Real HTTPS/UPnP loopback rendezvous for the desktop stdout acknowledgement contract.
use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex, atomic::AtomicBool, atomic::Ordering},
    thread::JoinHandle,
    time::Duration,
};

const NETWORK: &str = "/api/admin/v1/network-audio";
const AVT: &str = "urn:schemas-upnp-org:service:AVTransport:1";
const CM: &str = "urn:schemas-upnp-org:service:ConnectionManager:1";

pub(super) struct NetworkFixture {
    host: Host,
    fixture: Fixture,
    catalog: Vec<u8>,
    pub(super) original: Vec<u8>,
    reference: Value,
}

impl NetworkFixture {
    pub(super) fn new() -> Self {
        let fixture = Fixture::new();
        let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
        let folder = music(&fixture.0);
        let original = fs::read(folder.join("test.wav")).unwrap();
        host.configure(available_port(), vec![folder.display().to_string()])
            .unwrap();
        host.start().unwrap();
        completed_scan(&mut host);
        let tracks = host.request("GET", "/api/v1/tracks", None).unwrap();
        assert_eq!(tracks["items"].as_array().unwrap().len(), 1);
        Self {
            catalog: catalog(&fixture.0),
            reference: tracks["items"][0]["reference"].clone(),
            host,
            fixture,
            original,
        }
    }

    pub(super) fn play(&mut self, renderer: &Renderer) {
        let accepted = self
            .host
            .request(
                "POST",
                &format!("{NETWORK}/play"),
                Some(&json!({"bind":"127.0.0.1", "device":renderer.endpoint(),
                    "tracks":[self.reference]})),
            )
            .unwrap();
        assert_eq!(accepted["status"], "running");
        assert!(accepted["id"].as_u64().unwrap() > 0);
    }

    pub(super) fn network_status(&mut self) -> Value {
        self.host.request("GET", NETWORK, None).unwrap()
    }

    pub(super) fn wait_for_status(&mut self, expected: &str) {
        wait_until(
            "network controller did not reach its expected terminal state",
            || {
                let status = self.network_status();
                match status["status"].as_str().unwrap() {
                    "running" | "stopping" => false,
                    actual => {
                        assert_eq!(actual, expected, "unexpected network outcome: {status}");
                        true
                    }
                }
            },
        );
    }

    pub(super) fn wait_for_playing(&mut self, renderer: &Renderer) -> String {
        // Renew the real login's lease while waiting for wire-observed PLAYING
        // and its matching position query; elapsed time never triggers playback.
        wait_until("renderer never confirmed owned playback", || {
            assert_eq!(self.network_status()["status"], "running");
            renderer.snapshot().positions > 0
        });
        renderer.snapshot().uri
    }

    pub(super) fn stop_and_restart(&mut self, media_uri: Option<&str>) {
        let old_pid = server_pid(&self.fixture.0);
        // Collect every user-visible operation before asserting, so a broken
        // first Stop still records retry/status/start and releases the fixture.
        let stopped = self.host.stop().map(|state| state.running);
        let retry = self.host.stop().map(|state| state.running);
        let status = self.host.status().map(|state| state.running);
        let restarted = self.host.start().map(|state| state.running);
        let new_pid = (restarted == Ok(true)).then(|| server_pid(&self.fixture.0));
        let reset = new_pid.map(|_| self.network_status());
        let final_stop = self.host.stop().map(|state| state.running);
        assert_eq!(catalog(&self.fixture.0), self.catalog);
        assert_eq!(
            fs::read(self.fixture.0.join("music/test.wav")).unwrap(),
            self.original,
        );
        assert!(!exists(old_pid), "the old owned engine survived Stop");
        if let Some(uri) = media_uri {
            assert!(
                TcpStream::connect(media_address(uri).0).is_err(),
                "the old controller's media listener survived Stop",
            );
        }
        assert!(
            stopped == Ok(false),
            "AE-039: Stop={stopped:?}; retry={retry:?}; status={status:?}; start={restarted:?}",
        );
        assert_eq!(retry, Ok(false));
        assert_eq!(status, Ok(false));
        assert_eq!(restarted, Ok(true));
        assert_ne!(new_pid, Some(old_pid));
        assert_eq!(reset, Some(Value::Null));
        assert_eq!(final_stop, Ok(false));
    }
}

#[derive(Clone, Default)]
pub(super) struct RendererState {
    pub(super) descriptions: usize,
    pub(super) positions: usize,
    pub(super) stops: usize,
    pub(super) uri: String,
    playing: bool,
    complete: bool,
}

pub(super) struct Renderer {
    address: SocketAddr,
    state: Arc<Mutex<RendererState>>,
    closed: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Renderer {
    pub(super) fn new(reject_description: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(RendererState::default()));
        let closed = Arc::new(AtomicBool::new(false));
        let worker_state = state.clone();
        let worker_closed = closed.clone();
        let worker = std::thread::spawn(move || {
            while !worker_closed.load(Ordering::Acquire) {
                let (socket, _) = listener.accept().unwrap();
                if worker_closed.load(Ordering::Acquire) {
                    break;
                }
                let Some((socket, action, payload)) = request(socket) else {
                    continue;
                };
                if action == "GET" {
                    worker_state.lock().unwrap().descriptions += 1;
                    let description = format!(
                        "<?xml version=\"1.0\"?><root xmlns=\"urn:schemas-upnp-org:device-1-0\"><device><friendlyName>Lifecycle Fixture</friendlyName><serviceList><service><serviceType>{AVT}</serviceType><controlURL>/control</controlURL></service><service><serviceType>{CM}</serviceType><controlURL>/connection</controlURL></service></serviceList></device></root>"
                    );
                    reply(
                        socket,
                        if reject_description { 503 } else { 200 },
                        &description,
                    );
                    continue;
                }
                let mut state = worker_state.lock().unwrap();
                let (service, fields) = match action.as_str() {
                    "GetProtocolInfo" => (CM, "<Sink>http-get:*:audio/wav:*</Sink>".into()),
                    "GetTransportInfo" => (
                        AVT,
                        format!(
                            "<CurrentTransportState>{}</CurrentTransportState><CurrentTransportStatus>OK</CurrentTransportStatus>",
                            if state.playing && !state.complete {
                                "PLAYING"
                            } else {
                                "STOPPED"
                            },
                        ),
                    ),
                    "GetPositionInfo" => {
                        state.positions += 1;
                        (AVT, format!("<TrackURI>{}</TrackURI>", state.uri))
                    }
                    "SetAVTransportURI" => {
                        state.uri = payload
                            .split("<CurrentURI>")
                            .nth(1)
                            .unwrap()
                            .split("</CurrentURI>")
                            .next()
                            .unwrap()
                            .to_owned();
                        (AVT, String::new())
                    }
                    "Play" => {
                        state.playing = true;
                        (AVT, String::new())
                    }
                    "Stop" => {
                        state.stops += 1;
                        state.playing = false;
                        (AVT, String::new())
                    }
                    other => panic!("unexpected lifecycle renderer action: {other}"),
                };
                drop(state);
                let response = format!(
                    "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><u:{action}Response xmlns:u=\"{service}\">{fields}</u:{action}Response></s:Body></s:Envelope>"
                );
                reply(socket, 200, &response);
            }
        });
        Self {
            address,
            state,
            closed,
            worker: Some(worker),
        }
    }

    pub(super) fn endpoint(&self) -> String {
        format!("http://{}/description.xml", self.address)
    }

    pub(super) fn snapshot(&self) -> RendererState {
        self.state.lock().unwrap().clone()
    }

    pub(super) fn finish(&self) {
        self.state.lock().unwrap().complete = true;
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}

fn request(socket: TcpStream) -> Option<(TcpStream, String, String)> {
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(socket);
    let mut first = String::new();
    if reader.read_line(&mut first).unwrap() == 0 {
        return None;
    }
    let mut length = 0usize;
    let mut action = String::new();
    let mut headers = first.len();
    loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        headers += line.len();
        assert!(headers < 32768);
        if line == "\r\n" {
            break;
        }
        let (key, value) = line.split_once(':').unwrap();
        if key.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().unwrap();
        }
        if key.eq_ignore_ascii_case("soapaction") {
            action = value
                .trim()
                .trim_matches('"')
                .split('#')
                .nth(1)
                .unwrap()
                .into();
        }
    }
    assert!(length < 32768);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    if first.starts_with("GET ") {
        action = "GET".into();
    }
    Some((
        reader.into_inner(),
        action,
        String::from_utf8(body).unwrap(),
    ))
}

fn reply(mut socket: TcpStream, status: u16, body: &str) {
    let response = format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if let Err(error) = socket.write_all(response.as_bytes()) {
        // Shutdown may cancel an in-flight control request after ownership was
        // observed. Refuse every error except the resulting closed peer.
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
        ));
    }
}

fn media_address(uri: &str) -> (SocketAddr, &str) {
    let rest = uri.strip_prefix("http://").unwrap();
    let (authority, path) = rest.split_once('/').unwrap();
    let address: SocketAddr = authority.parse().unwrap();
    assert!(address.ip().is_loopback());
    (address, path)
}

pub(super) fn read_original(uri: &str) -> Vec<u8> {
    let (address, path) = media_address(uri);
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        socket,
        "GET /{path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = Vec::new();
    socket.take(1024 * 1024).read_to_end(&mut response).unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200"));
    let split = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap();
    response[split + 4..].to_vec()
}
