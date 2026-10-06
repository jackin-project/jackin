// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn key_fixture() -> serde_json::Value {
    serde_json::json!({
        "data": {
            "label": "test-key",
            "usage": 12.5,
            "usage_daily": 1.2,
            "usage_weekly": 5.0,
            "usage_monthly": 12.5,
            "limit": 100.0,
            "limit_remaining": 87.5,
            "is_free_tier": false
        }
    })
}

pub(super) fn one_shot_key_server(
    status: u16,
    body: &str,
) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let body = body.to_owned();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..read]).contains("/key"));
        let reason = match status {
            200 => "OK",
            401 => "Unauthorized",
            _ => "Error",
        };
        write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    (format!("http://{address}"), server)
}
