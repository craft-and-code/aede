//! The one place this program reaches the network.
//!
//! Behind the `fetch` feature, so that a build without it has no way to make a
//! request at all — a property the compiler enforces rather than a promise a
//! README makes.
//!
//! It throttles requests, identifies the application, and returns bounded JSON
//! or image bytes. Endpoint construction and response interpretation live in
//! service modules such as [`crate::musicbrainz`], which have no socket and can
//! be tested without contacting a service.
//!
//! **Waiting its turn is not politeness, it is the contract.** MusicBrainz
//! allows one request per second per address and answers `503` to everything
//! once that is exceeded — not just to the request that went over. A client
//! without a throttle does not run slightly too fast; it stops working, and it
//! does so for every other program on the same address.

use crate::json::Json;
use std::time::{Duration, Instant};

/// How long to wait for one answer before giving up on it.
const TIMEOUT: Duration = Duration::from_secs(25);

/// The largest body this client will read into memory.
///
/// Cover art may be much larger than the JSON metadata. A ceiling turns a wrong
/// address — a redirect gone astray, a service handing back something else
/// entirely — into a refusal rather than a machine filling its memory.
const MAX_BODY: u64 = 32 * 1024 * 1024;

/// What can go wrong, kept apart because the answers differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// HTTP 503: the service is busy or limiting requests. Callers may apply
    /// bounded backoff and must stop if it persists. HTTP 429 stays a separate
    /// [`Error::Status`] so it cannot enter that retry path blindly.
    RateLimited,
    /// The service answered, with something other than success.
    Status(u16),
    /// Nothing came back: no route, no name, a timeout.
    Network(String),
    /// Something came back, and it was not the JSON that was asked for.
    NotJson(String),
    /// The response exceeded the explicit in-memory body limit, in bytes.
    BodyTooLarge(u64),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::RateLimited => write!(
                f,
                "the service is busy or limiting requests; nothing was lost, try later"
            ),
            Error::Status(code) => write!(f, "the service answered {code}"),
            Error::Network(detail) => write!(f, "could not reach the service: {detail}"),
            Error::NotJson(detail) => write!(f, "the answer was not readable: {detail}"),
            Error::BodyTooLarge(limit) => write!(
                f,
                "the answer exceeded the {} MiB download limit, so it was not kept",
                limit / (1024 * 1024)
            ),
        }
    }
}

impl std::error::Error for Error {}

/// A client that waits its turn and says who it is.
pub struct Client {
    agent: ureq::Agent,
    user_agent: String,
    interval: Duration,
    last: Option<Instant>,
}

impl Client {
    /// A client for one service.
    ///
    /// `user_agent` is not decoration: MusicBrainz **requires** a descriptive
    /// one carrying a way to contact whoever wrote the program, and throttles
    /// anonymous or generic callers as a single shared pool — so a library
    /// that sent `ureq/3.4` would be sharing a rate limit with every other
    /// program that could not be bothered either. Build it with
    /// [`Client::identify`].
    pub fn new(user_agent: impl Into<String>, interval: Duration) -> Client {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .build();
        Client {
            agent: config.into(),
            user_agent: user_agent.into(),
            interval,
            last: None,
        }
    }

    /// The `User-Agent` MusicBrainz asks for: `name/version ( contact )`.
    pub fn identify(name: &str, version: &str, contact: &str) -> String {
        format!("{name}/{version} ( {contact} )")
    }

    /// Fetches a URL and parses the answer as JSON.
    ///
    /// Blocks for as long as the rate requires before sending. That is the
    /// whole point: a caller that could forget to wait is a caller that will.
    /// Bodies over 32 MiB and JSON that is not UTF-8 are refused.
    pub fn get_json(&mut self, url: &str) -> Result<Json, Error> {
        self.wait_turn();
        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", &self.user_agent)
            .header("Accept", "application/json")
            .call()
            .map_err(from_ureq)?;
        read_json_body(response.body_mut())
    }

    /// Fetches a URL and hands back the body as it came.
    ///
    /// The one thing this client downloads that is not an answer to a question:
    /// an image. It waits its turn like everything else.
    ///
    /// At most 32 MiB are accepted. The response reader has an explicit limit
    /// rather than relying on a dependency default; one extra byte detects
    /// oversized bodies even when the server omits their length.
    pub fn get_bytes(&mut self, url: &str) -> Result<Vec<u8>, Error> {
        self.wait_turn();
        let mut response = self
            .agent
            .get(url)
            .header("User-Agent", &self.user_agent)
            .call()
            .map_err(from_ureq)?;
        read_bytes_body(response.body_mut())
    }

    /// Sleeps until the next request is allowed.
    fn wait_turn(&mut self) {
        if let Some(last) = self.last {
            let since = last.elapsed();
            if since < self.interval {
                std::thread::sleep(self.interval - since);
            }
        }
        self.last = Some(Instant::now());
    }

    /// The minimum delay between the starts of two requests on this client.
    ///
    /// Exposed so a command can tell the user that asking about six hundred
    /// artists will take ten minutes *before* it starts, rather than leaving
    /// them to work it out from a progress line.
    pub fn interval(&self) -> Duration {
        self.interval
    }
}

fn read_json_body(body: &mut ureq::Body) -> Result<Json, Error> {
    // Reading bytes avoids a lossy text conversion changing an external claim.
    let text =
        String::from_utf8(read_bytes_body(body)?).map_err(|e| Error::NotJson(e.to_string()))?;
    crate::json::parse(&text).map_err(|e| Error::NotJson(e.to_string()))
}

fn read_bytes_body(body: &mut ureq::Body) -> Result<Vec<u8>, Error> {
    let reader = body.with_config().limit(MAX_BODY + 1).reader();
    read_limited(reader, MAX_BODY)
}

fn read_limited(reader: impl std::io::Read, limit: u64) -> Result<Vec<u8>, Error> {
    // ureq's body limit applies before decompression. Bound the decoded bytes
    // too, before allocating them, so a small compressed answer cannot expand
    // past the same memory budget.
    use std::io::Read;
    let mut reader = reader.take(limit + 1);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|e| match ureq::Error::from(e) {
            ureq::Error::BodyExceedsLimit(_) => Error::BodyTooLarge(limit),
            other => Error::Network(other.to_string()),
        })?;
    if bytes.len() as u64 > limit {
        return Err(Error::BodyTooLarge(limit));
    }
    Ok(bytes)
}

/// Preserves HTTP status meaning while hiding the dependency's error types.
fn from_ureq(error: ureq::Error) -> Error {
    match error {
        // 503 is what MusicBrainz answers when the rate is exceeded, and it
        // answers it to everything until the rate drops — so it has to be told
        // apart from an ordinary failure, which a retry could survive.
        ureq::Error::StatusCode(503) => Error::RateLimited,
        ureq::Error::StatusCode(code) => Error::Status(code),
        other => Error::Network(other.to_string()),
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
