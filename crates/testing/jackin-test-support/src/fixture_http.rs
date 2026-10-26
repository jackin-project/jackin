use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ScriptedResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    latency: Duration,
}

impl ScriptedResponse {
    #[must_use]
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
            latency: Duration::ZERO,
        }
    }

    #[must_use]
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    #[must_use]
    pub fn with_retry_after_secs(self, secs: u64) -> Self {
        self.with_header("Retry-After", &secs.to_string())
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RecordedRequest {
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    #[must_use]
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

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

#[derive(Debug)]
pub struct FixtureHttpServer {
    addr: SocketAddr,
    script: Arc<Mutex<VecDeque<ScriptedResponse>>>,
    recorded: Arc<Mutex<Vec<RecordedRequest>>>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FixtureHttpServer {
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

    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    #[must_use]
    pub fn request_count(&self) -> usize {
        self.recorded
            .lock()
            .map(|guard| guard.len())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.recorded
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub fn push_response(&self, response: ScriptedResponse) {
        if let Ok(mut script) = self.script.lock() {
            script.push_back(response);
        }
    }

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
