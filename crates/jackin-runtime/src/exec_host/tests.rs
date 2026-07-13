//! Tests for `exec_host`.
use super::*;

#[test]
fn validate_op_source_accepts_well_formed_ref() {
    validate_op_source("op://vault/item/field").unwrap();
}

#[test]
fn validate_op_source_rejects_non_op_scheme() {
    validate_op_source("https://evil/x").unwrap_err();
    validate_op_source("vault/item/field").unwrap_err();
}

#[test]
fn validate_op_source_rejects_flag_segments() {
    // A path segment that looks like a CLI flag could inject arguments into
    // `op read` — must be rejected before the subprocess is spawned.
    validate_op_source("op://vault/-rf/field").unwrap_err();
    validate_op_source("op://-vault/item").unwrap_err();
}

/// Drive `handle_connection` over an in-memory socket pair and return the
/// decoded JSON reply (`{"values":…}` or `{"error":…}`).
async fn roundtrip(
    allowed: Vec<ExecBinding>,
    request_refs: serde_json::Value,
) -> serde_json::Value {
    #[cfg(target_os = "linux")]
    let caller_auth = CallerAuth::PeerPid(std::process::id());
    #[cfg(not(target_os = "linux"))]
    let caller_auth = CallerAuth::CapsuleDaemon;

    roundtrip_with_auth(allowed, request_refs, caller_auth)
        .await
        .expect("authenticated roundtrip should return a reply")
}

async fn roundtrip_with_auth(
    allowed: Vec<ExecBinding>,
    request_refs: serde_json::Value,
    caller_auth: CallerAuth,
) -> Option<serde_json::Value> {
    let (mut client, server) = UnixStream::pair().unwrap();
    let server_task =
        tokio::spawn(async move { handle_connection(server, &allowed, caller_auth).await });

    let body = serde_json::to_vec(&serde_json::json!({ "refs": request_refs })).unwrap();
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

#[tokio::test]
async fn approved_literal_ref_resolves() {
    let allowed = vec![ExecBinding {
        name: "TOKEN".into(),
        kind: ExecKind::Literal,
        source: "s3cr3t".into(),
    }];
    let reply = roundtrip(
        allowed,
        serde_json::json!([{ "name": "TOKEN", "kind": "literal", "source": "s3cr3t" }]),
    )
    .await;
    assert_eq!(reply["values"]["TOKEN"], "s3cr3t");
    assert!(reply.get("error").is_none());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn unauthenticated_peer_is_rejected_before_resolution() {
    let allowed = vec![ExecBinding {
        name: "TOKEN".into(),
        kind: ExecKind::Literal,
        source: "s3cr3t".into(),
    }];
    let reply = roundtrip_with_auth(
        allowed,
        serde_json::json!([{ "name": "TOKEN", "kind": "literal", "source": "s3cr3t" }]),
        CallerAuth::PeerPid(std::process::id().saturating_add(1)),
    )
    .await;

    assert!(reply.is_none(), "unauthenticated peer must be closed");
}

#[cfg(target_os = "linux")]
#[test]
fn container_init_peer_status_requires_innermost_nspid_one() {
    assert!(peer_is_container_init_process_status(
        "Name:\tjackin-capsule\nNSpid:\t424242\t1\n"
    ));
    assert!(!peer_is_container_init_process_status(
        "Name:\tagent\nNSpid:\t424243\t37\n"
    ));
    assert!(!peer_is_container_init_process_status("Name:\tno-nspid\n"));
}

#[tokio::test]
async fn unapproved_source_is_rejected() {
    // Same name + kind, but a `source` the operator never approved. A
    // name-only match would let a compromised container swap the source to
    // read a different secret — the allow-list must reject this.
    let allowed = vec![ExecBinding {
        name: "TOKEN".into(),
        kind: ExecKind::Literal,
        source: "approved".into(),
    }];
    let reply = roundtrip(
        allowed,
        serde_json::json!([{ "name": "TOKEN", "kind": "literal", "source": "attacker-swapped" }]),
    )
    .await;
    assert!(reply.get("values").is_none());
    assert!(
        reply["error"]
            .as_str()
            .unwrap()
            .contains("not in the approved binding set")
    );
}

#[tokio::test]
async fn approved_env_ref_resolves_from_host_env() {
    // Use an existing var (PATH is always set) — the crate forbids `unsafe`, so
    // `std::env::set_var` is unavailable.
    let expected = std::env::var("PATH").expect("PATH is set in the test env");
    let allowed = vec![ExecBinding {
        name: "X".into(),
        kind: ExecKind::Env,
        source: "$PATH".into(),
    }];
    let reply = roundtrip(
        allowed,
        serde_json::json!([{ "name": "X", "kind": "env", "source": "$PATH" }]),
    )
    .await;
    assert_eq!(reply["values"]["X"], expected);
}

#[tokio::test]
async fn unknown_name_is_rejected() {
    let allowed = vec![ExecBinding {
        name: "TOKEN".into(),
        kind: ExecKind::Literal,
        source: "x".into(),
    }];
    let reply = roundtrip(
        allowed,
        serde_json::json!([{ "name": "OTHER", "kind": "literal", "source": "x" }]),
    )
    .await;
    assert!(reply.get("values").is_none());
    assert!(reply.get("error").is_some());
}
