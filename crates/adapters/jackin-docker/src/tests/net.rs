// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::io::{Read as _, Write as _};

use reqwest::header::HeaderMap;

use crate::net::{USER_AGENT, get_text, http_client};

fn serve_once(response: &'static [u8]) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("loopback addr");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let mut request = vec![0u8; 4096];
        drop(stream.read(&mut request));
        stream.write_all(response).expect("write response");
    });
    let url = format!("http://{addr}/object");
    let client = http_client(HeaderMap::new()).expect("client builds");
    let result = tokio_test_block_on(get_text(&client, &url));
    server.join().expect("server finishes");
    result.expect("request succeeds")
}

fn tokio_test_block_on<F>(future: F) -> F::Output
where
    F: Future,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread runtime")
        .block_on(future)
}

#[test]
fn user_agent_carries_crate_name() {
    assert!(USER_AGENT.starts_with("jackin/"), "{USER_AGENT}");
}

#[test]
fn get_text_returns_body_for_success_status() {
    let body =
        serve_once(b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\nconnection: close\r\n\r\nhello");
    assert_eq!(body, "hello");
}

#[test]
fn get_text_rejects_error_status_with_url() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("loopback addr");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let mut request = vec![0u8; 4096];
        drop(stream.read(&mut request));
        stream
            .write_all(b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .expect("write response");
    });
    let url = format!("http://{addr}/object");
    let client = http_client(HeaderMap::new()).expect("client builds");
    let error = tokio_test_block_on(get_text(&client, &url)).expect_err("status is rejected");
    server.join().expect("server finishes");
    assert_eq!(
        error.to_string(),
        format!("{url} failed: HTTP 500 Internal Server Error")
    );
}
