// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `socket`.
use super::*;

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
    assert!(result.is_err(), "expected oversize rejection: {result:?}");
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
    assert!(result.is_err(), "expected JSON parse error: {result:?}");
}

#[tokio::test]
async fn read_control_msg_decodes_known_request() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let body = br#"{"type":"status"}"#;
    let len_buf = (body.len() as u32).to_be_bytes();
    a.write_all(&len_buf[1..]).await.unwrap();
    a.write_all(body).await.unwrap();
    a.shutdown().await.unwrap();
    let msg = read_control_msg(&mut b, len_buf[0]).await.unwrap();
    assert!(matches!(msg, ClientMsg::Status));
}

#[tokio::test]
async fn read_control_msg_decodes_unknown_variant_for_forward_compat() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let body = br#"{"type":"future_query"}"#;
    let len_buf = (body.len() as u32).to_be_bytes();
    a.write_all(&len_buf[1..]).await.unwrap();
    a.write_all(body).await.unwrap();
    a.shutdown().await.unwrap();
    let msg = read_control_msg(&mut b, len_buf[0]).await.unwrap();
    assert!(matches!(msg, ClientMsg::Unknown));
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
