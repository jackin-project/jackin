// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Provider, Secret, Timestamp};
use jackin_testkit::{Method, Route, UsageServer};
use jackin_usage::connected::KeyConnection;
use jackin_usage_provider_core::ProviderError;
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
    assert_eq!(snapshot["period_spend"]["today"]["spend"], "1.25");
    assert_eq!(snapshot["period_spend"]["today"]["byokSpend"], "0.75");
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
    assert_eq!(snapshot["period_spend"]["today"]["spend"], "126.25");
    assert_eq!(snapshot["activity"]["observed_days"], 30);
    assert_eq!(
        snapshot["activity"]["yesterday"]["by_model"]["alpha"]["inputTokens"],
        6.0
    );
    let days = snapshot["activity"]["days"]
        .as_object()
        .ok_or("missing daily history")?;
    assert!(!days.contains_key("2025-01-06"));
    assert_eq!(days["2025-01-04"]["totals"]["spend"], "0");
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
    assert_eq!(snapshot["resources"]["balance"]["available"], "0");
    assert_eq!(snapshot["activity"]["observed_days"], 29);
    assert!(snapshot["activity"]["yesterday"].is_null());
    server.replace(P, Method::Get, "/api/v1/credits", Route::json(503, "{}"));
    assert_eq!(
        KeyConnection::OpenRouter
            .read_with_policy(&server.endpoints(), &secret, NOW, None, false)
            .await,
        Err(ProviderError::Status(503))
    );
    Ok(())
}

async fn standard(server: &UsageServer, data: &str) -> Result<Value, ProviderError> {
    server.replace(
        P,
        Method::Get,
        "/api/v1/key",
        Route::json(200, &format!("{{\"data\":{data}}}")),
    );
    KeyConnection::OpenRouter
        .read_with_policy(
            &server.endpoints(),
            &Secret::new("fixture".into()),
            NOW,
            None,
            false,
        )
        .await
}

const EXACT_KEY: &str = r#"{"is_management_key":false,"limit":100.10,"limit_remaining":0.1,
"usage_daily":0.10000000000000001,"usage_weekly":"2.5e-3","usage_monthly":1e-7,
"byok_usage_daily":0.0000001,"byok_usage_weekly":5,"byok_usage_monthly":"0.01",
"effective_budget":{"limit_usd":10000000000.0000001,"spend_usd":10000000000,"reset_interval":"weekly"},
"free_model_daily_requests":{"used":1,"limit":2}}"#;

#[tokio::test]
async fn a_standard_key_keeps_every_digit_of_each_amount() -> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    let snapshot = standard(&server, EXACT_KEY).await?;
    let spend = &snapshot["period_spend"];
    assert_eq!(
        spend["today"],
        json!({"spend":"0.10000000000000001","byokSpend":"0.0000001"})
    );
    assert_eq!(spend["thisWeek"], json!({"spend":"0.0025","byokSpend":"5"}));
    assert_eq!(
        spend["thisMonth"],
        json!({"spend":"0.0000001","byokSpend":"0.01"})
    );
    let key = &snapshot["resources"]["keyLimit"];
    assert_eq!(
        (&key["limit"], &key["used"], &key["remaining"]),
        (&json!("100.1"), &json!("100"), &json!("0.1"))
    );
    let budget = &snapshot["resources"]["budget"];
    assert_eq!(
        (&budget["limit"], &budget["used"], &budget["remaining"]),
        (
            &json!("10000000000.0000001"),
            &json!("10000000000"),
            &json!("0.0000001")
        )
    );
    assert_eq!(snapshot["resources"]["freeRequests"]["limit"], 2.0);
    assert_eq!(snapshot["resources"]["freeRequests"]["used"], 1.0);
    assert!(snapshot["resources"].get("balance").is_none());
    assert!(snapshot["activity"].is_null());
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn negative_amounts_clamp_to_zero() -> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    let snapshot = standard(
        &server,
        r#"{"is_management_key":false,"limit":10,"limit_remaining":-3,
"usage_daily":-1.5,"usage_weekly":-0.25,"usage_monthly":0,"byok_usage_daily":-0.5,
"effective_budget":{"limit_usd":4,"spend_usd":-2}}"#,
    )
    .await?;
    assert_eq!(
        snapshot["resources"]["keyLimit"],
        json!({"kind":"consumption","unit":"usd","used":"10","limit":"10","remaining":"0"})
    );
    assert_eq!(
        snapshot["resources"]["budget"],
        json!({"kind":"consumption","unit":"usd","used":"0","limit":"4","remaining":"4"})
    );
    assert_eq!(
        snapshot["period_spend"]["today"],
        json!({"spend":"0","byokSpend":"0"})
    );
    for period in ["thisWeek", "thisMonth"] {
        assert_eq!(snapshot["period_spend"][period]["spend"], "0");
    }
    Ok(())
}

#[tokio::test]
async fn null_and_unlimited_fields_follow_the_existing_matrix() -> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    let spends = r#""usage_daily":1,"usage_weekly":2,"usage_monthly":3"#;
    for limit in ["null", "0", "-1"] {
        let snapshot = standard(
            &server,
            &format!(
                r#"{{"is_management_key":false,"limit":{limit},"limit_remaining":1,{spends}}}"#
            ),
        )
        .await?;
        assert!(snapshot["resources"].get("keyLimit").is_none(), "{limit}");
    }
    let snapshot = standard(
        &server,
        &format!(r#"{{"is_management_key":false,"byok_usage_daily":null,{spends}}}"#),
    )
    .await?;
    assert_eq!(snapshot["period_spend"]["today"], json!({"spend":"1"}));
    assert_eq!(
        standard(
            &server,
            &format!(r#"{{"is_management_key":false,"limit":10,{spends}}}"#),
        )
        .await,
        Err(ProviderError::Decode("missing OpenRouter consumption"))
    );
    for missing in [
        r#""usage_daily":null,"usage_weekly":2,"usage_monthly":3"#,
        r#""usage_weekly":2,"usage_monthly":3"#,
    ] {
        assert_eq!(
            standard(
                &server,
                &format!(r#"{{"is_management_key":false,{missing}}}"#)
            )
            .await,
            Err(ProviderError::Decode("invalid OpenRouter usage response"))
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_amount_beyond_the_bounds_or_a_sum_beyond_them_is_refused() -> Result<(), Box<dyn Error>>
{
    let server = UsageServer::start().await?;
    assert_eq!(
        standard(
            &server,
            r#"{"is_management_key":false,"usage_daily":1e40,"usage_weekly":0,"usage_monthly":0}"#
        )
        .await,
        Err(ProviderError::OUT_OF_RANGE)
    );
    assert_eq!(
        standard(
            &server,
            r#"{"is_management_key":false,"usage_daily":"abc","usage_weekly":0,"usage_monthly":0}"#
        )
        .await,
        Err(ProviderError::Decode("invalid OpenRouter usage response"))
    );
    server.replace(
        P,
        Method::Get,
        "/api/v1/key",
        Route::json(200, &key(true).to_string()),
    );
    server.serve(P, Method::Get, "/api/v1/credits", Route::json(403, "{}"));
    let widest = r#"{"usage_daily":99999999999999999999999999999999999999,"usage_weekly":0,"usage_monthly":0}"#;
    server.serve(
        P,
        Method::Get,
        KEYS,
        Route::json(200, &format!("{{\"data\":[{widest},{widest}]}}")),
    );
    assert_eq!(
        KeyConnection::OpenRouter
            .read_with_policy(
                &server.endpoints(),
                &Secret::new("fixture".into()),
                NOW,
                None,
                false
            )
            .await,
        Err(ProviderError::OUT_OF_RANGE)
    );
    Ok(())
}

fn alpha(usage: &str) -> String {
    format!(
        r#"{{"date":"2025-01-05","model":"alpha","usage":{usage},"byok_usage_inference":0.2,"prompt_tokens":8,"cached_tokens":2,"completion_tokens":5,"reasoning_tokens":1}}"#
    )
}

fn beta(date: &str, usage: &str) -> String {
    format!(
        r#"{{"date":"{date}","model":"beta","usage":{usage},"prompt_tokens":1,"completion_tokens":1}}"#
    )
}

#[tokio::test]
async fn management_sums_and_activity_stay_exact() -> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    let secret = Secret::new("fixture".into());
    server.serve(
        P,
        Method::Get,
        "/api/v1/key",
        Route::json(200, &key(true).to_string()),
    );
    server.serve(
        P,
        Method::Get,
        "/api/v1/credits",
        Route::json(
            200,
            r#"{"data":{"total_credits":100.00000000000000001,"total_usage":100}}"#,
        ),
    );
    let row = r#"{"usage_daily":0.10000000000000001,"usage_weekly":0.2,"usage_monthly":0.3,"byok_usage_daily":0.10000000000000001}"#;
    server.serve(
        P,
        Method::Get,
        KEYS,
        Route::json(200, &format!("{{\"data\":[{}]}}", vec![row; 100].join(","))),
    );
    server.serve(
        P,
        Method::Get,
        SECOND,
        Route::json(200, &format!("{{\"data\":[{row}]}}")),
    );
    let one = alpha("0.10000000000000001");
    let activity = format!(
        "{{\"data\":[{one},{one},{one},{},{}]}}",
        beta("2025-01-04", "0.1"),
        beta("2025-01-01", "0.0000003")
    );
    server.serve(
        P,
        Method::Get,
        "/api/v1/activity",
        Route::json(200, &activity),
    );
    let snapshot = KeyConnection::OpenRouter
        .read_with_policy(&server.endpoints(), &secret, NOW, None, false)
        .await?;
    let spend = &snapshot["period_spend"];
    assert_eq!(
        spend["today"],
        json!({"spend":"10.10000000000000101","byokSpend":"10.10000000000000101"})
    );
    assert_eq!(spend["thisWeek"], json!({"spend":"20.2"}));
    assert_eq!(spend["thisMonth"], json!({"spend":"30.3"}));
    assert_eq!(
        snapshot["resources"]["balance"]["available"],
        "0.00000000000000001"
    );
    let days = &snapshot["activity"]["days"];
    assert_eq!(days["2025-01-05"]["totals"]["spend"], "0.30000000000000003");
    assert_eq!(days["2025-01-05"]["totals"]["byokSpend"], "0.6");
    assert_eq!(days["2025-01-05"]["totals"]["tokens"], 39.0);
    assert_eq!(
        days["2025-01-05"]["by_model"]["alpha"]["spend"],
        "0.30000000000000003"
    );
    assert_eq!(days["2025-01-04"]["totals"]["spend"], "0.1");
    assert_eq!(days["2025-01-01"]["totals"]["spend"], "0.0000003");
    assert_eq!(snapshot["activity"]["yesterday"], days["2025-01-05"]);
    assert_eq!(
        snapshot["activity"]["prior30Days"]["spend"],
        "0.40000030000000003"
    );
    assert_eq!(snapshot["activity"]["prior30Days"]["byokSpend"], "0.6");
    server.replace(
        P,
        Method::Get,
        "/api/v1/credits",
        Route::json(200, r#"{"data":{"total_credits":3,"total_usage":5}}"#),
    );
    let again = KeyConnection::OpenRouter
        .read_with_policy(&server.endpoints(), &secret, NOW, Some(&snapshot), false)
        .await?;
    assert_eq!(again["resources"]["balance"]["available"], "0");
    assert_eq!(again["activity"], snapshot["activity"]);
    Ok(())
}

async fn activity_read(
    activity: &str,
    previous: Option<&Value>,
    manual: bool,
) -> Result<Value, Box<dyn Error>> {
    let server = UsageServer::start().await?;
    server.serve(
        P,
        Method::Get,
        "/api/v1/key",
        Route::json(200, &key(true).to_string()),
    );
    server.serve(P, Method::Get, "/api/v1/credits", Route::json(403, "{}"));
    server.serve(P, Method::Get, KEYS, Route::json(200, r#"{"data":[]}"#));
    server.serve(
        P,
        Method::Get,
        "/api/v1/activity",
        Route::json(200, activity),
    );
    Ok(KeyConnection::OpenRouter
        .read_with_policy(
            &server.endpoints(),
            &Secret::new("fixture".into()),
            NOW,
            previous,
            manual,
        )
        .await?)
}

#[tokio::test]
async fn an_activity_row_that_is_negative_unreadable_or_too_large_rejects_the_whole_read()
-> Result<(), Box<dyn Error>> {
    let widest = "99999999999999999999999999999999999999";
    for bad in [
        beta("2025-01-04", "-1"),
        beta("2025-01-04", "\"abc\""),
        beta("2025-01-04", "1e40"),
    ] {
        let activity = format!("{{\"data\":[{},{bad}]}}", beta("2025-01-05", "0.1"));
        let snapshot = activity_read(&activity, None, true).await?;
        assert_eq!(snapshot["activity"]["observed_days"], 0, "{bad}");
        assert!(snapshot["activity"]["prior30Days"].is_null(), "{bad}");
        assert!(snapshot["activity"]["yesterday"].is_null(), "{bad}");
    }
    let merged = format!(
        "{{\"data\":[{},{}]}}",
        beta("2025-01-05", widest),
        beta("2025-01-05", widest)
    );
    let snapshot = activity_read(&merged, None, true).await?;
    assert_eq!(snapshot["activity"]["observed_days"], 0);
    let across = format!(
        "{{\"data\":[{},{}]}}",
        beta("2025-01-05", widest),
        beta("2025-01-04", widest)
    );
    let snapshot = activity_read(&across, None, true).await?;
    assert_eq!(snapshot["activity"]["observed_days"], 30);
    assert!(snapshot["activity"]["prior30Days"].is_null());
    Ok(())
}

#[tokio::test]
async fn stored_days_that_are_not_exact_amounts_are_refetched_not_summed()
-> Result<(), Box<dyn Error>> {
    let day =
        |spend: Value| json!({"totals":{"spend":spend,"byokSpend":"0","tokens":0},"by_model":{}});
    let previous = json!({"activity":{
    "as_of":"2025-01-05","read_at":NOW.0 - 4000,"expected_days":30,"observed_days":3,
    "days":{
        "2025-01-04":day(json!(0.1)),
        "2025-01-03":day(json!("abc")),
        "2025-01-02":day(json!("0.25"))
    }}});
    let snapshot = activity_read(
        &format!("{{\"data\":[{}]}}", beta("2025-01-05", "0.5")),
        Some(&previous),
        true,
    )
    .await?;
    let days = &snapshot["activity"]["days"];
    assert_eq!(days["2025-01-04"]["totals"]["spend"], "0");
    assert_eq!(days["2025-01-03"]["totals"]["spend"], "0");
    assert_eq!(days["2025-01-02"]["totals"]["spend"], "0.25");
    assert_eq!(snapshot["activity"]["prior30Days"]["spend"], "0.75");
    Ok(())
}

#[tokio::test]
async fn a_fresh_stored_activity_with_numeric_amounts_is_read_again() -> Result<(), Box<dyn Error>>
{
    let legacy = json!({"activity":{
        "as_of":"2025-01-06","read_at":NOW.0 - 60,"expected_days":30,"observed_days":1,
        "prior30Days":{"spend":0.1,"tokens":0},
        "yesterday":{"totals":{"spend":0.1,"tokens":0},"by_model":{}},
        "days":{"2025-01-05":{"totals":{"spend":0.1,"byokSpend":0,"tokens":0},"by_model":{}}}}});
    let snapshot = activity_read(
        &format!("{{\"data\":[{}]}}", beta("2025-01-05", "0.5")),
        Some(&legacy),
        false,
    )
    .await?;
    assert_eq!(
        snapshot["activity"]["days"]["2025-01-05"]["totals"]["spend"],
        "0.5"
    );
    assert_eq!(snapshot["activity"]["prior30Days"]["spend"], "0.5");
    let exact = json!({"activity":{
        "as_of":"2025-01-06","read_at":NOW.0 - 60,"expected_days":30,"observed_days":1,
        "prior30Days":{"spend":"0.1"},
        "yesterday":{"totals":{"spend":"0.1"},"by_model":{}},
        "days":{"2025-01-05":{"totals":{"spend":"0.1"},"by_model":{}}}}});
    let kept = activity_read(
        &format!("{{\"data\":[{}]}}", beta("2025-01-05", "0.5")),
        Some(&exact),
        false,
    )
    .await?;
    assert_eq!(kept["activity"], exact["activity"]);
    Ok(())
}

#[tokio::test]
async fn a_remainder_below_a_cent_is_not_used_up_and_zero_is() -> Result<(), Box<dyn Error>> {
    use jackin_usage::dashboard::{Dashboard, DashboardMemory, Section};

    let (_temp, registry) = stored(&json!({"resources":{
        "keyLimit":{"kind":"consumption","unit":"usd","used":"99.996","limit":"100","remaining":"0.004"},
        "budget":{"kind":"consumption","unit":"usd","used":"4","limit":"4","remaining":"0"}}}))?;
    let board = Dashboard::load(&registry, &mut DashboardMemory::default(), NOW.0)?;
    let used_up: Vec<&str> = board
        .priority
        .iter()
        .filter(|(section, _)| *section == Section::UsedUp)
        .map(|(_, row)| row.metric.as_str())
        .collect();
    assert_eq!(used_up, ["budget"]);
    let key = board
        .all
        .iter()
        .find(|row| row.metric == "keyLimit")
        .ok_or("no key limit row")?;
    assert_eq!(key.value, "0.004 usd");
    Ok(())
}

#[derive(Debug, Default)]
struct Memory(std::sync::Mutex<std::collections::HashMap<uuid::Uuid, Vec<u8>>>);

impl jackin_usage::accounts::CredentialVault for Memory {
    fn store(
        &self,
        slot: uuid::Uuid,
        value: &jackin_core::SecretBytes,
    ) -> Result<(), jackin_usage::accounts::ConnectionError> {
        self.0
            .lock()
            .map_err(|_| jackin_usage::accounts::ConnectionError::Vault)?
            .insert(slot, value.expose().to_vec());
        Ok(())
    }
    fn load(
        &self,
        slot: uuid::Uuid,
    ) -> Result<jackin_core::SecretBytes, jackin_usage::accounts::ConnectionError> {
        self.0
            .lock()
            .map_err(|_| jackin_usage::accounts::ConnectionError::Vault)?
            .get(&slot)
            .cloned()
            .map(jackin_core::SecretBytes::new)
            .ok_or(jackin_usage::accounts::ConnectionError::MissingCredential)
    }
    fn delete(&self, slot: uuid::Uuid) -> Result<(), jackin_usage::accounts::ConnectionError> {
        self.0
            .lock()
            .map_err(|_| jackin_usage::accounts::ConnectionError::Vault)?
            .remove(&slot);
        Ok(())
    }
}

fn stored(
    snapshot: &Value,
) -> Result<(tempfile::TempDir, jackin_usage::accounts::AccountRegistry), Box<dyn Error>> {
    use jackin_usage::accounts::{AccountRegistry, CandidateIdentity, UsageProvider};
    let temp = tempfile::tempdir()?;
    let vault = std::sync::Arc::new(Memory::default());
    let mut registry = AccountRegistry::with_vault(&temp.path().join("usage/accounts.db"), vault)?;
    registry.add_with_snapshot(
        UsageProvider::OpenRouter,
        "Router",
        &CandidateIdentity::api_key(&Secret::new("fixture".into())),
        &jackin_core::SecretBytes::new(b"fixture".to_vec()),
        NOW.0,
        snapshot,
    )?;
    Ok((temp, registry))
}

#[tokio::test]
async fn the_stored_snapshot_round_trips_and_every_surface_shows_the_exact_text()
-> Result<(), Box<dyn Error>> {
    use jackin_usage::dashboard::{Dashboard, DashboardMemory};
    use jackin_usage::export;

    let server = UsageServer::start().await?;
    let snapshot = standard(&server, EXACT_KEY).await?;
    let (_temp, registry) = stored(&snapshot)?;
    let mut rows = registry.snapshots()?;
    assert_eq!(rows[0].2.as_ref(), Some(&snapshot));
    let board = Dashboard::load(&registry, &mut DashboardMemory::default(), NOW.0)?;
    let shown = |metric: &str| {
        board
            .all
            .iter()
            .find(|row| row.metric == metric)
            .map(|row| row.value.clone())
    };
    assert_eq!(shown("today").as_deref(), Some("0.10000000000000001 usd"));
    assert_eq!(shown("week").as_deref(), Some("0.0025 usd"));
    assert_eq!(shown("month").as_deref(), Some("0.0000001 usd"));
    assert_eq!(shown("budget").as_deref(), Some("0.0000001 usd"));
    let report = export::details_account(
        rows.remove(0),
        0,
        &registry.dashboard_settings()?,
        None,
        NOW,
    );
    let usage = &report["usage"];
    assert_eq!(
        usage["period_spend"]["today"]["spend"],
        "0.10000000000000001"
    );
    assert_eq!(usage["period_spend"]["thisWeek"]["byokSpend"], "5");
    assert_eq!(usage["resources"]["budget"]["limit"], "10000000000.0000001");
    assert_eq!(usage["resources"]["budget"]["remaining"], "0.0000001");
    assert_eq!(usage["resources"]["keyLimit"]["used"], "100");
    let lines: Vec<String> = serde_json::from_value(report["details"].clone())?;
    for expected in [
        "Today · 0.10000000000000001 usd",
        "Limit · 10000000000.0000001 usd",
        "Remaining · 0.0000001 usd",
    ] {
        assert!(
            lines.iter().any(|line| line == expected),
            "{expected}: {lines:?}"
        );
    }
    Ok(())
}

const READER_SNAPSHOT: &str = r#"{"key_type":"standard","resources":{
"keyLimit":{"kind":"consumption","unit":"usd","used":"99.99","limit":"100","remaining":"0.01"},
"balance":{"kind":"balance","unit":"usd","available":"0"},
"budget":{"kind":"consumption","unit":"usd","used":"1.5","limit":"4","remaining":"2.5"}},
"period_spend":{"today":{"spend":"0.0042","byokSpend":"0.00000001"}},
"activity":{"as_of":"2025-01-05","read_at":1736121000,"expected_days":2,"observed_days":2,
"prior30Days":{"spend":"0.3000000000000000004","tokens":4},
"yesterday":{"totals":{"spend":"0.1000000000000000002","tokens":2},"by_model":{"alpha":{"spend":"0.1000000000000000001","tokens":1},"beta":{"spend":"0.0000000000000000001","tokens":1}}},
"days":{
"2025-01-05":{"totals":{"spend":"0.1000000000000000002","tokens":2},"by_model":{"alpha":{"spend":"0.1000000000000000001","tokens":1},"beta":{"spend":"0.0000000000000000001","tokens":1}}},
"2025-01-04":{"totals":{"spend":"0.2000000000000000002","tokens":2},"by_model":{"alpha":{"spend":"0.2000000000000000002","tokens":2}}}}}}"#;

#[tokio::test]
async fn string_amounts_drive_every_dashboard_surface() -> Result<(), Box<dyn Error>> {
    use jackin_usage::dashboard::{Dashboard, DashboardMemory, Section, Severity};

    let snapshot: Value = serde_json::from_str(READER_SNAPSHOT)?;
    let (_temp, registry) = stored(&snapshot)?;
    let board = Dashboard::load(&registry, &mut DashboardMemory::default(), NOW.0)?;
    let row = |metric: &str| {
        board
            .all
            .iter()
            .find(|row| row.metric == metric)
            .ok_or(format!("no row for {metric}"))
    };
    let key = row("keyLimit")?;
    assert_eq!(key.value, "0.01 usd");
    assert_eq!(key.severity, Severity::Critical);
    assert!(key.display_fraction(true).is_some_and(|left| left < 0.0002));
    assert!(
        board
            .priority
            .iter()
            .any(|(section, row)| *section == Section::UsedUp && row.metric == "balance")
    );
    assert_eq!(row("today")?.value, "0.0042 usd");
    let prior = row("prior30")?;
    for expected in [
        "0.3000000000000000004 USD spent",
        "alpha",
        "0.3000000000000000003 USD spent",
        "beta",
        "0.0000000000000000001 USD spent",
    ] {
        assert!(
            prior.details.iter().any(|line| line == expected),
            "{expected}: {:?}",
            prior.details
        );
    }
    let shared = board.redacted_export();
    let values = shared["rows"]
        .as_array()
        .ok_or("rows")?
        .iter()
        .find(|row| row["metric"] == "Key limit")
        .ok_or("shared key limit")?;
    assert!(values["values"]["used"].is_number(), "{values}");
    assert!(values["values"]["limit"].is_number(), "{values}");
    Ok(())
}
