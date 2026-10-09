// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn claude_limits_array_surfaces_every_scoped_model_together() {
    let reset_at = "2026-07-03T06:59:59Z";
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "session", "group": "session", "percent": 46,
              "severity": "normal", "resets_at": "2026-07-03T03:20:00Z", "scope": null },
            { "kind": "weekly_all", "group": "weekly", "percent": 36,
              "severity": "normal", "resets_at": reset_at, "scope": null },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 12,
              "severity": "normal", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Sonnet" } } },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 8,
              "severity": "normal", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Opus" } } },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 43,
              "severity": "warn", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Fable" } } }
        ]
    }))
    .expect("multi-model limits response");

    let buckets = usage.into_buckets(1_781_300_000);
    let labels: Vec<&str> = buckets.iter().map(|b| b.label.as_str()).collect();

    // Headline windows bind to their slots; every model-scoped window renders
    // as its own labelled, non-headline row.
    assert!(labels.contains(&"Session"));
    assert!(labels.contains(&"All models"));
    assert!(labels.contains(&"Sonnet"));
    assert!(labels.contains(&"Opus"));
    assert!(labels.contains(&"Fable"));
    for label in ["Sonnet", "Opus", "Fable"] {
        let b = buckets
            .iter()
            .find(|b| b.label == label)
            .unwrap_or_else(|| panic!("{label} bucket"));
        assert_eq!(b.status_slot, None, "{label} must be non-headline");
        assert!(b.remaining_percent.is_some(), "{label} must carry a meter");
    }
    // Fable carries its own (warn) severity for meter color, independent of the
    // other scoped windows.
    let fable = buckets.iter().find(|b| b.label == "Fable").expect("Fable");
    assert_eq!(fable.severity, UsageSeverity::Warn);
    assert_eq!(fable.remaining_percent, Some(57));
}

#[test]
fn claude_codename_dollar_window_is_surfaced() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": null,
        "amber_ladder": {
            "utilization": 0.0,
            "resets_at": "2026-09-02T06:59:59+00:00",
            "limit_dollars": 25000,
            "used_dollars": 5000.0
        },
        // Present but empty — must not produce a bucket.
        "omelette_promotional": { "utilization": 0.0, "limit_dollars": null }
    }))
    .expect("valid Claude OAuth usage");

    let buckets = usage.into_buckets(1_781_185_560);
    let amber = buckets
        .iter()
        .find(|bucket| bucket.label == "Amber Ladder")
        .expect("amber_ladder dollar window surfaced");
    assert_eq!(amber.used_label.as_deref(), Some("$5000.00 spent"));
    assert_eq!(amber.limit_label.as_deref(), Some("$25000.00"));
    assert_eq!(amber.remaining_percent, Some(80));
    assert_eq!(amber.status_slot, None);
    assert!(
        !buckets
            .iter()
            .any(|bucket| bucket.label.contains("omelette")),
        "a null-budget codename window must not produce a bucket"
    );
}

#[test]
fn claude_spend_disabled_is_surfaced_with_reason() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "spend": {
            "used": { "amount_minor": 7849, "currency": "SGD", "exponent": 2 },
            "limit": { "amount_minor": 26000, "currency": "SGD", "exponent": 2 },
            "percent": 30,
            "severity": "normal",
            "enabled": false,
            "disabled_reason": "out_of_credits"
        }
    }))
    .expect("valid Claude OAuth usage");

    let buckets = usage.into_buckets(1_781_185_560);
    let spend = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Spend))
        .expect("disabled spend bucket is still present");
    assert_eq!(spend.used_label.as_deref(), Some("SGD 78.49 spent"));
    assert_eq!(
        spend.pace_label.as_deref(),
        Some("disabled · out of credits")
    );
    // Headline still shows the cap context: `<used> of <limit>`.
    assert_eq!(
        spend_headline_label(&buckets).as_deref(),
        Some("SGD 78 of 260")
    );
}

#[test]
fn claude_oauth_response_accepts_window_aliases() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 0.10 },
        "seven_day": { "utilization": 0.45 },
        "seven_day_opus": { "utilization": 0.30 },
        // `seven_day_oauth_apps` is a SEPARATE window, not an alias of
        // `seven_day` — it must be ignored, never override Weekly.
        "seven_day_oauth_apps": { "utilization": 0.99 },
        "seven_day_cowork": { "utilization": 0.25 }
    }))
    .expect("valid Claude OAuth usage aliases");

    let buckets = usage.into_buckets(1_781_185_560);

    assert!(
        buckets
            .iter()
            .any(|bucket| bucket.label == "Weekly" && bucket.remaining_percent == Some(55))
    );
    assert!(
        buckets
            .iter()
            .any(|bucket| bucket.label == "Daily Routines" && bucket.remaining_percent == Some(75))
    );
    // A present Opus window is a detail row, never a headline slot.
    assert!(
        buckets
            .iter()
            .any(|bucket| bucket.label == "Opus" && bucket.status_slot.is_none())
    );
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
