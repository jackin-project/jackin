// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn managed_probe_boundaries_export_fixed_private_shapes() {
    use std::sync::mpsc;

    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        let codex = ChildOperation::begin("codex");
        codex.spawn_failed();
        let grok = ChildOperation::begin("/private/bin/grok");
        grok.io_failed();

        let (codex_tx, codex_rx) = mpsc::channel();
        codex_tx
            .send(
                serde_json::json!({
                    "id": 1,
                    "result": {"private_response": "codex-secret"}
                })
                .to_string(),
            )
            .unwrap();
        let mut codex_wire = Vec::new();
        codex_rpc_request(
            &mut codex_wire,
            &codex_rx,
            1,
            "account/rateLimits/read",
            serde_json::json!({"private_request": "codex-secret"}),
            Duration::from_secs(1),
        )
        .unwrap();
        codex_rpc_notification(&mut codex_wire, "initialized").unwrap();

        let (grok_tx, grok_rx) = mpsc::channel();
        grok_tx
            .send(
                serde_json::json!({
                    "id": 2,
                    "error": {"message": "grok-private-error"}
                })
                .to_string(),
            )
            .unwrap();
        let mut grok_wire = Vec::new();
        grok_rpc_request(
            &mut grok_wire,
            &grok_rx,
            2,
            "x.ai/billing",
            serde_json::json!({"private_request": "grok-secret"}),
            Duration::from_secs(1),
        )
        .unwrap_err();

        let (_timeout_tx, timeout_rx) = mpsc::channel();
        codex_rpc_request(
            &mut Vec::new(),
            &timeout_rx,
            3,
            "account/read",
            serde_json::json!({}),
            Duration::from_millis(1),
        )
        .unwrap_err();
    });
    export.force_flush();

    let spans = export.finished_spans();
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == jackin_telemetry::schema::spans::PROCESS_COMMAND)
            .count(),
        2
    );
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == jackin_telemetry::schema::spans::RPC_CLIENT)
            .count(),
        4
    );
    for expected in [
        "codex",
        "grok",
        "codex.app-server",
        "grok.acp",
        "account/rateLimits/read",
        "account/read",
        "initialized",
        "x.ai/billing",
        "process_spawn_error",
        "io_error",
        "rpc_error",
        "timeout",
    ] {
        assert!(export.contains_span_text(expected), "missing {expected}");
    }
    for prohibited in [
        "/private/bin/grok",
        "private_request",
        "private_response",
        "codex-secret",
        "grok-secret",
        "grok-private-error",
    ] {
        assert!(!export.contains_span_text(prohibited));
        assert!(!export.contains_log_text(prohibited));
    }
}
