//! Tiny instrumented HTTP fixture server for provider-adapter tests.
//!
//! [`FixtureHttpServer`] binds `127.0.0.1` on an ephemeral port and replays a
//! script of [`ScriptedResponse`] values, one per connection, while recording
//! every [`RecordedRequest`] it receives. Standard library only: no TCP/HTTP
//! framework dependencies, so provider crates can use it without new deps.
//!
//! # Secrets
//!
//! Fixture scripts must never contain real credentials. Use obviously fake
//! placeholder tokens (e.g. `test-token-123`) and assert captured traffic is
//! clean with [`RecordedRequest::assert_no_secrets`]. Use [`redact_secrets`]
//! before embedding recorded traffic in failure messages or snapshots.
//!
//! # Example
//!
//! ```no_run
//! use jackin_test_support::fixture_http::{FixtureHttpServer, ScriptedResponse};
//! use std::time::Duration;
//!
//! let server = FixtureHttpServer::start(vec![
//!     ScriptedResponse::new(429, "slow down").with_retry_after_secs(2),
//!     ScriptedResponse::new(200, r#"{"ok":true}"#)
//!         .with_header("Content-Type", "application/json"),
//! ])?;
//! // ... drive the provider adapter against server.url() ...
//! assert_eq!(server.request_count(), 2);
//! assert_eq!(server.requests()[0].path, "/v1/models");
//! # Ok::<(), std::io::Error>(())
//! ```

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// One scripted HTTP response, replayed for a single connection.
#[derive(Debug, Clone)]
pub struct ScriptedResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    latency: Duration,
}

impl ScriptedResponse {
    /// Response with `status` and `body`. `Content-Length` and
    /// `Connection: close` are added automatically at send time.
    #[must_use]
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
            latency: Duration::ZERO,
        }
    }

    /// Add a response header (e.g. `Content-Type`, `Retry-After`).
    #[must_use]
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Add a `Retry-After` header with a delay-seconds value.
    #[must_use]
    pub fn with_retry_after_secs(self, secs: u64) -> Self {
        self.with_header("Retry-After", &secs.to_string())
    }

    /// Sleep this long before sending the response (latency injection).
    #[must_use]
    pub fn with_latency(mut self, latency: Duration) -> Self {
        self.latency = latency;
        self
    }

    fn reason_phrase(status: u16) -> &'static str {
        match status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            _ => "Unknown",
        }
    }
}

/// One HTTP request captured by [`FixtureHttpServer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    /// Request method (`GET`, `POST`, ...), uppercased as received.
    pub method: String,
    /// Request target exactly as received (`/v1/models?x=1`).
    pub path: String,
    /// Request headers in received order.
    pub headers: Vec<(String, String)>,
    /// Raw request body bytes.
    pub body: Vec<u8>,
}

impl RecordedRequest {
    /// First header value for `name` (ASCII case-insensitive), if present.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Request body lossily decoded as UTF-8.
    #[must_use]
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Panic if any `forbidden` value appears in the path, headers, or body.
    ///
    /// The panic message is redacted via [`redact_secrets`]: it names which
    /// index matched, never the secret itself.
    ///
    /// # Panics
    ///
    /// Panics when a non-empty `forbidden` value is found in the request.
    pub fn assert_no_secrets(&self, forbidden: &[&str]) {
        let mut haystacks = Vec::with_capacity(self.headers.len() * 2 + 2);
        haystacks.push(self.path.as_str());
        for (key, value) in &self.headers {
            haystacks.push(key.as_str());
            haystacks.push(value.as_str());
        }
        let body = self.body_text();
        haystacks.push(body.as_str());
        for (index, secret) in forbidden.iter().enumerate() {
            if secret.is_empty() {
                continue;
            }
            for haystack in &haystacks {
                assert!(
                    !haystack.contains(secret),
                    "recorded request leaked forbidden value #{index} (redacted)"
                );
            }
        }
    }
}

/// Replace every occurrence of each non-empty `secret` in `text` with
/// `<redacted>`. Use before logging recorded traffic.
#[must_use]
pub fn redact_secrets(text: &str, secrets: &[&str]) -> String {
    let mut out = text.to_owned();
    for secret in secrets {
        if secret.is_empty() {
            continue;
        }
        out = out.replace(secret, "<redacted>");
    }
    out
}

/// Instrumented single-threaded HTTP/1.0-style fixture server.
///
/// Connections are served sequentially in accept order, so each connection
/// consumes the next [`ScriptedResponse`] deterministically. When the script
/// is exhausted the last response repeats; an empty script yields
/// `500` with an empty body until [`FixtureHttpServer::push_response`]
/// extends it.
#[derive(Debug)]
pub struct FixtureHttpServer {
    addr: SocketAddr,
    script: Arc<Mutex<VecDeque<ScriptedResponse>>>,
    recorded: Arc<Mutex<Vec<RecordedRequest>>>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FixtureHttpServer {
    /// Bind `127.0.0.1:0` and start serving `script` in a background thread.
    ///
    /// # Errors
    ///
    /// Returns the socket error when binding or spawning the server thread fails.
    pub fn start(script: Vec<ScriptedResponse>) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let script = Arc::new(Mutex::new(VecDeque::from(script)));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker = Worker {
            listener,
            script: Arc::clone(&script),
            recorded: Arc::clone(&recorded),
            shutdown: Arc::clone(&shutdown),
        };
        let thread =
            jackin_telemetry::spawn::thread_joined_named("fixture-http".to_owned(), move || {
                worker.serve();
            })?;
        Ok(Self {
            addr,
            script,
            recorded,
            shutdown,
            thread: Some(thread),
        })
    }

    /// Local address the server is bound to.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Base URL for the server, e.g. `http://127.0.0.1:41233`.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Number of requests recorded so far.
    #[must_use]
    pub fn request_count(&self) -> usize {
        self.recorded
            .lock()
            .map(|guard| guard.len())
            .unwrap_or_default()
    }

    /// Snapshot of all recorded requests in arrival order.
    #[must_use]
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.recorded
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Append a response to the live script (for multi-phase tests).
    pub fn push_response(&self, response: ScriptedResponse) {
        if let Ok(mut script) = self.script.lock() {
            script.push_back(response);
        }
    }

    /// Number of scripted responses still queued (excludes the sticky last).
    #[must_use]
    pub fn script_remaining(&self) -> usize {
        self.script
            .lock()
            .map(|guard| guard.len())
            .unwrap_or_default()
    }
}

impl Drop for FixtureHttpServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            drop(thread.join());
        }
    }
}

#[derive(Debug)]
struct Worker {
    listener: TcpListener,
    script: Arc<Mutex<VecDeque<ScriptedResponse>>>,
    recorded: Arc<Mutex<Vec<RecordedRequest>>>,
    shutdown: Arc<AtomicBool>,
}

impl Worker {
    #[expect(
        clippy::disallowed_methods,
        reason = "fixture server runs on its own owned OS thread; 1ms backoff keeps accept-loop tests fast"
    )]
    fn serve(&self) {
        while !self.shutdown.load(Ordering::Acquire) {
            match self.listener.accept() {
                Ok((stream, _)) => self.serve_one(stream),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(_) => {
                    if self.shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "fixture server runs on its own owned OS thread; scripted latency is the test product"
    )]
    fn serve_one(&self, stream: TcpStream) {
        // Accepted sockets inherit the listener's nonblocking mode on
        // BSD/macOS, and read timeouts do not apply to nonblocking reads:
        // without this, `read_request` fails with WouldBlock whenever the
        // server thread outruns the client write. Restore blocking mode so
        // the timeouts below actually bound the reads.
        drop(stream.set_nonblocking(false));
        drop(stream.set_read_timeout(Some(Duration::from_secs(5))));
        drop(stream.set_write_timeout(Some(Duration::from_secs(5))));
        let mut reader = BufReader::new(stream);
        let Ok(request) = read_request(&mut reader) else {
            return;
        };
        if let Ok(mut recorded) = self.recorded.lock() {
            recorded.push(request);
        }
        let response = self.next_response();
        if !response.latency.is_zero() {
            std::thread::sleep(response.latency);
        }
        let stream = reader.into_inner();
        drop(write_response(stream, &response));
    }

    fn next_response(&self) -> ScriptedResponse {
        self.script
            .lock()
            .ok()
            .and_then(|mut script| {
                let response = script.pop_front()?;
                if script.is_empty() {
                    // Last scripted response sticks: keep serving it.
                    script.push_back(response.clone());
                }
                Some(response)
            })
            .unwrap_or_else(|| ScriptedResponse::new(500, Vec::new()))
    }
}

fn read_request(reader: &mut BufReader<TcpStream>) -> std::io::Result<RecordedRequest> {
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    if method.is_empty() || path.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "bad request line",
        ));
    }
    let mut headers = Vec::new();
    let mut content_length = 0_usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_owned();
            let value = value.trim().to_owned();
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }
    let mut body = vec![0_u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(RecordedRequest {
        method,
        path,
        headers,
        body,
    })
}

fn write_response(mut stream: TcpStream, response: &ScriptedResponse) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        ScriptedResponse::reason_phrase(response.status),
        response.body.len()
    );
    for (name, value) in &response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

#[cfg(test)]
mod tests;
