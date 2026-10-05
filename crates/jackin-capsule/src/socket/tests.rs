// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `socket`.
use super::*;
use crate::protocol::control::ClientMsg;

const FRAME_DEADLINE_CHILD: &str = "JACKIN_FRAME_DEADLINE_CHILD";
const FRAME_DEADLINE_TEST: &str =
    "socket::tests::real_uds_frames_share_one_deadline_and_release_connections";

/// Exercise actual Unix sockets and wall time in a separate process. Clock
/// advancement alone cannot establish that stalled socket owners get dropped.
#[test]
fn real_uds_frames_share_one_deadline_and_release_connections() -> Result<()> {
    if std::env::var_os(FRAME_DEADLINE_CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", FRAME_DEADLINE_TEST, "--nocapture"])
            .env(FRAME_DEADLINE_CHILD, "1")
            .status()?;
        anyhow::ensure!(status.success(), "isolated frame deadline test failed");
        return Ok(());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let (a, b, c, d) = tokio::join!(
            check_real_frame_deadline(true, true),
            check_real_frame_deadline(true, false),
            check_real_frame_deadline(false, true),
            check_real_frame_deadline(false, false),
        );
        a?;
        b?;
        c?;
        d?;
        Ok(())
    })
}

async fn check_real_frame_deadline(attach: bool, partial_prefix: bool) -> Result<()> {
    use tokio::io::AsyncReadExt;

    let tmp = tempfile::tempdir()?;
    let socket_path = tmp.path().join("run/jackin.sock");
    let (mut rx, limiter) = start_listener_at_with_limiter(&socket_path)?;
    let mut client = UnixStream::connect(&socket_path).await?;
    let (mut server, permit) = rx.recv().await.context("accepted connection")?;
    assert_eq!(limiter.available_permits(), MAX_CONCURRENT_CLIENTS - 1);

    let payload_len = 4u32 * 16 * 1024;
    let prefix = payload_len.to_be_bytes();
    let first = if attach {
        crate::protocol::attach::TAG_INPUT
    } else {
        prefix[0]
    };
    client.write_all(&[first]).await?;
    let mut tag = [0];
    server.read_exact(&mut tag).await?;
    let started = std::time::Instant::now();
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
        client.write_all(&suffix[..1]).await?;
        tokio::time::sleep(Duration::from_secs(3)).await;
        client.write_all(&suffix[1..]).await?;
    } else {
        client.write_all(suffix).await?;
    }
    // Every read makes progress within ten seconds. The entire frame does not.
    for _ in 0..if partial_prefix { 2 } else { 3 } {
        tokio::time::sleep(Duration::from_secs(3)).await;
        client.write_all(&[b' '; 16 * 1024]).await?;
    }
    let result = match tokio::time::timeout(Duration::from_secs(3), &mut reader).await {
        Ok(result) => result?,
        Err(_) => {
            reader.abort();
            drop(reader.await);
            anyhow::bail!("reader renewed its frame budget");
        }
    };
    let error = result.expect_err("incomplete frame must expire");
    assert!(format!("{error:#}").contains("timed out"), "{error:#}");
    assert!(started.elapsed() >= Duration::from_secs(9));
    assert!(started.elapsed() < Duration::from_secs(12));
    assert_eq!(limiter.available_permits(), MAX_CONCURRENT_CLIENTS);
    let mut byte = [0];
    assert_eq!(client.read(&mut byte).await?, 0, "reader must drop socket");

    // Cancellation must also release the stream and permit, then a reconnect
    // must decode normally without inheriting the expired frame's deadline.
    let mut canceled_client = UnixStream::connect(&socket_path).await?;
    let (mut canceled_server, permit) = rx.recv().await.context("cancel connection")?;
    let canceled = tokio::spawn(async move {
        let _permit = permit;
        if attach {
            crate::protocol::attach::read_client_frame(&mut canceled_server, first)
                .await
                .map(|_| ())
        } else {
            read_control_msg(&mut canceled_server, 0).await.map(|_| ())
        }
    });
    tokio::task::yield_now().await;
    canceled.abort();
    assert!(canceled.await.expect_err("reader canceled").is_cancelled());
    assert_eq!(limiter.available_permits(), MAX_CONCURRENT_CLIENTS);
    assert_eq!(canceled_client.read(&mut byte).await?, 0);

    let fresh_client = UnixStream::connect(&socket_path).await?;
    let (mut fresh_server, permit) = rx.recv().await.context("reconnect")?;
    check_fragmented_fresh_frame(fresh_client, &mut fresh_server, attach, first).await?;
    drop((fresh_server, permit));
    assert_eq!(limiter.available_permits(), MAX_CONCURRENT_CLIENTS);
    Ok(())
}

async fn check_fragmented_fresh_frame(
    mut fresh_client: UnixStream,
    fresh_server: &mut UnixStream,
    attach: bool,
    first: u8,
) -> Result<()> {
    let mut body = if attach {
        vec![b'x'; 16 * 1024 + 1]
    } else {
        let mut body = br#"{"ctx":{"v":1},"msg":{"type":"status"}}"#.to_vec();
        body.resize(16 * 1024 + 1, b' ');
        body
    };
    let prefix = u32::try_from(body.len())?.to_be_bytes();
    let writer = tokio::spawn(async move {
        let suffix = if attach { &prefix[..] } else { &prefix[1..] };
        fresh_client.write_all(&suffix[..1]).await?;
        tokio::time::sleep(Duration::from_millis(10)).await;
        fresh_client.write_all(&suffix[1..]).await?;
        let final_byte = body.pop().context("last payload byte")?;
        fresh_client.write_all(&body).await?;
        tokio::time::sleep(Duration::from_millis(10)).await;
        fresh_client.write_all(&[final_byte]).await?;
        Ok::<_, anyhow::Error>(())
    });
    if attach {
        let decoded = crate::protocol::attach::read_client_frame(fresh_server, first)
            .await?
            .context("fresh attach frame")?;
        assert!(
            matches!(decoded, crate::protocol::attach::ClientFrame::Input(bytes) if bytes == vec![b'x'; 16 * 1024 + 1])
        );
    } else {
        assert!(matches!(
            read_control_msg(fresh_server, prefix[0]).await?.msg,
            ClientMsg::Status
        ));
    }
    writer.await??;
    Ok(())
}

const SOCKET_WIRE_CHILD: &str = "JACKIN_SOCKET_WIRE_CHILD";
const SOCKET_WIRE_TEST: &str =
    "socket::tests::conformance_wire_real_listener_has_bounded_private_open_and_close";

fn dispatch_socket_wire_child() -> Result<bool> {
    if std::env::var_os(SOCKET_WIRE_CHILD).is_some() {
        return Ok(false);
    }
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", SOCKET_WIRE_TEST, "--nocapture"])
        .env(SOCKET_WIRE_CHILD, "1")
        .status()?;
    anyhow::ensure!(status.success(), "isolated socket wire test failed");
    Ok(true)
}

#[tokio::test]
async fn read_control_msg_rejects_oversize_length_prefix() {
    // Length prefix claims 5 MiB (> 4 MiB cap). Reader must bail
    // rather than allocate the buffer.
    let (mut a, mut b) = UnixStream::pair().unwrap();
    // Length = 5 MiB, as a 4-byte BE u32 split across `first_byte`
    // (0x00) + the 3-byte suffix `read_control_msg` reads itself.
    let len_bytes = (5u32 * 1024 * 1024).to_be_bytes();
    a.write_all(&len_bytes[1..]).await.unwrap();
    a.shutdown().await.unwrap();
    let result = read_control_msg(&mut b, len_bytes[0]).await;
    result.expect_err("expected oversize rejection");
}

#[tokio::test]
async fn read_control_msg_rejects_malformed_json() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let body = b"{not valid json";
    let len_buf = u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes();
    a.write_all(&len_buf[1..]).await.unwrap();
    a.write_all(body).await.unwrap();
    a.shutdown().await.unwrap();
    let result = read_control_msg(&mut b, len_buf[0]).await;
    result.expect_err("expected JSON parse error");
}

#[tokio::test]
async fn read_control_msg_decodes_known_request() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let body = br#"{"ctx":{"v":1},"msg":{"type":"status"}}"#;
    let len_buf = u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes();
    a.write_all(&len_buf[1..]).await.unwrap();
    a.write_all(body).await.unwrap();
    a.shutdown().await.unwrap();
    let msg = read_control_msg(&mut b, len_buf[0]).await.unwrap();
    assert!(matches!(msg.msg, ClientMsg::Status));
}

#[tokio::test]
async fn read_control_msg_decodes_unknown_variant_for_forward_compat() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let body = br#"{"ctx":{"v":1},"msg":{"type":"future_query"}}"#;
    let len_buf = u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes();
    a.write_all(&len_buf[1..]).await.unwrap();
    a.write_all(body).await.unwrap();
    a.shutdown().await.unwrap();
    let msg = read_control_msg(&mut b, len_buf[0]).await.unwrap();
    assert!(matches!(msg.msg, ClientMsg::Unknown));
}

#[tokio::test]
async fn start_listener_caps_concurrent_clients_at_max() {
    // Hard regression guard for `MAX_CONCURRENT_CLIENTS`. Without
    // the cap, any in-uid process can flood the attach channel
    // and starve the legitimate operator. The over-cap connection
    // must drop on the server side without ever landing in `rx`.
    //
    // Negative-delivery assertions go through `limiter`
    // directly (`available_permits == 0` after saturation) rather
    // than real-wall-clock `timeout()` checks against `rx.recv()`
    // — the wall-clock approach passed on loaded CI runners
    // simply because the daemon hadn't been scheduled within the
    // timeout window, masking real cap regressions. Reading the
    // semaphore is cap-sensitive instead of timing-sensitive.
    let tmp = tempfile::tempdir().expect("tempdir");
    let parent = tmp.path().join("run");
    let socket_path = parent.join("jackin.sock");
    let (mut rx, limiter) = start_listener_at_with_limiter(&socket_path).expect("bind");

    // Hold every accepted stream + permit so the semaphore stays
    // saturated. Dropping the permit would let the next accept
    // proceed and invalidate the assertion.
    //
    // Per-iteration `connect().await` then `rx.recv()` assumes
    // the unbounded mpsc preserves FIFO order of accepts — held
    // today and contract-stable across tokio versions.
    let mut held: Vec<(UnixStream, tokio::sync::OwnedSemaphorePermit)> = Vec::new();
    let mut client_streams: Vec<UnixStream> = Vec::new();
    for i in 0..MAX_CONCURRENT_CLIENTS {
        let client = UnixStream::connect(&socket_path)
            .await
            .unwrap_or_else(|e| panic!("connect {i}: {e}"));
        client_streams.push(client);
        let pair = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("rx did not deliver connection {i}"))
            .expect("rx closed");
        held.push(pair);
    }
    assert_eq!(
        limiter.available_permits(),
        0,
        "after saturating the cap, no permits should remain"
    );

    // Cap is now at MAX. The next connect should be accepted by
    // the kernel but dropped on the server side. Yield to the
    // tokio scheduler so the accept loop processes the over-cap
    // connect, then check the semaphore: it must still report 0
    // (no permit acquired) because `try_acquire_owned` failed and
    // the loop continued without delivering to `rx`.
    let over_cap_client = UnixStream::connect(&socket_path)
        .await
        .expect("kernel-side connect");
    client_streams.push(over_cap_client);
    for _ in 0..10 {
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        limiter.available_permits(),
        0,
        "over-cap connect must not consume a permit"
    );
    match rx.try_recv() {
        Err(mpsc::error::TryRecvError::Empty) => {}
        other => panic!("rx must not deliver beyond MAX_CONCURRENT_CLIENTS; got: {other:?}"),
    }

    // Releasing one permit must let a fresh attach through.
    drop(held.pop().expect("drop one held permit"));
    let new_client = UnixStream::connect(&socket_path)
        .await
        .expect("post-release connect");
    client_streams.push(new_client);
    let resumed = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("rx did not resume after permit release")
        .expect("rx closed");
    held.push(resumed);
    assert_eq!(
        limiter.available_permits(),
        0,
        "after re-saturation the cap should hold permits at 0"
    );
}

#[tokio::test]
async fn start_listener_locks_socket_and_parent_dir_to_owner_only() {
    // Hard regression guard for the socket's defense-in-depth file-mode
    // contract. Protocol peer authentication remains mandatory because
    // session processes retain CAP_DAC_OVERRIDE.
    let tmp = tempfile::tempdir().expect("tempdir");
    let parent = tmp.path().join("run");
    let socket_path = parent.join("jackin.sock");
    let _rx = start_listener_at(&socket_path).expect("bind");
    let parent_mode = std::fs::metadata(&parent)
        .expect("parent metadata")
        .permissions()
        .mode()
        & 0o777;
    let sock_mode = std::fs::metadata(&socket_path)
        .expect("socket metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        parent_mode, 0o700,
        "parent dir must be 0o700 (was {parent_mode:o})"
    );
    assert_eq!(sock_mode, 0o600, "socket must be 0o600 (was {sock_mode:o})");
}

#[test]
fn conformance_wire_real_listener_has_bounded_private_open_and_close() -> Result<()> {
    if dispatch_socket_wire_child()? {
        return Ok(());
    }
    let _telemetry_guard = crate::test_support::telemetry_test_guard();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )?;
    let directory = tempfile::tempdir()?;
    let socket_path = directory.path().join("wire-private-run/wire-private.sock");
    let receiver = {
        let _runtime = runtime.enter();
        start_listener_at(&socket_path)?
    };
    drop(receiver);
    runtime.block_on(async {
        let mut client = UnixStream::connect(&socket_path).await?;
        let mut closed = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut closed)).await??;
        anyhow::ensure!(closed.is_empty(), "listener close returned private bytes");
        Ok::<_, anyhow::Error>(())
    })?;

    let failure_parent = directory.path().join("wire-private-parent-file");
    std::fs::write(&failure_parent, "wire-private-parent-content")?;
    let failure_path = failure_parent.join("wire-private-failed.sock");
    {
        let _runtime = runtime.enter();
        let Err(_error) = start_listener_at(&failure_path) else {
            panic!("listener setup unexpectedly accepted a file as its parent directory");
        };
    }
    jackin_diagnostics::flush_wire_test_export()?;

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let spans = runtime.block_on(async {
        loop {
            let spans = testbed
                .spans()
                .into_iter()
                .filter(|span| span.name == "stream.operation")
                .collect::<Vec<_>>();
            if spans.len() == 3 {
                break spans;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "listener wire phases did not arrive exactly once: {spans:?}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
    let wire = format!("{spans:?}");
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.status.as_ref().is_some_and(|status| status.code == 2))
            .count(),
        1
    );
    for expected in ["open", "close", "success", "error", "io_error"] {
        assert!(wire.contains(expected), "missing {expected}: {wire}");
    }
    assert_eq!(
        testbed
            .log_records()
            .iter()
            .filter(|record| record.event_name == "error.typed")
            .count(),
        1
    );
    let socket_path = socket_path.to_string_lossy();
    let failure_path = failure_path.to_string_lossy();
    let prohibited = [
        socket_path.as_ref(),
        failure_path.as_ref(),
        "wire-private-run",
        "wire-private.sock",
        "wire-private-parent-file",
        "wire-private-parent-content",
        "wire-private-failed.sock",
    ];
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}
