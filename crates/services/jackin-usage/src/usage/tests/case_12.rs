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
        let codex = process_telemetry::ChildOperation::begin("codex");
        codex.spawn_failed();
        let grok = process_telemetry::ChildOperation::begin("/private/bin/grok");
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

#[test]
fn classify_claude_keychain_status_maps_denial_and_absence() {
    assert!(matches!(
        classify_claude_keychain_status(-128),
        ClaudeKeychainRead::Denied
    ));
    assert!(matches!(
        classify_claude_keychain_status(-25293),
        ClaudeKeychainRead::Denied
    ));
    assert!(matches!(
        classify_claude_keychain_status(-25300),
        ClaudeKeychainRead::Missing
    ));
    assert!(matches!(
        classify_claude_keychain_status(-25308),
        ClaudeKeychainRead::ConsentRequired
    ));
    assert!(matches!(
        classify_claude_keychain_status(-1),
        ClaudeKeychainRead::Missing
    ));
}

#[test]
fn claude_keychain_credential_wins_over_file_paths() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let resolution = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_service| ClaudeKeychainRead::Payload {
            json: KEYCHAIN_PAYLOAD.to_owned(),
        },
        || ClaudeFileProbe {
            credential: claude_oauth_from_value(
                &serde_json::json!({"claudeAiOauth":{"accessToken":"file-token"}}),
            ),
            origin: Some("OAuth · file".to_owned()),
            account_email: Some("user@example.com".to_owned()),
            organization_type: Some("Max".to_owned()),
        },
        || Some(ClaudeOAuthEnvToken::new("env-token".to_owned())),
    );
    match resolution {
        ClaudeWaveResolution::Resolved(resolved) => {
            assert_eq!(resolved.access_token, "kc-token");
            assert_eq!(
                resolved.credential_origin,
                "OAuth · macOS Keychain (Claude Code-credentials)"
            );
            assert!(!resolved.is_anonymous);
        }
        _ => panic!("expected Resolved"),
    }
    assert_eq!(state.read_count(), 1);
}

#[test]
fn claude_keychain_denial_short_circuits_before_file_or_env_read() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let resolution = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_service| ClaudeKeychainRead::Denied,
        || panic!("file probe must not run after denial"),
        || panic!("env reader must not run after denial"),
    );
    assert!(matches!(resolution, ClaudeWaveResolution::Denied));
    // Terminal for the service: a later wave whose reader panics still returns
    // Denied from the process-lifetime cache without re-prompting.
    let again = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_service| panic!("reader must not run after cached denial"),
        || panic!("no file probe"),
        || panic!("no env"),
    );
    assert!(matches!(again, ClaudeWaveResolution::Denied));
    assert_eq!(state.read_count(), 1);
    assert_eq!(claude_wave_policy(&again), ClaudeWavePolicy::LocalDenied);
}

#[test]
fn claude_keychain_missing_falls_back_to_file_then_env() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let with_file = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_| ClaudeKeychainRead::Missing,
        || ClaudeFileProbe {
            credential: claude_oauth_from_value(
                &serde_json::json!({"claudeAiOauth":{"accessToken":"file-token","refreshToken":"rt"}}),
            ),
            origin: Some("OAuth · file".to_owned()),
            account_email: None,
            organization_type: None,
        },
        || None,
    );
    match with_file {
        ClaudeWaveResolution::Resolved(r) => assert_eq!(r.access_token, "file-token"),
        _ => panic!("file fallback"),
    }
    let state2 = ClaudeKeychainState::default();
    let with_env = resolve_claude_refresh_wave_with(
        &scope,
        &state2,
        |_| ClaudeKeychainRead::Missing,
        empty_file_probe,
        || Some(ClaudeOAuthEnvToken::new("env-token".to_owned())),
    );
    match &with_env {
        ClaudeWaveResolution::Resolved(r) => {
            assert_eq!(r.access_token, "env-token");
            assert!(r.is_anonymous);
        }
        _ => panic!("env fallback"),
    }
    assert_eq!(
        claude_wave_policy(&with_env),
        ClaudeWavePolicy::LocalAnonymous
    );
}

#[test]
fn claude_oauth_env_reader_never_reads_api_key_variables() {
    let mut requested = None;
    let token = read_claude_oauth_env_token(|name| {
        requested = Some(name.to_owned());
        match name {
            jackin_core::ANTHROPIC_API_KEY_ENV_NAME
            | jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME => {
                Ok("api-key-must-not-be-read".to_owned())
            }
            jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME => Ok("oauth-token".to_owned()),
            _ => panic!("unexpected environment variable: {name}"),
        }
    });

    assert_eq!(requested.as_deref(), Some("CLAUDE_CODE_OAUTH_TOKEN"));
    assert_eq!(
        token,
        Some(ClaudeOAuthEnvToken::new("oauth-token".to_owned()))
    );
}

#[test]
fn claude_keychain_consent_required_falls_back_like_missing() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let resolution = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_| ClaudeKeychainRead::ConsentRequired,
        || ClaudeFileProbe {
            credential: claude_oauth_from_value(
                &serde_json::json!({"claudeAiOauth":{"accessToken":"file-token"}}),
            ),
            origin: Some("OAuth · file".to_owned()),
            account_email: None,
            organization_type: None,
        },
        || None,
    );
    match resolution {
        ClaudeWaveResolution::Resolved(resolved) => {
            assert_eq!(resolved.access_token, "file-token");
        }
        _ => panic!("consent-gated Keychain must preserve file fallback"),
    }
    assert_eq!(state.read_count(), 1);
}
