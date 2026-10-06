// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn credential_file_loaders_reread_updated_container_files() {
    let dir = tempfile::tempdir().expect("tempdir");

    let claude_path = dir.path().join(".credentials.json");
    fs::write(
        &claude_path,
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "old-claude",
                "subscriptionType": "max"
            }
        })
        .to_string(),
    )
    .expect("write Claude auth");
    assert_eq!(
        load_claude_oauth_credentials(&claude_path)
            .expect("Claude credentials")
            .access_token,
        "old-claude"
    );
    fs::write(
        &claude_path,
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "new-claude",
                "subscriptionType": "max"
            }
        })
        .to_string(),
    )
    .expect("refresh Claude auth");
    assert_eq!(
        load_claude_oauth_credentials(&claude_path)
            .expect("updated Claude credentials")
            .access_token,
        "new-claude"
    );

    let codex_path = dir.path().join("auth.json");
    fs::write(
        &codex_path,
        serde_json::json!({
            "tokens": {
                "access_token": "old-codex",
                "id_token": test_jwt(serde_json::json!({"email": "old@example.com"}))
            }
        })
        .to_string(),
    )
    .expect("write Codex auth");
    assert_eq!(
        load_codex_oauth_credentials(&codex_path)
            .expect("Codex credentials")
            .access_token,
        "old-codex"
    );
    fs::write(
        &codex_path,
        serde_json::json!({
            "tokens": {
                "access_token": "new-codex",
                "id_token": test_jwt(serde_json::json!({"email": "new@example.com"}))
            }
        })
        .to_string(),
    )
    .expect("refresh Codex auth");
    let codex = load_codex_oauth_credentials(&codex_path).expect("updated Codex credentials");
    assert_eq!(codex.access_token, "new-codex");
    assert_eq!(codex.account_label.as_deref(), Some("new@example.com"));

    let kimi_path = dir.path().join(".kimi-code/credentials/kimi-code.json");
    fs::create_dir_all(kimi_path.parent().expect("Kimi credentials parent"))
        .expect("create Kimi credentials dir");
    fs::write(
        &kimi_path,
        serde_json::json!({
            "access_token": "old-kimi",
            "expires_at": 1_781_300_000
        })
        .to_string(),
    )
    .expect("write Kimi auth");
    assert_eq!(
        load_kimi_local_token_from_home(dir.path(), 1_781_200_000).as_deref(),
        Some("old-kimi")
    );
    fs::write(
        &kimi_path,
        serde_json::json!({
            "access_token": "new-kimi",
            "expires_at": 1_781_300_000
        })
        .to_string(),
    )
    .expect("refresh Kimi auth");
    assert_eq!(
        load_kimi_local_token_from_home(dir.path(), 1_781_200_000).as_deref(),
        Some("new-kimi")
    );
    fs::write(
        &kimi_path,
        serde_json::json!({
            "access_token": "expired-kimi",
            "expires_at": 1_781_100_000
        })
        .to_string(),
    )
    .expect("expire Kimi auth");
    assert_eq!(
        load_kimi_local_token_from_home(dir.path(), 1_781_200_000),
        None
    );
}

#[test]
fn quota_pace_label_uses_codexbar_reserve_deficit_onpace() {
    // Behind pace (burning faster than the clock): 60% quota left with 90%
    // of the window still remaining -> 30 points of deficit, and the linear
    // projection runs out before the reset (Variant A composite).
    let deficit = quota_pace_label(Some(60), Some(900), Some(1_000), 0).expect("pace label");
    assert_eq!(deficit, "30% in deficit · Runs out in 2m");

    // Ahead of pace (quota outlasting the clock): 90% left, 60% of window
    // remaining -> 30 points in reserve.
    let reserve = quota_pace_label(Some(90), Some(600), Some(1_000), 0).expect("pace label");
    assert_eq!(reserve, "30% in reserve");

    // Within 2 points of the clock -> On pace.
    let on_pace = quota_pace_label(Some(50), Some(500), Some(1_000), 0).expect("pace label");
    assert_eq!(on_pace, "On pace");
}

#[test]
fn reset_label_uses_relative_and_local_timestamp() {
    let now = parse_iso_epoch("2026-06-11T13:46:00Z").expect("now");
    let same_day = parse_iso_epoch("2026-06-11T15:12:00Z").expect("same day");
    assert_eq!(
        reset_label(same_day, now),
        format!(
            "Resets in 1h 26m ({})",
            format::local_timestamp_label(same_day)
        )
    );
    let tomorrow = parse_iso_epoch("2026-06-12T04:18:00Z").expect("tomorrow");
    assert_eq!(
        reset_label(tomorrow, now),
        format!(
            "Resets in 14h 32m ({})",
            format::local_timestamp_label(tomorrow)
        )
    );
    let future = parse_iso_epoch("2026-07-01T16:31:00Z").expect("future");
    assert_eq!(
        reset_label(future, now),
        format!(
            "Resets in 20d 2h ({})",
            format::local_timestamp_label(future)
        )
    );
    assert_eq!(reset_label(now, now), "Resets now");
}

#[test]
fn claude_oauth_credentials_parse_subscription_label() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "access",
                "subscriptionType": "claude_max"
            }
        })
        .to_string(),
    )
    .expect("write auth");

    let credentials = load_claude_oauth_credentials(&path).expect("credentials");

    assert_eq!(credentials.access_token, "access");
    assert_eq!(credentials.subscription_type.as_deref(), Some("Claude Max"));
}

#[test]
fn claude_oauth_credentials_fall_back_to_rate_limit_tier() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "access",
                "rateLimitTier": "max"
            }
        })
        .to_string(),
    )
    .expect("write auth");

    let credentials = load_claude_oauth_credentials(&path).expect("credentials");

    assert_eq!(credentials.access_token, "access");
    assert_eq!(credentials.subscription_type.as_deref(), Some("Max"));
}

#[test]
fn claude_organization_type_humanizes_enterprise_tier() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({
            "oauthAccount": {
                "emailAddress": "user@company.com",
                "organizationType": "claude_enterprise",
                "subscriptionType": "API Usage Billing"
            }
        })
        .to_string(),
    )
    .expect("write account");
    assert_eq!(
        load_claude_organization_type(&path).as_deref(),
        Some("Claude Enterprise")
    );
}

#[test]
fn claude_organization_type_humanizes_team_tier() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({
            "oauthAccount": {
                "emailAddress": "user@team.ai",
                "organizationType": "claude_team"
            }
        })
        .to_string(),
    )
    .expect("write account");
    assert_eq!(
        load_claude_organization_type(&path).as_deref(),
        Some("Claude Team")
    );
}

#[test]
fn claude_organization_type_humanizes_max_tier() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({
            "oauthAccount": {
                "organizationType": "claude_max"
            }
        })
        .to_string(),
    )
    .expect("write account");
    assert_eq!(
        load_claude_organization_type(&path).as_deref(),
        Some("Claude Max")
    );
}

#[test]
fn claude_organization_type_absent_returns_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({ "oauthAccount": { "emailAddress": "x@y.com" } }).to_string(),
    )
    .expect("write account");
    assert_eq!(load_claude_organization_type(&path), None);
}

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
fn amp_daily_display_text_maps_daily_slot_and_reset_description() {
    let api = AmpUsage::from_api_value(serde_json::json!({
        "result": { "displayText": AMP_DAILY_FIXTURE }
    }))
    .expect("Amp API daily usage");
    let cli = parse_amp_usage_output(AMP_DAILY_FIXTURE).expect("Amp CLI daily usage");

    // API and CLI delegate to one parser: identical parsed fields.
    assert_eq!(api.account_label.as_deref(), Some("user@example.com"));
    assert_eq!(api.account_label, cli.account_label);
    assert_eq!(api.daily_remaining_percent, Some(61));
    assert_eq!(api.daily_remaining_percent, cli.daily_remaining_percent);
    assert_eq!(api.individual_credits, cli.individual_credits);
    assert_eq!(api.workspace_balances, cli.workspace_balances);

    let buckets = api.buckets(1_781_185_560);
    assert_eq!(buckets[0].label, "Amp Free");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Daily));
    assert_eq!(buckets[0].remaining_percent, Some(61));
    assert_eq!(buckets[0].reset_label.as_deref(), Some("Resets daily"));
    assert_eq!(buckets[0].resets_at, None);
}

#[test]
fn amp_daily_percentage_clamps_to_protocol_range() {
    let high = parse_amp_usage_output("Amp Free: 140% remaining today (resets daily)")
        .expect("high daily");
    assert_eq!(high.daily_remaining_percent, Some(100));
    let low =
        parse_amp_usage_output("Amp Free: -5% remaining today (resets daily)").expect("low daily");
    assert_eq!(low.daily_remaining_percent, Some(0));
    // A malformed/non-finite percent yields no Daily bucket.
    assert!(parse_amp_usage_output("Amp Free: abc% remaining today (resets daily)").is_none());
}

#[test]
fn amp_daily_parser_preserves_workspace_balances_in_order() {
    let usage = parse_amp_usage_output(AMP_TWO_WORKSPACE_FIXTURE).expect("two workspace");
    assert_eq!(usage.individual_credits, Some(9.86));
    assert_eq!(
        usage.workspace_balances,
        vec![
            AmpWorkspaceBalance {
                name: "alpha".to_owned(),
                remaining: 5.33,
            },
            AmpWorkspaceBalance {
                name: "beta".to_owned(),
                remaining: 2.25,
            },
        ]
    );
    let buckets = usage.buckets(1_781_185_560);
    let labels: Vec<_> = buckets.iter().map(|bucket| bucket.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "Amp Free",
            "Individual credits",
            "Workspace alpha",
            "Workspace beta"
        ]
    );
    // Only the Amp Free bucket carries a status slot.
    assert!(
        buckets[1..]
            .iter()
            .all(|bucket| bucket.status_slot.is_none())
    );
}
