// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn test_caller_auth() -> CallerAuth {
    #[cfg(target_os = "linux")]
    {
        CallerAuth::PeerPid(std::process::id())
    }
    #[cfg(not(target_os = "linux"))]
    {
        CallerAuth::TestPeer
    }
}

pub(super) async fn roundtrip(
    allowed: Vec<ExecBinding>,
    request_refs: serde_json::Value,
) -> serde_json::Value {
    let caller_auth = test_caller_auth();

    roundtrip_with_auth(allowed, request_refs, caller_auth)
        .await
        .expect("authenticated roundtrip should return a reply")
}

pub(super) async fn roundtrip_with_auth(
    allowed: Vec<ExecBinding>,
    request_refs: serde_json::Value,
    caller_auth: CallerAuth,
) -> Option<serde_json::Value> {
    let (mut client, server) = UnixStream::pair().unwrap();
    let server_task =
        tokio::spawn(async move { handle_connection(server, &allowed, caller_auth).await });

    let body = serde_json::to_vec(&serde_json::json!({
        "ctx": { "v": jackin_telemetry::propagation::VERSION },
        "refs": request_refs,
    }))
    .unwrap();
    if client
        .write_all(&(body.len() as u32).to_be_bytes())
        .await
        .is_err()
        || client.write_all(&body).await.is_err()
    {
        server_task.await.unwrap().unwrap();
        return None;
    }

    let mut len_buf = [0u8; 4];
    if client.read_exact(&mut len_buf).await.is_err() {
        server_task.await.unwrap().unwrap();
        return None;
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut reply = vec![0u8; len];
    client.read_exact(&mut reply).await.unwrap();

    server_task.await.unwrap().unwrap();
    Some(serde_json::from_slice(&reply).unwrap())
}

pub(super) async fn exported_exec_roundtrip(
    context: jackin_protocol::TelemetryContext,
) -> (
    serde_json::Value,
    Vec<jackin_diagnostics::TestSpanSnapshot>,
    usize,
) {
    let caller_auth = test_caller_auth();
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let (mut client, server) = UnixStream::pair().expect("host socket pair");
    client
        .write_all(&frame(&CredRequest {
            ctx: context,
            refs: Vec::new(),
        }))
        .await
        .expect("write credential request");
    handle_connection(server, &[], caller_auth)
        .await
        .expect("handle credential request");
    let mut len = [0_u8; 4];
    client
        .read_exact(&mut len)
        .await
        .expect("read reply length");
    let mut body = vec![0_u8; u32::from_be_bytes(len) as usize];
    client.read_exact(&mut body).await.expect("read reply body");
    drop(guard);
    export.force_flush();
    let reply = serde_json::from_slice(&body).expect("decode credential reply");
    let spans = export.finished_spans();
    let errors = export.typed_error_count("error.typed", "rpc_error");
    (reply, spans, errors)
}
