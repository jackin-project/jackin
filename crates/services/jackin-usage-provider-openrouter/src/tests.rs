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

mod case_01;
mod case_02;
