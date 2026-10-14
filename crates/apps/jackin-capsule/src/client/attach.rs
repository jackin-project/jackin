// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, bail};

use crate::protocol::attach::SpawnRequest;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::socket::SOCKET_PATH;

pub async fn run_client(
    spawn_request: Option<SpawnRequest>,
    focus_session: Option<u64>,
) -> Result<()> {
    crate::tui::run::run_client(spawn_request, focus_session).await
}

pub async fn run_attach_proxy() -> Result<()> {
    run_attach_proxy_at(SOCKET_PATH, tokio::io::stdin(), tokio::io::stdout()).await
}

pub async fn run_protocol_check(args: &[String]) -> Result<()> {
    let expected_major = match args {
        [] => jackin_protocol::capsule_transport::CONTROL_PROTOCOL_MAJOR,
        [flag, value] if flag == "--expected-major" => value
            .parse::<u16>()
            .context("--expected-major must be an unsigned 16-bit integer")?,
        _ => bail!("usage: jackin-capsule protocol-check [--expected-major <major>]"),
    };
    anyhow::ensure!(
        expected_major == jackin_protocol::capsule_transport::CONTROL_PROTOCOL_MAJOR,
        "Capsule client protocol major {} does not match required major {}",
        jackin_protocol::capsule_transport::CONTROL_PROTOCOL_MAJOR,
        expected_major
    );
    let mut stream = jackin_diagnostics::operation::connection_attempt(
        jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleControl,
        UnixStream::connect(SOCKET_PATH),
    )
    .await
    .with_context(|| format!("cannot connect to jackin-capsule daemon at {SOCKET_PATH}"))?;
    jackin_protocol::capsule_transport::client_handshake_async(&mut stream)
        .await
        .context("Capsule transport protocol check failed")?;
    Ok(())
}

pub(crate) async fn run_attach_proxy_at<R, W>(socket_path: &str, input: R, output: W) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut stream = jackin_diagnostics::operation::connection_attempt(
        jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleAttach,
        UnixStream::connect(socket_path),
    )
    .await
    .with_context(|| format!("cannot connect to jackin-capsule daemon at {socket_path}"))?;
    jackin_protocol::capsule_transport::client_handshake_async(&mut stream)
        .await
        .context("negotiating Capsule attach transport")?;
    let (mut socket_read, mut socket_write) = stream.into_split();
    let mut input = input;
    let mut output = output;

    let input_to_socket = async {
        tokio::io::copy(&mut input, &mut socket_write).await?;
        socket_write.shutdown().await?;
        Ok::<(), std::io::Error>(())
    };
    let socket_to_output = async {
        tokio::io::copy(&mut socket_read, &mut output).await?;
        output.shutdown().await?;
        Ok::<(), std::io::Error>(())
    };

    tokio::pin!(input_to_socket);
    tokio::pin!(socket_to_output);
    tokio::select! {
        result = &mut input_to_socket => {
            result.context("relaying stdin to attach socket")?;
            socket_to_output.await.context("relaying attach socket to stdout")?;
        }
        result = &mut socket_to_output => {
            result.context("relaying attach socket to stdout")?;
        }
    }
    Ok(())
}
