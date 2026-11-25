// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Provider, Secret, Timestamp};
use jackin_testkit::{Method, Route, UsageServer};
use jackin_usage::connected::KeyConnection;
use serde_json::{Value, json};
use std::error::Error;

const P: Provider = Provider::Openrouter;
const NOW: Timestamp = Timestamp(1736121600);
const KEYS: &str = "/api/v1/keys?include_disabled=true&include_expired=true&offset=0";
const SECOND: &str = "/api/v1/keys?include_disabled=true&include_expired=true&offset=100";

fn key(management: bool) -> Value {
    json!({"data":{"is_management_key":management,"usage_daily":1.25,"usage_weekly":2.5,"usage_monthly":5.0,"byok_usage_daily":0.75,
    "effective_budget":{"limit_usd":4.0,"spend_usd":1.0,"reset_interval":"weekly"},
    "free_model_daily_requests":{"used":1,"limit":2}}})
}

#[tokio::test]
async fn standard_key_sees_own_spend_and_resets_at_monday_utc() -> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    server.serve(
        P,
        Method::Get,
        "/api/v1/key",
        Route::json(200, &key(false).to_string())
            .requires_header("authorization", "Bearer fixture"),
    );
    let snapshot = KeyConnection::OpenRouter
        .read_with_policy(
            &server.endpoints(),
            &Secret::new("fixture".into()),
            NOW,
            None,
            false,
        )
        .await?;
    assert_eq!(snapshot["period_spend"]["today"]["spend"], 1.25);
    assert_eq!(snapshot["period_spend"]["today"]["byokSpend"], 0.75);
    assert_eq!(snapshot["resources"]["budget"]["resets_at"], NOW.0 + 604800);
    assert_eq!(snapshot["resources"]["freeRequests"]["limit"], 2.0);
    assert!(snapshot["resources"].get("balance").is_none());
    assert!(snapshot["activity"].is_null());
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn management_preserves_workspace_visibility_and_observed_history()
-> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    let secret = Secret::new("fixture".into());
    server.serve(
        P,
        Method::Get,
        "/api/v1/key",
        Route::json(200, &key(true).to_string()),
    );
    server.serve(P, Method::Get, "/api/v1/credits", Route::json(403, "{}"));
    let spend = &key(false)["data"];
    let page = vec![spend; 100];
    server.serve(
        P,
        Method::Get,
        KEYS,
        Route::json(200, &json!({"data":page}).to_string()),
    );
    server.serve(
        P,
        Method::Get,
        SECOND,
        Route::json(200, &json!({"data":[spend]}).to_string()),
    );
    server.serve(P,Method::Get,"/api/v1/activity",Route::json(200,r#"{"data":[{"date":"2025-01-05","model":"alpha","usage":4,"prompt_tokens":8,"cached_tokens":2,"completion_tokens":5,"reasoning_tokens":1}]}"#));
    let snapshot = KeyConnection::OpenRouter
        .read_with_policy(&server.endpoints(), &secret, NOW, None, false)
        .await?;
    assert!(snapshot["resources"].get("balance").is_none());
    assert_eq!(snapshot["period_spend"]["today"]["spend"], 126.25);
    assert_eq!(snapshot["activity"]["observed_days"], 30);
    assert_eq!(
        snapshot["activity"]["yesterday"]["by_model"]["alpha"]["inputTokens"],
        6.0
    );
    let days = snapshot["activity"]["days"]
        .as_object()
        .ok_or("missing daily history")?;
    assert!(!days.contains_key("2025-01-06"));
    assert_eq!(days["2025-01-04"]["totals"]["spend"], 0);
    server.replace(
        P,
        Method::Get,
        "/api/v1/credits",
        Route::json(200, r#"{"data":{"total_credits":3,"total_usage":3}}"#),
    );
    server.serve(
        P,
        Method::Get,
        "/api/v1/activity?date=2025-01-05",
        Route::json(503, "{}"),
    );
    let snapshot = KeyConnection::OpenRouter
        .read_with_policy(&server.endpoints(), &secret, NOW, Some(&snapshot), true)
        .await?;
    assert_eq!(snapshot["resources"]["balance"]["available"], 0.0);
    assert_eq!(snapshot["activity"]["observed_days"], 29);
    assert!(snapshot["activity"]["yesterday"].is_null());
    server.replace(P, Method::Get, "/api/v1/credits", Route::json(503, "{}"));
    assert!(
        KeyConnection::OpenRouter
            .read_with_policy(&server.endpoints(), &secret, NOW, None, false)
            .await
            .is_err()
    );
    Ok(())
}
