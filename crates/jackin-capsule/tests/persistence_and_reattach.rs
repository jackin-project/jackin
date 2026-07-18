// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// End-to-end persistence and reattach contracts exercised over a real
/// Unix socket without spawning a PTY.
///
/// These tests start the socket listener, simulate a client connecting,
/// send a `Hello` frame, and verify the daemon-side message path. They
/// do not boot the full `run_daemon` event loop (which requires PID 1
/// behaviour) but cover the parts that the rewrite touches: control vs
/// attach dispatch by first byte, control-channel JSON shape, and
/// attach-channel binary roundtrip.
use jackin_capsule::protocol::attach::{
    ClientFrame, ClientTerminal, ServerFrame, encode_client, encode_server, read_client_frame,
    read_server_frame,
};
use jackin_capsule::protocol::control::{ClientMsg, ControlRequest, ServerMsg, frame};
use jackin_protocol::TelemetryContext;
use std::path::PathBuf;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

fn socket_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("test.sock")
}

#[tokio::test]
async fn attach_hello_roundtrips_over_socket() {
    let dir = tempdir().unwrap();
    let sock = socket_path(&dir);
    let listener = UnixListener::bind(&sock).unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut first = [0u8; 1];
        stream.read_exact(&mut first).await.unwrap();
        // First byte of an attach Hello must be a non-zero tag.
        assert_ne!(first[0], 0x00);
        let frame = read_client_frame(&mut stream, first[0])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            frame,
            ClientFrame::Hello {
                rows: 24,
                cols: 80,
                spawn: None,
                env: Vec::new(),
                terminal: ClientTerminal::default(),
                focus_session: None,
                context: None,
            }
        );
        // Server replies with Welcome + a fake Output payload.
        stream
            .write_all(&encode_server(ServerFrame::Welcome { session_count: 1 }))
            .await
            .unwrap();
        stream
            .write_all(&encode_server(ServerFrame::Output(b"hi".to_vec())))
            .await
            .unwrap();
    });

    let mut client = UnixStream::connect(&sock).await.unwrap();
    let hello = encode_client(ClientFrame::Hello {
        rows: 24,
        cols: 80,
        spawn: None,
        env: Vec::new(),
        terminal: ClientTerminal::default(),
        focus_session: None,
        context: None,
    })
    .expect("encode Hello");
    client.write_all(&hello).await.unwrap();

    // Read two server frames and assert payloads.
    let mut tag = [0u8; 1];
    client.read_exact(&mut tag).await.unwrap();
    let f1 = read_server_frame(&mut client, tag[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(f1, ServerFrame::Welcome { session_count: 1 });

    client.read_exact(&mut tag).await.unwrap();
    let f2 = read_server_frame(&mut client, tag[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(f2, ServerFrame::Output(b"hi".to_vec()));

    server.await.unwrap();
}

#[tokio::test]
async fn control_channel_status_roundtrip() {
    let dir = tempdir().unwrap();
    let sock = socket_path(&dir);
    let listener = UnixListener::bind(&sock).unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut first = [0u8; 1];
        stream.read_exact(&mut first).await.unwrap();
        // Control channel always starts with `0x00` (high byte of the
        // 4-byte length prefix for messages under 16 MiB).
        assert_eq!(first[0], 0x00);
        // Read the rest of the length and the body.
        let mut rest = [0u8; 3];
        stream.read_exact(&mut rest).await.unwrap();
        let len = u32::from_be_bytes([first[0], rest[0], rest[1], rest[2]]) as usize;
        let mut body = vec![0u8; len];
        stream.read_exact(&mut body).await.unwrap();
        let req: ControlRequest = serde_json::from_slice(&body).unwrap();
        assert_eq!(req.ctx.v, 1);
        assert!(matches!(req.msg, ClientMsg::Status));
        // Reply with an empty session list.
        let reply = ServerMsg::SessionList { sessions: vec![] };
        stream.write_all(&frame(&reply)).await.unwrap();
    });

    let mut client = UnixStream::connect(&sock).await.unwrap();
    client
        .write_all(&frame(&ControlRequest {
            ctx: TelemetryContext::v1(),
            msg: ClientMsg::Status,
        }))
        .await
        .unwrap();

    let mut len_buf = [0u8; 4];
    client.read_exact(&mut len_buf).await.unwrap();
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    client.read_exact(&mut body).await.unwrap();
    let reply: ServerMsg = serde_json::from_slice(&body).unwrap();
    let ServerMsg::SessionList { sessions } = reply else {
        panic!("Status reply must be SessionList, got {reply:?}");
    };
    assert!(sessions.is_empty());

    server.await.unwrap();
}

#[tokio::test]
async fn second_attach_takes_over_first() {
    // Models the takeover model: when a second client connects, the
    // first should receive Shutdown. The daemon enforces this with the
    // `attached_out.take()` step in the accept handler.
    let dir = tempdir().unwrap();
    let sock = socket_path(&dir);
    let listener = UnixListener::bind(&sock).unwrap();

    let server = tokio::spawn(async move {
        let mut first_stream: Option<UnixStream> = None;
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            // Drop the first stream by writing Shutdown to it when the
            // second arrives — mimicking the daemon's takeover path.
            if let Some(mut prev) = first_stream.take() {
                prev.write_all(&encode_server(ServerFrame::Shutdown { reason: None }))
                    .await
                    .unwrap();
            }
            // Consume the Hello from the new client.
            let mut t = [0u8; 1];
            stream.read_exact(&mut t).await.unwrap();
            drop(read_client_frame(&mut stream, t[0]).await.unwrap());
            first_stream = Some(stream);
        }
    });

    let hello = || {
        encode_client(ClientFrame::Hello {
            rows: 24,
            cols: 80,
            spawn: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            focus_session: None,
            context: None,
        })
        .expect("encode Hello")
    };
    let mut client_a = UnixStream::connect(&sock).await.unwrap();
    client_a.write_all(&hello()).await.unwrap();

    let mut client_b = UnixStream::connect(&sock).await.unwrap();
    client_b.write_all(&hello()).await.unwrap();

    // Client A should receive a Shutdown frame. Cap the wait so a
    // scheduler-ordering deadlock fails the test deterministically
    // instead of hanging CI.
    let mut tag = [0u8; 1];
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client_a.read_exact(&mut tag),
    )
    .await
    .expect("client A did not receive Shutdown within 5s")
    .unwrap();
    let f = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        read_server_frame(&mut client_a, tag[0]),
    )
    .await
    .expect("decoding Shutdown frame timed out")
    .unwrap()
    .unwrap();
    assert_eq!(f, ServerFrame::Shutdown { reason: None });

    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .expect("server task did not complete within 5s")
        .unwrap();
}
