// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const IO_LIMIT: Duration = Duration::from_secs(30);
const BODY_LIMIT: u64 = 8 * 1024 * 1024;

#[async_trait]
pub trait HostEngine: Send + Sync + Debug {
    async fn engine_id(&self) -> Result<String, String>;
    async fn container_names(&self) -> Result<Vec<String>, String>;
}

#[derive(Clone, Debug)]
pub struct SocketEngine {
    socket: PathBuf,
}

impl SocketEngine {
    #[must_use]
    pub fn new(socket: PathBuf) -> SocketEngine {
        SocketEngine { socket }
    }

    #[must_use]
    pub fn from_docker_host(docker_host: Option<&str>) -> SocketEngine {
        let socket = docker_host
            .and_then(|host| host.strip_prefix("unix://"))
            .map_or_else(|| PathBuf::from("/var/run/docker.sock"), PathBuf::from);
        SocketEngine::new(socket)
    }

    async fn get(&self, path: &str) -> Result<Value, String> {
        let work = async {
            let mut stream = UnixStream::connect(&self.socket)
                .await
                .map_err(|error| format!("cannot connect to {}: {error}", self.socket.display()))?;
            let request =
                format!("GET {path} HTTP/1.0\r\nHost: docker\r\nConnection: close\r\n\r\n");
            stream
                .write_all(request.as_bytes())
                .await
                .map_err(|error| error.to_string())?;
            let mut raw = Vec::new();
            stream
                .take(BODY_LIMIT)
                .read_to_end(&mut raw)
                .await
                .map_err(|error| error.to_string())?;
            Ok::<_, String>(raw)
        };
        let raw = tokio::time::timeout(IO_LIMIT, work)
            .await
            .map_err(|_| "the engine socket did not answer in time".to_owned())??;
        let split = raw
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or_else(|| "the engine answered without a header".to_owned())?;
        let head = String::from_utf8_lossy(&raw[..split]).into_owned();
        if !head.starts_with("HTTP/1.0 200") && !head.starts_with("HTTP/1.1 200") {
            return Err(format!(
                "the engine answered {}",
                head.lines().next().unwrap_or("")
            ));
        }
        let body = &raw[split + 4..];
        let body = if head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            dechunk(body)?
        } else {
            body.to_vec()
        };
        serde_json::from_slice(&body).map_err(|error| format!("unreadable engine answer: {error}"))
    }
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line_end = body
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(|| "bad chunked body".to_owned())?;
        let size = usize::from_str_radix(String::from_utf8_lossy(&body[..line_end]).trim(), 16)
            .map_err(|_| "bad chunk size".to_owned())?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size + 2 {
            return Err("short chunk".to_owned());
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

#[async_trait]
impl HostEngine for SocketEngine {
    async fn engine_id(&self) -> Result<String, String> {
        let info = self.get("/info").await?;
        info.get("ID")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| "the engine info has no ID".to_owned())
    }

    async fn container_names(&self) -> Result<Vec<String>, String> {
        let list = self.get("/containers/json?all=1").await?;
        let items = list
            .as_array()
            .ok_or_else(|| "no container list".to_owned())?;
        Ok(items
            .iter()
            .flat_map(|item| {
                item.get("Names")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|name| name.trim_start_matches('/').to_owned())
            })
            .collect())
    }
}
