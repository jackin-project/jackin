// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_protocol::control::StatusSlot;

fn account_credits_bucket(
    view: &jackin_protocol::control::FocusedUsageView,
) -> &jackin_protocol::control::QuotaBucketView {
    view.buckets
        .iter()
        .find(|bucket| bucket.label == "Account credits")
        .unwrap_or_else(|| {
            panic!(
                "account credits bucket missing: last_error={:?}, bucket_labels={:?}",
                view.last_error,
                view.buckets
                    .iter()
                    .map(|bucket| bucket.label.as_str())
                    .collect::<Vec<_>>()
            )
        })
}

pub(super) const fn key_fixture() -> &'static str {
    r#"{"data":{"label":"test-key","usage":12.5,"usage_daily":1.2,"usage_weekly":5.0,"usage_monthly":12.5,"limit":100.0,"limit_remaining":87.5,"is_free_tier":false}}"#
}

struct CannedSnapshotServer {
    base_url: String,
    stop: std::sync::mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl CannedSnapshotServer {
    fn start(key_body: &'static str) -> Self {
        Self::start_with_credits(
            key_body,
            serde_json::json!({"data": {"total_credits": 10.0, "total_usage": 2.5}}).to_string(),
        )
    }

    fn start_with_credits(key_body: &'static str, credits_body: String) -> Self {
        use std::net::{Ipv4Addr, TcpListener};
        use std::sync::mpsc;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, worker_stop) = mpsc::channel();
        let worker = jackin_telemetry::spawn::thread_joined(move || {
            loop {
                if !serve_connection(&listener, &worker_stop, key_body, &credits_body) {
                    return;
                }
            }
        });
        Self {
            base_url: format!("http://{address}"),
            stop,
            worker: Some(worker),
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }
}

fn serve_connection(
    listener: &std::net::TcpListener,
    stop: &std::sync::mpsc::Receiver<()>,
    key_body: &'static str,
    credits_body: &str,
) -> bool {
    use std::io::{Read, Write};
    use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
    use std::time::{Duration, Instant};

    const MAX_REQUEST_BYTES: usize = 8192;
    const IO_TIMEOUT: Duration = Duration::from_millis(250);

    if matches!(stop.try_recv(), Ok(()) | Err(TryRecvError::Disconnected)) {
        return false;
    }
    let mut stream = match listener.accept() {
        Ok((stream, _)) => stream,
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            return matches!(
                stop.recv_timeout(Duration::from_millis(5)),
                Err(RecvTimeoutError::Timeout)
            );
        }
        Err(_) => return false,
    };

    let header_deadline = Instant::now() + Duration::from_secs(5);
    if stream.set_write_timeout(Some(IO_TIMEOUT)).is_err() {
        return false;
    }
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        if request.windows(4).any(|part| part == b"\r\n\r\n") {
            break;
        }
        if request.len() >= MAX_REQUEST_BYTES || Instant::now() >= header_deadline {
            return false;
        }
        if matches!(stop.try_recv(), Ok(()) | Err(TryRecvError::Disconnected)) {
            return false;
        }
        let remaining = header_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero()
            || stream
                .set_read_timeout(Some(remaining.min(IO_TIMEOUT)))
                .is_err()
        {
            return false;
        }
        let count = match stream.read(&mut chunk) {
            Ok(count) if count > 0 => count,
            Ok(_) => return false,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(_) => return false,
        };
        if request.len() + count > MAX_REQUEST_BYTES {
            return false;
        }
        request.extend_from_slice(&chunk[..count]);
    }

    let body = if String::from_utf8_lossy(&request).contains("/credits") {
        credits_body.to_owned()
    } else {
        key_body.to_owned()
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    if stream.write_all(response.as_bytes()).is_err() {
        return false;
    }
    true
}

impl Drop for CannedSnapshotServer {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _send_result = self.stop.send(());
            drop(worker.join());
        }
    }
}

enum ChildSnapshotResponse {
    Close,
    Http {
        status: u16,
        reason: &'static str,
        header: Option<(&'static str, &'static str)>,
        body: &'static str,
    },
}

struct ChildSnapshotServer {
    base_url: String,
    request_seen: std::sync::mpsc::Receiver<()>,
    stop: std::sync::mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl ChildSnapshotServer {
    fn start(response: ChildSnapshotResponse) -> Self {
        use std::net::{Ipv4Addr, TcpListener};
        use std::sync::mpsc;

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, worker_stop) = mpsc::channel();
        let (worker_request_seen, request_seen) = mpsc::channel();
        let worker = jackin_telemetry::spawn::thread_joined(move || {
            serve_child_snapshot_once(&listener, &worker_stop, &worker_request_seen, response);
        });
        Self {
            base_url: format!("http://{address}"),
            request_seen,
            stop,
            worker: Some(worker),
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn request_seen(&self) -> bool {
        matches!(self.request_seen.try_recv(), Ok(()))
    }
}

fn serve_child_snapshot_once(
    listener: &std::net::TcpListener,
    stop: &std::sync::mpsc::Receiver<()>,
    request_seen: &std::sync::mpsc::Sender<()>,
    response: ChildSnapshotResponse,
) {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
    use std::time::{Duration, Instant};

    const MAX_REQUEST_BYTES: usize = 8192;
    const IO_TIMEOUT: Duration = Duration::from_millis(250);

    let mut stream: TcpStream = loop {
        if matches!(stop.try_recv(), Ok(()) | Err(TryRecvError::Disconnected)) {
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if !matches!(
                    stop.recv_timeout(Duration::from_millis(5)),
                    Err(RecvTimeoutError::Timeout)
                ) {
                    return;
                }
            }
            Err(_) => return,
        }
    };

    if stream.set_write_timeout(Some(IO_TIMEOUT)).is_err() {
        return;
    }
    let header_deadline = Instant::now() + Duration::from_secs(5);
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|part| part == b"\r\n\r\n") {
        if request.len() >= MAX_REQUEST_BYTES || Instant::now() >= header_deadline {
            return;
        }
        if matches!(stop.try_recv(), Ok(()) | Err(TryRecvError::Disconnected)) {
            return;
        }
        let remaining = header_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero()
            || stream
                .set_read_timeout(Some(remaining.min(IO_TIMEOUT)))
                .is_err()
        {
            return;
        }
        let count = match stream.read(&mut chunk) {
            Ok(count) if count > 0 => count,
            Ok(_) => return,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(_) => return,
        };
        if request.len() + count > MAX_REQUEST_BYTES {
            return;
        }
        request.extend_from_slice(&chunk[..count]);
    }
    let _request_result = request_seen.send(());

    match response {
        ChildSnapshotResponse::Close => drop(stream),
        ChildSnapshotResponse::Http {
            status,
            reason,
            header,
            body,
        } => {
            let mut response = format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\nconnection: close\r\n",
                body.len()
            );
            if let Some((name, value)) = header {
                response.push_str(name);
                response.push_str(": ");
                response.push_str(value);
                response.push_str("\r\n");
            }
            response.push_str("\r\n");
            let _write_result = stream
                .write_all(response.as_bytes())
                .and_then(|()| stream.write_all(body.as_bytes()));
        }
    }
}

impl Drop for ChildSnapshotServer {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _send_result = self.stop.send(());
            drop(worker.join());
        }
    }
}

fn run_snapshot_child(
    server: &ChildSnapshotServer,
    test_name: &str,
    scenario: &str,
    completion_marker: &str,
) -> std::process::Output {
    use std::process::{Command, Stdio};

    let child_state = format!("{scenario}|{completion_marker}");
    let executable = std::env::current_exe()
        .unwrap_or_else(|error| panic!("current test executable is unavailable: {error}"));
    let mut command = Command::new(executable);
    command
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .env_clear()
        .env("OPENROUTER_BASE_URL", server.base_url())
        .env("JACKIN_OPENROUTER_TEST_CHILD", child_state)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if test_name == "tests::case_02::openrouter_snapshot_public_429_preserves_retry_after_deadline"
    {
        command.arg("--include-ignored");
    }
    command
        .spawn()
        .unwrap_or_else(|error| panic!("could not spawn current test executable: {error}"))
        .wait_with_output()
        .unwrap_or_else(|error| panic!("could not reap child test process: {error}"))
}

fn assert_snapshot_child_completed(
    output: &std::process::Output,
    request_seen: bool,
    completion_marker: &str,
) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "child test failed with status {:?}; request_seen={request_seen}; stdout={stdout:?}; stderr={stderr:?}",
        output.status.code()
    );
    assert!(
        request_seen,
        "child exited without reaching the loopback fixture; stdout={stdout:?}; stderr={stderr:?}"
    );
    assert!(
        stdout.lines().any(|line| line == completion_marker),
        "child exited without its post-assertion completion marker; stdout={stdout:?}; stderr={stderr:?}"
    );
}

mod case_01;
mod case_02;
