// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jackin_openshell::{
    Gateway, GatewayError, GatewayHealth, GatewayOp, GrpcGateway, resolve_gateway,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::fakes::{ScriptedRunner, exited};
use super::harness::{
    GATEWAY, WORKSPACE, config, connect, env, guard, list_json, name, resolved, wait_until,
};
use super::pki::{KEY_SENTINEL, Pki};
use super::server::start;

#[tokio::test]
async fn mtls_connect_health_and_debug_hygiene() {
    let env = env().await;
    let resolved = resolved(&env).await;
    let resolved_debug = format!("{resolved:?}");
    let gateway = connect(&env, WORKSPACE).await;
    let health = guard(gateway.health()).await.unwrap();
    assert_eq!(
        health,
        GatewayHealth {
            version: "0.1.7".to_owned(),
            serving: true,
        }
    );
    assert_eq!(gateway.name().as_str(), GATEWAY);
    let gateway_debug = format!("{gateway:?}");
    for window in KEY_SENTINEL.as_bytes().windows(3) {
        let window = std::str::from_utf8(window).unwrap();
        assert!(!resolved_debug.contains(window), "{window}");
        assert!(!gateway_debug.contains(window), "{window}");
    }
}

#[tokio::test]
async fn unreachable_and_untrusted_servers_map_to_unreachable_and_tls() {
    let env = env().await;
    let closed_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let closed = format!("https://127.0.0.1:{closed_port}");
    let runner = ScriptedRunner::always(exited(0, list_json(&closed).as_bytes(), b""));
    let gateway = resolve_gateway(
        &runner,
        env.fs.as_ref(),
        &env.home,
        &name(GATEWAY),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let error = guard(GrpcGateway::connect(config(
        &env,
        gateway,
        WORKSPACE,
        runner.clone(),
    )))
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        GatewayError::Unreachable { ref endpoint, .. } if *endpoint == closed
    ));

    let foreign = Pki::new();
    let server = start(&foreign, &foreign.ca_pem).await;
    let runner = ScriptedRunner::always(exited(0, list_json(&server.endpoint()).as_bytes(), b""));
    let gateway = resolve_gateway(
        &runner,
        env.fs.as_ref(),
        &env.home,
        &name(GATEWAY),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let error = guard(GrpcGateway::connect(config(
        &env,
        gateway,
        WORKSPACE,
        runner.clone(),
    )))
    .await
    .unwrap_err();
    assert!(matches!(error, GatewayError::Tls { .. }), "{error:?}");
    assert_eq!(server.state().rpc_count, 0);
}

struct Silent {
    endpoint: String,
    accepted: Arc<AtomicUsize>,
}

async fn silent_listener() -> Silent {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "https://127.0.0.1:{}",
        listener.local_addr().unwrap().port()
    );
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&accepted);
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
            count.fetch_add(1, Ordering::SeqCst);
        }
    });
    Silent { endpoint, accepted }
}

async fn silent_config(
    env: &super::harness::Env,
    silent: &Silent,
    cancel: &CancellationToken,
) -> jackin_openshell::GrpcGatewayConfig {
    let runner = ScriptedRunner::always(exited(0, list_json(&silent.endpoint).as_bytes(), b""));
    let gateway = resolve_gateway(
        &runner,
        env.fs.as_ref(),
        &env.home,
        &name(GATEWAY),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let mut config = config(env, gateway, WORKSPACE, runner);
    config.cancel = cancel.clone();
    config
}

#[tokio::test]
async fn cancelling_while_the_tls_handshake_waits_ends_connect_with_cancelled() {
    let env = env().await;
    let silent = silent_listener().await;
    let cancel = CancellationToken::new();
    let connecting = tokio::spawn(GrpcGateway::connect(
        silent_config(&env, &silent, &cancel).await,
    ));
    wait_until(|| silent.accepted.load(Ordering::SeqCst) >= 2).await;
    assert!(!connecting.is_finished(), "connect ended before the cancel");
    cancel.cancel();
    let error = guard(connecting).await.unwrap().unwrap_err();
    assert!(
        matches!(
            error,
            GatewayError::Cancelled {
                op: GatewayOp::Health,
                group: None
            }
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_token_cancelled_before_connect_opens_no_connection() {
    let env = env().await;
    let silent = silent_listener().await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = guard(GrpcGateway::connect(
        silent_config(&env, &silent, &cancel).await,
    ))
    .await
    .unwrap_err();
    assert!(matches!(error, GatewayError::Cancelled { .. }), "{error:?}");
    assert_eq!(silent.accepted.load(Ordering::SeqCst), 0);
}
