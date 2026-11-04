// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::{Gateway, GatewayError, GatewayHealth, GrpcGateway, resolve_gateway};
use tokio_util::sync::CancellationToken;

use super::fakes::{ScriptedRunner, exited};
use super::harness::{GATEWAY, WORKSPACE, config, connect, env, guard, list_json, name, resolved};
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
