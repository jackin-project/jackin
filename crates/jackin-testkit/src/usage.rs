// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_core::Provider;
use jackin_usage::ProviderEndpoints;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, OnceCell, watch};
use tokio::task::JoinHandle;

use crate::sync::locked;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
}

impl Method {
    fn token(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Route {
    status: u16,
    body: String,
    headers: Vec<(String, String)>,
    require_headers: Vec<(String, String)>,
    require_body: Option<Value>,
    delay: Option<Duration>,
    framed: bool,
}

impl Route {
    pub fn json(status: u16, body: &str) -> Route {
        Route {
            status,
            body: body.to_owned(),
            headers: Vec::new(),
            require_headers: Vec::new(),
            require_body: None,
            delay: None,
            framed: true,
        }
    }

    pub fn status(status: u16) -> Route {
        Route::json(status, "")
    }

    #[must_use]
    pub fn reply_header(mut self, name: &str, value: &str) -> Route {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    #[must_use]
    pub fn requires_header(mut self, name: &str, value: &str) -> Route {
        self.require_headers
            .push((name.to_ascii_lowercase(), value.to_owned()));
        self
    }

    #[must_use]
    pub fn requires_json_body(mut self, body: Value) -> Route {
        self.require_body = Some(body);
        self
    }

    #[must_use]
    pub fn unframed(mut self) -> Route {
        self.framed = false;
        self
    }

    #[must_use]
    pub fn delayed(mut self, delay: Duration) -> Route {
        self.delay = Some(delay);
        self
    }
}

#[derive(Debug)]
struct Gate {
    arrived: Notify,
    released: watch::Sender<bool>,
}

#[derive(Clone, Debug)]
pub struct Held {
    gate: Arc<Gate>,
}

impl Held {
    pub async fn arrived(&self) {
        self.gate.arrived.notified().await;
    }

    pub fn release(&self) {
        self.gate.released.send_replace(true);
    }
}

type Key = (Method, String);

#[derive(Debug, Default)]
struct State {
    routes: HashMap<Key, Vec<Route>>,
    holds: HashMap<Key, Arc<Gate>>,
    mismatches: Vec<String>,
}

struct Request {
    method: Method,
    target: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

#[derive(Debug)]
pub struct UsageServer {
    state: Arc<Mutex<State>>,
    addr: SocketAddr,
    accept: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug, Default)]
pub struct LazyUsageServer {
    cell: OnceCell<UsageServer>,
}

impl LazyUsageServer {
    pub async fn get(&self) -> io::Result<&UsageServer> {
        self.cell.get_or_try_init(UsageServer::start).await
    }
}

impl UsageServer {
    pub async fn start() -> io::Result<UsageServer> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let state = Arc::new(Mutex::new(State::default()));
        let shared = Arc::clone(&state);
        let accept = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let shared = Arc::clone(&shared);
                tokio::spawn(async move {
                    let _served = serve_connection(stream, shared).await;
                });
            }
        });
        Ok(UsageServer {
            state,
            addr,
            accept: Mutex::new(Some(accept)),
        })
    }

    pub fn serve(&self, provider: Provider, method: Method, path: &str, route: Route) {
        locked(&self.state)
            .routes
            .entry(key(provider, method, path))
            .or_default()
            .push(route);
    }

    pub fn replace(&self, provider: Provider, method: Method, path: &str, route: Route) {
        locked(&self.state)
            .routes
            .insert(key(provider, method, path), vec![route]);
    }

    pub fn hold(&self, provider: Provider, method: Method, path: &str) -> Held {
        let (released, _) = watch::channel(false);
        let gate = Arc::new(Gate {
            arrived: Notify::new(),
            released,
        });
        locked(&self.state)
            .holds
            .insert(key(provider, method, path), Arc::clone(&gate));
        Held { gate }
    }

    pub async fn shutdown(&self) {
        let handle = locked(&self.accept).take();
        if let Some(handle) = handle {
            handle.abort();
            let _joined = handle.await;
        }
    }

    pub fn mismatches(&self) -> Vec<String> {
        locked(&self.state).mismatches.clone()
    }

    pub fn endpoints(&self) -> ProviderEndpoints {
        Provider::ALL
            .into_iter()
            .fold(ProviderEndpoints::real(), |endpoints, provider| {
                endpoints.with(
                    provider,
                    format!("http://{}/{}", self.addr, provider.as_str()),
                )
            })
    }
}

fn key(provider: Provider, method: Method, path: &str) -> Key {
    (method, format!("/{}{path}", provider.as_str()))
}

async fn serve_connection(stream: TcpStream, state: Arc<Mutex<State>>) -> io::Result<()> {
    let mut reader = BufReader::new(stream);
    let Some(request) = read_request(&mut reader).await? else {
        return Ok(());
    };
    let gate = locked(&state)
        .holds
        .get(&(request.method, request.target.clone()))
        .cloned();
    if let Some(gate) = gate {
        gate.arrived.notify_one();
        let mut released = gate.released.subscribe();
        let _open = released.wait_for(|open| *open).await;
    }
    let (reply, delay) = answer(&request, &state);
    if let Some(delay) = delay {
        tokio::time::sleep(delay).await;
    }
    let mut stream = reader.into_inner();
    stream.write_all(&reply).await?;
    stream.shutdown().await
}

fn answer(request: &Request, state: &Mutex<State>) -> (Vec<u8>, Option<Duration>) {
    let mut guard = locked(state);
    let candidates = guard
        .routes
        .get(&(request.method, request.target.clone()))
        .cloned()
        .unwrap_or_default();
    let mut unmet = vec![format!(
        "no route for {} {}",
        request.method.token(),
        request.target
    )];
    for route in candidates {
        match unmet_requirement(&route, request) {
            None => return (render(&route), route.delay),
            Some(reason) => unmet = vec![reason],
        }
    }
    guard.mismatches.extend(unmet);
    (render(&Route::status(401)), None)
}

fn unmet_requirement(route: &Route, request: &Request) -> Option<String> {
    for (name, value) in &route.require_headers {
        if request.headers.get(name) != Some(value) {
            return Some(format!(
                "header {name} on {} did not match the route",
                request.target
            ));
        }
    }
    if let Some(wanted) = &route.require_body {
        let found: Option<Value> = serde_json::from_slice(&request.body).ok();
        if found.as_ref() != Some(wanted) {
            return Some(format!(
                "body of {} did not match the route",
                request.target
            ));
        }
    }
    None
}

fn render(route: &Route) -> Vec<u8> {
    let mut head = format!("HTTP/1.1 {} {}\r\n", route.status, reason(route.status));
    if route.framed {
        head.push_str(&format!("Content-Length: {}\r\n", route.body.len()));
    }
    head.push_str("Connection: close\r\nContent-Type: application/json\r\n");
    for (name, value) in &route.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(route.body.as_bytes());
    bytes
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

async fn read_request(reader: &mut BufReader<TcpStream>) -> io::Result<Option<Request>> {
    let mut line = String::new();
    if reader.read_line(&mut line).await? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let method = match parts.next() {
        Some("GET") => Method::Get,
        Some("POST") => Method::Post,
        _ => return Ok(None),
    };
    let target = parts.next().unwrap_or("/").to_owned();
    let mut headers = HashMap::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).await? == 0 {
            return Ok(None);
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let length: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    Ok(Some(Request {
        method,
        target,
        headers,
        body,
    }))
}
