//! The one place this program reaches the network.
//!
//! Behind the `fetch` feature, so that a build without it has no way to make a
//! request at all — a property the compiler enforces rather than a promise a
//! README makes.
//!
//! It does three things and refuses to do more: it waits its turn, it names
//! itself, and it hands back parsed JSON. Everything about *what* to ask and
//! *what to do with the answer* lives in [`crate::musicbrainz`], which has no
//! socket and is therefore testable.
//!
//! **Waiting its turn is not politeness, it is the contract.** MusicBrainz
//! allows one request per second per address and answers `503` to everything
//! once that is exceeded — not just to the request that went over. A client
//! without a throttle does not run slightly too fast; it stops working, and it
//! does so for every other program on the same address.

use crate::json::Json;
use std::time::{Duration, Instant};

/// How long to wait for one answer before giving up on it.
const TIMEOUT: Duration = Duration::from_secs(20);

/// What can go wrong, kept apart because the answers differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The service asked us to slow down. Not a failure of the request: a
    /// statement about the rate, and the only sane response is to stop.
    RateLimited,
    /// The service answered, with something other than success.
    Status(u16),
    /// Nothing came back: no route, no name, a timeout.
    Network(String),
    /// Something came back, and it was not the JSON that was asked for.
    NotJson(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::RateLimited => write!(
                f,
                "the service is refusing requests because too many were sent \
                 (one per second is the limit); nothing was lost, try later"
            ),
            Error::Status(code) => write!(f, "the service answered {code}"),
            Error::Network(detail) => write!(f, "could not reach the service: {detail}"),
            Error::NotJson(detail) => write!(f, "the answer was not readable: {detail}"),
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
    pub fn get_json(&mut self, url: &str) -> Result<Json, Error> {
        self.wait_turn();
        let text = self
            .agent
            .get(url)
            .header("User-Agent", &self.user_agent)
            .header("Accept", "application/json")
            .call()
            .map_err(from_ureq)?
            .body_mut()
            .read_to_string()
            .map_err(|e| Error::Network(e.to_string()))?;
        crate::json::parse(&text).map_err(|e| Error::NotJson(e.to_string()))
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

    /// How long the next [`Client::get_json`] would wait before sending.
    ///
    /// Exposed so a command can tell the user that asking about six hundred
    /// artists will take ten minutes *before* it starts, rather than leaving
    /// them to work it out from a progress line.
    pub fn interval(&self) -> Duration {
        self.interval
    }
}

/// Translates the client library's errors into this module's four cases.
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
mod tests {
    use super::*;

    #[test]
    fn the_user_agent_carries_a_way_to_reach_us() {
        // The format MusicBrainz documents. A generic one is throttled as part
        // of a shared pool, so getting this wrong is not a cosmetic mistake.
        let agent = Client::identify("aede", "0.1.0", "https://example.org/aede");
        assert_eq!(agent, "aede/0.1.0 ( https://example.org/aede )");
    }

    #[test]
    fn the_second_request_waits_for_its_turn() {
        // No network involved: the throttle is a clock, and a clock can be
        // tested. This is the one part of the client that has to be right on
        // the first run, because getting it wrong means a 503 for everything.
        let mut client = Client::new("test", Duration::from_millis(120));
        let start = Instant::now();
        client.wait_turn();
        assert!(
            start.elapsed() < Duration::from_millis(50),
            "the first request waits for nobody"
        );
        client.wait_turn();
        assert!(
            start.elapsed() >= Duration::from_millis(120),
            "the second waited: {:?}",
            start.elapsed()
        );
    }
}
