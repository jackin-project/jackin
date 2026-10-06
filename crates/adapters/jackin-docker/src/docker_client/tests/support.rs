// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
const _: fn() -> Result<Docker, bollard::errors::Error> = Docker::connect_with_ssl_defaults;

pub(super) fn content_length(headers: &str) -> anyhow::Result<usize> {
    headers
        .lines()
        .find(|line| {
            line.split_once(':')
                .is_some_and(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        })
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim().parse::<usize>())
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("Docker request omitted content-length"))
}

pub(super) fn capture_docker_create_request(
    listener: std::net::TcpListener,
) -> std::thread::JoinHandle<anyhow::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        use std::io::{Read as _, Write as _};

        let (mut stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        let (header_end, content_length) = loop {
            let read = stream.read(&mut chunk)?;
            anyhow::ensure!(read > 0, "Docker request ended before headers");
            request.extend_from_slice(&chunk[..read]);
            if let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&request[..header_end])?;
                break (header_end + 4, content_length(headers)?);
            }
        };
        while request.len() < header_end + content_length {
            let read = stream.read(&mut chunk)?;
            anyhow::ensure!(read > 0, "Docker request body ended early");
            request.extend_from_slice(&chunk[..read]);
        }
        let response = br#"{"Id":"wire-created-container","Warnings":[]}"#;
        write!(
            stream,
            "HTTP/1.1 201 Created\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            response.len()
        )?;
        stream.write_all(response)?;
        Ok(request)
    })
}

pub(super) async fn exercise_private_container_create() -> anyhow::Result<Vec<String>> {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    let request = capture_docker_create_request(listener);
    let inner = Docker::connect_with_http(
        &format!("http://{address}"),
        2,
        bollard::API_DEFAULT_VERSION,
    )?;
    let client = BollardDockerClient { inner };
    let private = vec![
        "wire-private-container-name".to_owned(),
        "registry.invalid/wire-private-image@sha256:deadbeef".to_owned(),
        "wire-private-label-key".to_owned(),
        "wire-private-label-value".to_owned(),
        "WIRE_PRIVATE_ENV=wire-private-env-value".to_owned(),
        "/wire-private-host-bind:/wire-private-container-bind:ro".to_owned(),
        "wire-private-network".to_owned(),
        "/wire-private-workdir".to_owned(),
        "wire-private-hostname".to_owned(),
        "/wire-private-entrypoint".to_owned(),
    ];
    client
        .create_container(
            &private[0],
            ContainerSpec {
                image: private[1].clone(),
                hostname: Some(private[8].clone()),
                env: vec![private[4].clone()],
                labels: [(private[2].clone(), private[3].clone())]
                    .into_iter()
                    .collect(),
                network: private[6].clone(),
                binds: vec![private[5].clone()],
                entrypoint: Some(vec![private[9].clone()]),
                privileged: false,
                workdir: Some(private[7].clone()),
                ..Default::default()
            },
        )
        .await?;
    let request = request
        .join()
        .map_err(|_| anyhow::anyhow!("Docker mock thread panicked"))??;
    let request = String::from_utf8(request)?;
    for value in &private {
        anyhow::ensure!(request.contains(value), "Docker request omitted {value}");
    }
    anyhow::ensure!(
        request.starts_with("POST /containers/create?name="),
        "unexpected Docker create route: {request}"
    );
    Ok(private)
}

pub(super) fn context_endpoint(host: &str) -> DockerContextEndpoint {
    DockerContextEndpoint::new(host, false, false)
}
