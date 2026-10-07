// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn claude_code_user_agent_parses_cli_version() {
    assert_eq!(
        claude_code_version_from_text("Claude Code 2.1.7\n").as_deref(),
        Some("2.1.7")
    );
    assert_eq!(
        claude_code_user_agent_with(|command, args, timeout| {
            assert_eq!(command, "claude");
            assert_eq!(args, ["--version"]);
            assert_eq!(timeout, CLAUDE_VERSION_TIMEOUT);
            Ok(CliOutput {
                success: true,
                exit_code: Some(0),
                stdout: "Claude Code 2.2.0".to_owned(),
                stderr: String::new(),
            })
        })
        .as_deref(),
        Some("claude-code/2.2.0")
    );
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

#[test]
fn claude_keychain_missing_with_no_credential_is_local_missing() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let resolution = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_| ClaudeKeychainRead::Missing,
        empty_file_probe,
        || None,
    );
    assert!(matches!(resolution, ClaudeWaveResolution::Missing));
    assert_eq!(
        claude_wave_policy(&resolution),
        ClaudeWavePolicy::LocalMissing
    );
}

#[test]
fn claude_keychain_metadata_makes_resolution_shared() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let resolution = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_| ClaudeKeychainRead::Payload {
            json: r#"{"claudeAiOauth":{"accessToken":"kc"}}"#.to_owned(),
        },
        || ClaudeFileProbe {
            credential: None,
            origin: None,
            account_email: Some("id@example.com".to_owned()),
            organization_type: Some("Max".to_owned()),
        },
        || None,
    );
    match &resolution {
        ClaudeWaveResolution::Resolved(r) => {
            assert!(!r.is_anonymous);
            assert_eq!(r.account_email.as_deref(), Some("id@example.com"));
        }
        _ => panic!("resolved"),
    }
    assert_eq!(claude_wave_policy(&resolution), ClaudeWavePolicy::Shared);
}

#[test]
fn claude_denied_view_has_no_quota_and_exact_error() {
    let view = claude_view_from_wave_with_rate_limit(
        "claude",
        Some("Anthropic / Claude"),
        1_781_185_560,
        ClaudeWaveResolution::Denied,
    )
    .0;
    assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
    assert!(view.buckets.is_empty());
    assert!(view.account.account_label.is_empty());
    assert_eq!(view.account.plan_label, None);
    assert_eq!(view.account.credential_origin, None);
    assert_eq!(
        view.last_error.as_deref(),
        Some("Claude Keychain access denied")
    );
}

#[test]
fn claude_limits_inactive_flag_does_not_gate_rendering() {
    // Live responses send `is_active: false` on headline limits that still
    // carry quota — the flag must never suppress a bucket.
    let response: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": null,
        "seven_day": null,
        "limits": [
            {"kind": "session", "percent": 10, "is_active": false,
             "resets_at": "2026-09-17T10:00:00Z"},
            {"kind": "weekly_all", "percent": 42, "is_active": false,
             "resets_at": "2026-09-24T10:00:00Z"},
        ]
    }))
    .expect("inactive limits decode");
    let buckets = response.into_buckets(1_781_185_560);
    let session = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Session))
        .expect("session bucket despite is_active false");
    assert_eq!(session.label, "Session");
    assert_eq!(session.remaining_percent, Some(90));
    let weekly = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Weekly))
        .expect("weekly bucket despite is_active false");
    assert_eq!(weekly.label, "All models");
    assert_eq!(weekly.remaining_percent, Some(58));
}

#[test]
fn claude_scope_restriction_error_is_explicit() {
    let forbidden = ProviderError::from(ProviderHttpError::HttpStatus {
        status: 403,
        message: "Claude OAuth usage HTTP 403 Forbidden".to_owned(),
        retry_after_seconds: None,
        response_received_at_epoch: None,
    });
    assert!(claude_error_is_scope_restriction(&forbidden));
    assert!(!claude_error_is_scope_restriction(&ProviderError::from(
        ProviderHttpError::Transport("HTTP 403 insufficient_scope".to_owned()),
    )));
    assert!(!claude_error_is_scope_restriction(&ProviderError::from(
        ProviderHttpError::HttpStatus {
            status: 401,
            message: "Claude OAuth usage HTTP 401 Unauthorized".to_owned(),
            retry_after_seconds: None,
            response_received_at_epoch: None,
        },
    )));
    assert!(!claude_error_is_scope_restriction(&ProviderError::from(
        "Claude OAuth usage request failed: connection reset".to_owned(),
    )));
    assert_eq!(
        claude_provider_error_label(
            Some(&forbidden),
            Some(&ProviderError::from("cli boom".to_owned()))
        )
        .as_deref(),
        Some("Claude token lacks usage scope (inference-only); quota unavailable")
    );
    // Non-scope errors pass through verbatim, OAuth first.
    assert_eq!(
        claude_provider_error_label(
            Some(&ProviderError::from("oauth boom".to_owned())),
            Some(&ProviderError::from("cli boom".to_owned())),
        )
        .as_deref(),
        Some("oauth boom")
    );
    assert_eq!(
        claude_provider_error_label(None, Some(&ProviderError::from("cli boom".to_owned())))
            .as_deref(),
        Some("cli boom")
    );
    assert_eq!(claude_provider_error_label(None, None), None);
}
