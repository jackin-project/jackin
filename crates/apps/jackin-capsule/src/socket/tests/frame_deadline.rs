// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::UnixStream;
use tokio::sync::oneshot;

use super::super::{MAX_CONCURRENT_CLIENTS, read_control_msg, start_listener_at_with_limiter};

const AWAIT_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) async fn run() -> Result<()> {
    let (attach_partial, attach_whole, control_partial, control_whole) = tokio::join!(
        check_real_frame_deadline(true, true),
        check_real_frame_deadline(true, false),
        check_real_frame_deadline(false, true),
        check_real_frame_deadline(false, false),
    );
    attach_partial?;
    attach_whole?;
    control_partial?;
    control_whole?;
    Ok(())
}

async fn bounded<F>(future: F, label: &'static str) -> Result<F::Output>
where
    F: Future,
{
    tokio::time::timeout(AWAIT_TIMEOUT, future)
        .await
        .with_context(|| format!("{label} timed out"))
}

async fn check_real_frame_deadline(attach: bool, partial_prefix: bool) -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let socket_path = tmp.path().join("run/jackin.sock");
    let (mut rx, limiter) = start_listener_at_with_limiter(&socket_path)?;
    let mut client = bounded(UnixStream::connect(&socket_path), "initial UDS connect").await??;
    let (mut server, permit) = bounded(rx.recv(), "initial listener accept")
        .await?
        .context("accepted connection")?;
    assert_eq!(limiter.available_permits(), MAX_CONCURRENT_CLIENTS - 1);

    let payload_len = 4u32 * 16 * 1024;
    let prefix = payload_len.to_be_bytes();
    let first_byte = if attach {
        crate::protocol::attach::TAG_INPUT
    } else {
        prefix[0]
    };
    bounded(
        client.write_all(&[first_byte]),
        "initial discriminator write",
    )
    .await??;
    let mut tag = [0u8; 1];
    bounded(server.read_exact(&mut tag), "initial discriminator read").await??;
    assert_eq!(tag[0], first_byte);

    // The read budget begins after the discriminator byte supplied above.
    let started = Instant::now();
    let mut reader = tokio::spawn(async move {
        let _permit = permit;
        if attach {
            crate::protocol::attach::read_client_frame(&mut server, tag[0])
                .await
                .map(|_| ())
        } else {
            read_control_msg(&mut server, tag[0]).await.map(|_| ())
        }
    });
    let suffix = if attach { &prefix[..] } else { &prefix[1..] };
    if partial_prefix {
        bounded(
            client.write_all(&suffix[..1]),
            "partial length-prefix write",
        )
        .await??;
        tokio::time::sleep(Duration::from_secs(3)).await;
        bounded(
            client.write_all(&suffix[1..]),
            "remaining length-prefix write",
        )
        .await??;
    } else {
        bounded(client.write_all(suffix), "complete length-prefix write").await??;
    }

    // Each payload chunk arrives within ten seconds, but the whole frame exceeds ten seconds.
    let payload_chunk = [b' '; 16 * 1024];
    let chunk_count = if partial_prefix { 2 } else { 3 };
    for _ in 0..chunk_count {
        tokio::time::sleep(Duration::from_secs(3)).await;
        bounded(client.write_all(&payload_chunk), "trickle payload write").await??;
    }

    let reader_result =
        if let Ok(joined) = tokio::time::timeout(Duration::from_secs(3), &mut reader).await {
            joined.context("frame reader task panicked")?
        } else {
            reader.abort();
            let joined = bounded(&mut reader, "renewed reader cancellation").await?;
            match joined {
                Err(error) if error.is_cancelled() => {}
                Ok(_) | Err(_) => bail!("reader did not stop after cancellation"),
            }
            bail!("reader renewed its frame budget");
        };
    let error = reader_result.expect_err("incomplete frame must expire");
    assert!(format!("{error:#}").contains("timed out"), "{error:#}");
    assert!(started.elapsed() >= Duration::from_secs(9));
    assert!(started.elapsed() < Duration::from_secs(12));
    assert_eq!(limiter.available_permits(), MAX_CONCURRENT_CLIENTS);
    let mut byte = [0u8; 1];
    assert_eq!(
        bounded(client.read(&mut byte), "expired peer EOF read").await??,
        0
    );

    // Cancellation drops the UDS owner and returns its permit.
    let mut canceled_client =
        bounded(UnixStream::connect(&socket_path), "cancel UDS connect").await??;
    let (mut canceled_server, permit) = bounded(rx.recv(), "cancel listener accept")
        .await?
        .context("cancel connection")?;
    let (started_tx, started_rx) = oneshot::channel();
    let canceled = tokio::spawn(async move {
        let _permit = permit;
        let _send_result = started_tx.send(());
        if attach {
            crate::protocol::attach::read_client_frame(&mut canceled_server, first_byte)
                .await
                .map(|_| ())
        } else {
            read_control_msg(&mut canceled_server, first_byte)
                .await
                .map(|_| ())
        }
    });
    bounded(started_rx, "cancel reader start")
        .await?
        .context("cancel reader did not start")?;
    canceled.abort();
    let joined = bounded(canceled, "canceled reader join").await?;
    assert!(
        joined
            .expect_err("reader task must be canceled")
            .is_cancelled()
    );
    assert_eq!(limiter.available_permits(), MAX_CONCURRENT_CLIENTS);
    assert_eq!(
        bounded(canceled_client.read(&mut byte), "canceled peer EOF read").await??,
        0
    );

    // A fresh fragmented frame must decode after the expired connection.
    let fresh_client =
        bounded(UnixStream::connect(&socket_path), "reconnect UDS connect").await??;
    let (mut fresh_server, permit) = bounded(rx.recv(), "reconnect listener accept")
        .await?
        .context("reconnect")?;
    check_fragmented_fresh_frame(fresh_client, &mut fresh_server, attach, first_byte).await?;
    drop((fresh_server, permit));
    assert_eq!(limiter.available_permits(), MAX_CONCURRENT_CLIENTS);
    Ok(())
}

async fn check_fragmented_fresh_frame(
    mut fresh_client: UnixStream,
    fresh_server: &mut UnixStream,
    attach: bool,
    attach_tag: u8,
) -> Result<()> {
    let mut body = if attach {
        vec![b'x'; 16 * 1024 + 1]
    } else {
        let mut body = br#"{"ctx":{"v":1},"msg":{"type":"status"}}"#.to_vec();
        body.resize(16 * 1024 + 1, b' ');
        body
    };
    let prefix = u32::try_from(body.len())?.to_be_bytes();
    let first_byte = if attach { attach_tag } else { prefix[0] };
    bounded(
        fresh_client.write_all(&[first_byte]),
        "reconnect discriminator write",
    )
    .await??;
    let mut received_first_byte = [0u8; 1];
    bounded(
        fresh_server.read_exact(&mut received_first_byte),
        "reconnect discriminator read",
    )
    .await??;
    assert_eq!(received_first_byte[0], first_byte);

    let writer = tokio::spawn(async move {
        let suffix = if attach { &prefix[..] } else { &prefix[1..] };
        bounded(
            fresh_client.write_all(&suffix[..1]),
            "reconnect partial length-prefix write",
        )
        .await??;
        tokio::time::sleep(Duration::from_millis(10)).await;
        bounded(
            fresh_client.write_all(&suffix[1..]),
            "reconnect remaining length-prefix write",
        )
        .await??;
        let final_byte = body.pop().context("last payload byte")?;
        bounded(
            fresh_client.write_all(&body),
            "reconnect payload chunk write",
        )
        .await??;
        tokio::time::sleep(Duration::from_millis(10)).await;
        bounded(
            fresh_client.write_all(&[final_byte]),
            "reconnect final payload byte write",
        )
        .await??;
        Ok::<_, anyhow::Error>(())
    });

    if attach {
        let decoded = bounded(
            crate::protocol::attach::read_client_frame(fresh_server, received_first_byte[0]),
            "reconnect attach frame read",
        )
        .await??
        .context("fresh attach frame")?;
        assert!(matches!(
            decoded,
            crate::protocol::attach::ClientFrame::Input(bytes)
                if bytes == vec![b'x'; 16 * 1024 + 1]
        ));
    } else {
        let decoded = bounded(
            read_control_msg(fresh_server, received_first_byte[0]),
            "reconnect control frame read",
        )
        .await??;
        assert!(matches!(
            decoded.msg,
            crate::protocol::control::ClientMsg::Status
        ));
    }
    bounded(writer, "reconnect writer join")
        .await?
        .context("reconnect writer task failed")??;
    Ok(())
}
