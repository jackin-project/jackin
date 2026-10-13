// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn get(
    addr: &std::net::SocketAddr,
    path: &str,
    extra_headers: &[(&str, &str)],
) -> Result<String> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut request = format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n");
    for (name, value) in extra_headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

pub(super) fn post(addr: &std::net::SocketAddr, path: &str, body: &str) -> Result<String> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}
