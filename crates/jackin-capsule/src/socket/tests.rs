// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `socket`.
use super::*;
use crate::protocol::control::ClientMsg;

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
    let len_buf = (body.len() as u32).to_be_bytes();
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
    let len_buf = (body.len() as u32).to_be_bytes();
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
    let len_buf = (body.len() as u32).to_be_bytes();
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
    // Hard regression guard for the file-mode security contract
    // documented at `start_listener_at`. Any refactor that drops
    // either chmod silently exposes the attach channel to any
    // in-container uid sharing the agent uid — the exact threat
    // the comments name.
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
