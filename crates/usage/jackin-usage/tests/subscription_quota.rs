// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::{Provider, Secret, Timestamp};
use jackin_testkit::{Method, Route, UsageServer};
use jackin_usage::accounts::UsageProvider;
use jackin_usage::quota::{read, read_with_policy};
use jackin_usage_provider_core::Http;
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn key() -> Secret {
    Secret::new("fixture-access".into())
}
fn http(s: &UsageServer, p: Provider) -> Result<Http> {
    Ok(Http::new(Duration::from_secs(3)).with_base(&s.provider_base(p))?)
}
fn ok(body: Value) -> Route {
    Route::json(200, &body.to_string()).requires_header("authorization", "Bearer fixture-access")
}

#[tokio::test]
async fn muse_idle_retains_week_and_clears_elapsed_session() -> Result<()> {
    let server = UsageServer::start().await?;
    let http = http(&server, Provider::Muse)?;
    let endpoint = "/muse-code/key";
    let mut prior = None;
    for (n, body, expected) in [
        (
            0,
            json!({"is_subs_active":true,"api_key":"private"}),
            Some((0.0, 0.0)),
        ),
        (
            1,
            json!({"is_subs_active":true,"subs_usage":{"window":{"used_percent":19,"resets_at":3},"weekly":{"used_percent":7,"resets_at":1500}}}),
            Some((19.0, 7.0)),
        ),
        (3, json!({"is_subs_active":true}), Some((0.0, 7.0))),
        (4, json!({"is_subs_active":false}), None),
    ] {
        server.replace(
            Provider::Muse,
            Method::Post,
            endpoint,
            ok(body).requires_json_body(json!({"onboard":false})),
        );
        let out = read(
            UsageProvider::Muse,
            &http,
            &key(),
            None,
            Timestamp(n),
            prior.as_ref(),
        )
        .await?;
        if let Some((session, weekly)) = expected {
            assert_eq!(out["resources"]["session"]["used"], session);
            assert_eq!(out["resources"]["weekly"]["used"], weekly);
        } else {
            assert!(out["notice"].is_string());
            assert_eq!(out["resources"], json!({}));
        }
        assert!(!out.to_string().contains("private"));
        prior = Some(out);
    }
    Ok(())
}

#[tokio::test]
async fn claude_currency_grants_and_manual_read_floor() -> Result<()> {
    let server = UsageServer::start().await?;
    let p = Provider::Claude;
    let http = http(&server, p)?;
    let grants = "/api/oauth/usage?cedar_ember=1&skip_spend=1";
    server.serve(p,Method::Get,"/api/oauth/usage",ok(json!({"limits":[{"kind":"weekly_scoped","scope":{"model":{"display_name":"Fable"}},"percent":70}],"extra_usage":{"is_enabled":true,"currency":"EUR","decimal_places":2,"used_credits":0,"monthly_limit":100000}})).requires_header("anthropic-beta","oauth-2025-04-20"));
    server.serve(p,Method::Get,grants,ok(json!({"cedar_ember":{"eligible":true,"next_grant_id":"g","grants":[{"id":"g","resets_left":2,"ends_at":1000,"type":"full","usable_now":true,"gives_back_weekly":true}]}})).requires_header("user-agent","claude-cli/2.1.294 (external, cli)"));
    server.serve(
        p,
        Method::Get,
        "/api/oauth/profile",
        ok(json!({"organization":{"organization_type":"claude_enterprise"}})),
    );
    let mut out = read(
        UsageProvider::Claude,
        &http,
        &key(),
        Some("org"),
        Timestamp(0),
        None,
    )
    .await?;
    assert_eq!(
        out["resources"]["extraUsage"],
        json!({"kind":"consumption","unit":"eur","used":0.0,"remaining":1000.0,"limit":1000.0,"utilization":0.0,"label":"Spend"})
    );
    assert_eq!(out["resources"]["fable"]["used"], 70.0);
    assert!(out["resources"].get("session").is_none());
    assert_eq!(out["resets"][0]["kind"], "fable");
    server.replace(
        p,
        Method::Get,
        grants,
        ok(json!({"cedar_ember":{"eligible":true,"grants":[]}})),
    );
    for (now, count) in [(899, 2), (900, 0)] {
        out = read_with_policy(
            UsageProvider::Claude,
            &http,
            &key(),
            Some("org"),
            Timestamp(now),
            Some(&out),
            true,
        )
        .await?;
        assert_eq!(out["resources"]["rateLimitResets"]["available"], count);
    }
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn chatgpt_dual_windows_credits_partial_history_and_inventory() -> Result<()> {
    let server = UsageServer::start().await?;
    let p = Provider::Codex;
    let http = http(&server, p)?;
    let wham = "/backend-api/wham/";
    let usage = format!("{wham}usage");
    let route = |count| {
        ok(json!({"plan_type":"pro","rate_limit":{"primary_window":{"limit_window_seconds":604800,"used_percent":42}},"additional_rate_limits":[{"limit_name":"Spark","rate_limit":{"secondary_window":{"limit_window_seconds":18000,"used_percent":0}}}],"rate_limit_reset_credits":{"available_count":count},"spend_control":{"individual_limit":{"used":"4.5","limit":"100"}}})).requires_header("ChatGPT-Account-Id","work").reply_header("x-codex-credits-balance","25.5")
    };
    server.serve(p, Method::Get, &usage, route(1));
    server.serve(p,Method::Get,&format!("{wham}rate-limit-reset-credits"),ok(json!({"available_count":1,"credits":[{"id":"one","status":"available","expires_at":"2027-01-01T00:00:00Z"}]})).requires_header("originator","Codex Desktop"));
    let reports = format!(
        "{wham}analytics/daily-workspace-usage-counts?start_date=2025-12-03&end_date=2026-01-01&group_by=day&workspace_user=true"
    );
    server.serve(p,Method::Get,&reports,ok(json!({"data":[{"date":"2025-12-29","totals":{"turns":2,"text_total_tokens":9},"models":[{"model":"m","turns":2}]},{"date":"2026-01-01","totals":{"turns":1,"text_total_tokens":8}}]})));
    let now = Timestamp(1_767_225_600);
    let first = read(
        UsageProvider::ChatGpt,
        &http,
        &key(),
        Some("work"),
        now,
        None,
    )
    .await?;
    assert_eq!(first["plan"], "Pro 200");
    assert!(first["resources"].get("session").is_none());
    assert_eq!(first["resources"]["weekly"]["used"], 42.0);
    assert_eq!(first["resources"]["spark"]["used"], 0.0);
    assert_eq!(first["resources"]["credits"]["available"], 25.0);
    assert_eq!(first["resources"]["creditLimit"]["used"], 4.5);
    assert_eq!(first["resets"][0]["id"], "one");
    assert_eq!(
        first["history"]["ranges"]["last30"]["totals"]["tokens"],
        17.0
    );
    server.replace(p, Method::Get, &usage, route(0));
    server.serve(p,Method::Get,&format!("{wham}analytics/daily-workspace-usage-counts?start_date=2025-12-31&end_date=2026-01-01&group_by=day&workspace_user=true"),Route::status(503));
    let later = read(
        UsageProvider::ChatGpt,
        &http,
        &key(),
        Some("work"),
        Timestamp(now.0 + 3600),
        Some(&first),
    )
    .await?;
    assert_eq!(later["resets"], json!([]));
    assert_eq!(later["history"]["ranges"]["last30"]["observedDays"], 28);
    assert_eq!(
        later["history"]["ranges"]["last30"]["totals"]["tokens"],
        9.0
    );
    assert_eq!(
        later["history"]["ranges"]["last30"]["byModel"]["m"]["turns"],
        2.0
    );
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn open_code_hides_only_untouched_rolling_reset_and_keeps_workspace() -> Result<()> {
    let server = UsageServer::start().await?;
    let p = Provider::Opencode;
    let http = http(&server, p)?;
    let window = |at| json!({"percent":0,"resetsAt":at});
    server.serve(p,Method::Get,"/zen/go/v1/usage",ok(json!({"usage":{"rolling":window("2026-01-01T05:00:00Z"),"weekly":window("2026-01-08T00:00:00Z"),"monthly":window("2026-02-01T00:00:00Z")}})).requires_header("x-opencode-org-id","work").reply_header("Date","Thu, 1 Jan 2026 00:00:00 GMT"));
    server.serve(
        p,
        Method::Get,
        "/console/api/go/status",
        ok(json!({"product":"go-plus"})).requires_header("x-org-id", "work"),
    );
    let out = read(
        UsageProvider::OpenCode,
        &http,
        &key(),
        Some("work"),
        Timestamp(1),
        None,
    )
    .await?;
    assert_eq!(out["plan"], "Go Plus");
    assert!(out["resources"]["session"].get("resetsAt").is_none());
    assert!(out["resources"]["weekly"]["resetsAt"].is_i64());
    assert!(out["resources"]["monthly"]["resetsAt"].is_i64());
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn grok_distinguishes_team_and_proto_zero_from_invalid_personal_quota() -> Result<()> {
    let server = UsageServer::start().await?;
    let p = Provider::Grok;
    let http = http(&server, p)?;
    let endpoint = "/v1/billing?format=credits";
    server.serve(
        p,
        Method::Get,
        endpoint,
        Route::json(412, r#"{"error":"No personal team available"}"#),
    );
    server.serve(
        p,
        Method::Get,
        "/v1/settings",
        ok(json!({"subscription_tier_display":"Team"})),
    );
    let key = key();
    let read_grok = || read(UsageProvider::Grok, &http, &key, None, Timestamp(3), None);
    let out = read_grok().await?;
    assert_eq!(out["plan"], "Team");
    assert!(out["notice"].is_string());
    assert_eq!(out["resources"], json!({}));
    let mut body = json!({"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","start":"2026-01-01T00:00:00Z","end":"2026-01-08T00:00:00Z"},"onDemandCap":{"val":"150"}}});
    server.replace(
        p,
        Method::Get,
        endpoint,
        ok(body.clone()).requires_header("X-XAI-Token-Auth", "xai-grok-cli"),
    );
    let out = read_grok().await?;
    assert_eq!(out["resources"]["weekly"]["used"], 0.0);
    assert_eq!(out["extraUsageCap"]["limit"], 1.5);
    body["config"]["creditUsagePercent"] = json!("unknown");
    server.replace(p, Method::Get, endpoint, ok(body));
    assert!(read_grok().await.is_err());
    assert!(server.mismatches().is_empty());
    Ok(())
}
