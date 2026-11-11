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
    let authority = env.server.endpoint();
    let registered = format!("{authority}/path-secret");
    let runner = ScriptedRunner::always(exited(0, list_json(&registered).as_bytes(), b""));
    let resolved_gateway = resolve_gateway(
        &runner,
        env.fs.as_ref(),
        &env.home,
        &name(GATEWAY),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let gateway_config = config(
        &env,
        resolved_gateway.clone(),
        WORKSPACE,
        runner.clone(),
    );
    let config_debug = format!("{gateway_config:?}");
    assert!(config_debug.contains(&authority), "{config_debug}");
    assert!(!config_debug.contains("path-secret"), "{config_debug}");
    let gateway = guard(GrpcGateway::connect(gateway_config)).await.unwrap();
    let gateway_debug = format!("{gateway:?}");
    assert!(gateway_debug.contains(&authority), "{gateway_debug}");
    assert!(!gateway_debug.contains("path-secret"), "{gateway_debug}");
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

#[tokio::test]
async fn unreachable_names_only_the_endpoint_authority() {
    let env = env().await;
    let closed_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let registered = format!("https://127.0.0.1:{closed_port}/path-secret");
    let runner = ScriptedRunner::always(exited(0, list_json(&registered).as_bytes(), b""));
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
    assert!(
        matches!(
            &error,
            GatewayError::Unreachable { endpoint, .. }
                if *endpoint == format!("https://127.0.0.1:{closed_port}")
        ),
        "{error:?}"
    );
    assert!(!error.to_string().contains("path-secret"), "{error}");
}

#[tokio::test]
async fn a_malformed_endpoint_refuses_to_connect_and_never_carries_its_secret() {
    let env = env().await;
    let gateway = guard(GrpcGateway::connect(config(
        &env,
        resolved(&env).await,
        WORKSPACE,
        env.runner.clone(),
    )))
    .await
    .unwrap();
    let health = guard(gateway.health()).await.unwrap();
    assert!(health.serving);

    let registered = "https://gw example.com:8443/path-secret";
    let runner = ScriptedRunner::always(exited(0, list_json(registered).as_bytes(), b""));
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
        runner,
    )))
    .await
    .unwrap_err();
    assert!(
        matches!(&error, GatewayError::InvalidArgument { field, message }
            if field == "endpoint" && message.contains("the host is invalid")),
        "{error:?}"
    );
    assert!(!error.is_retryable());
    let shown = error.to_string();
    assert!(!shown.contains("path-secret"), "{shown}");
    assert!(!shown.contains("gw example"), "{shown}");
}

#[tokio::test]
async fn the_resolved_gateway_debug_keeps_only_the_authority() {
    let env = env().await;
    let authority = env.server.endpoint();
    let registered = format!("{authority}/path-secret?token=query-secret");
    let runner = ScriptedRunner::always(exited(0, list_json(&registered).as_bytes(), b""));
    let resolved_gateway = resolve_gateway(
        &runner,
        env.fs.as_ref(),
        &env.home,
        &name(GATEWAY),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let debug = format!("{resolved_gateway:?}");
    assert!(debug.contains(&authority), "{debug}");
    assert!(!debug.contains("path-secret"), "{debug}");
    assert!(!debug.contains("query-secret"), "{debug}");
}

#[tokio::test]
async fn a_percent_encoded_endpoint_is_invalid_and_never_leaks() {
    let env = env().await;
    let registered = "https://secret%ZZ/path-secret";
    let runner = ScriptedRunner::always(exited(0, list_json(registered).as_bytes(), b""));
    let resolved_gateway = resolve_gateway(
        &runner,
        env.fs.as_ref(),
        &env.home,
        &name(GATEWAY),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let gateway_config = config(&env, resolved_gateway, WORKSPACE, runner.clone());
    let config_debug = format!("{gateway_config:?}");
    assert!(!config_debug.contains("secret%ZZ"), "{config_debug}");
    assert!(!config_debug.contains("path-secret"), "{config_debug}");
    let error = guard(GrpcGateway::connect(gateway_config))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, GatewayError::InvalidArgument { field, message }
            if field == "endpoint" && message.contains("the host is invalid")),
        "{error:?}"
    );
    assert!(!error.is_retryable());
    let shown = error.to_string();
    assert!(!shown.contains("secret%ZZ"), "{shown}");
    assert!(!shown.contains("path-secret"), "{shown}");
}

#[tokio::test]
async fn a_bracketed_non_ipv6_host_is_invalid_and_never_dials() {
    let env = env().await;
    let registered = "https://[127.0.0.1]/path-secret";
    let runner = ScriptedRunner::always(exited(0, list_json(registered).as_bytes(), b""));
    let resolved_gateway = resolve_gateway(
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
        resolved_gateway,
        WORKSPACE,
        runner,
    )))
    .await
    .unwrap_err();
    assert!(
        matches!(&error, GatewayError::InvalidArgument { field, message }
            if field == "endpoint" && message.contains("the host is invalid")),
        "{error:?}"
    );
    assert!(!error.is_retryable());
    let shown = error.to_string();
    assert!(!shown.contains("127.0.0.1"), "{shown}");
    assert!(!shown.contains("path-secret"), "{shown}");
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
