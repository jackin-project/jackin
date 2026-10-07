// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_wire_provider_boundary_exports_bounded_private_shapes() {
    if std::env::var_os("JACKIN_USAGE_WIRE_USAGE_CHILD").is_none() {
        let status = Command::new(
            std::env::current_exe().expect("usage test executable must resolve"),
        )
        .arg("--exact")
        .arg("usage::tests::case_12::conformance_wire_provider_boundary_exports_bounded_private_shapes")
        .arg("--nocapture")
        .env("JACKIN_USAGE_WIRE_USAGE_CHILD", "1")
        .status()
        .expect("isolated wire usage test must start");
        assert!(status.success(), "isolated wire usage test failed");
        return;
    }
    let testbed = jackin_otlp_testbed::Testbed::start().expect("start OTLP testbed");
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )
    .expect("initialize wire test export");

    let success = provider_request(
        jackin_telemetry::schema::enums::ProviderName::Openai,
        "GET",
        "/backend-api/wham/usage",
        || Ok::<_, String>("private-provider-response"),
    );
    assert_eq!(
        success.expect("provider request succeeds"),
        "private-provider-response"
    );
    let failure = provider_request(
        jackin_telemetry::schema::enums::ProviderName::Anthropic,
        "POST",
        "/api/oauth/usage",
        || Err::<(), _>("private-token private-account ?private=query".to_owned()),
    );
    assert!(failure.is_err());
    jackin_diagnostics::flush_wire_test_export().expect("flush wire test export");

    let deadline = Instant::now() + Duration::from_secs(2);
    let spans = loop {
        let spans = testbed
            .spans()
            .into_iter()
            .filter(|span| span.name == "http.client")
            .collect::<Vec<_>>();
        if spans.len() == 2 {
            break spans;
        }
        assert!(
            Instant::now() < deadline,
            "provider HTTP wire spans did not arrive"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let wire_text = format!("{spans:?}");
    for expected in [
        "openai",
        "anthropic",
        "GET",
        "POST",
        "/backend-api/wham/usage",
        "/api/oauth/usage",
        "success",
        "failure",
        "http_error",
    ] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    let prohibited = [
        "private-provider-response",
        "private-token",
        "private-account",
        "?private=query",
    ];
    for value in prohibited {
        assert!(!wire_text.contains(value), "exported {value}");
    }
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
}

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
