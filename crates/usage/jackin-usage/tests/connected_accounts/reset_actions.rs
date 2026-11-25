// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{Clock, Provider, Secret, SecretBytes};
use jackin_testkit::{FakeClock, Method, Route, UsageServer};
use jackin_usage::accounts::{AccountRegistry, CandidateIdentity, UsageProvider};
use jackin_usage::connected::KeyConnection;
use jackin_usage::onboarding::{LoginError, OwnedLogin};
use jackin_usage::resets::{ClaimOutcome, ResetError, ResetSelection};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{Result, Vault};

#[tokio::test]
async fn claude_confirmation_checks_fresh_grant_and_remembers_spent_selection() -> Result<()> {
    static FIRST_CLAUDE_REQUEST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut db = AccountRegistry::with_vault(&path, Arc::<Vault>::clone(&vault))?;
    let server = UsageServer::start().await?;
    let clock = Arc::new(FakeClock::new());
    let now = clock.now();
    let login = OwnedLogin::new(path, server.endpoints(), clock).with_vault(vault);
    let org = "00000000-0000-4000-8000-0000000000a1";
    let bytes = SecretBytes::new(serde_json::to_vec(&(
        "access",
        Option::<String>::None,
        Option::<i64>::None,
        org,
    ))?);
    let snapshot = json!({"resets":[{"kind":"session","id":"a","remaining":2,"expiresAt":now.0+600,"usable":true,"clears":["five_hour"]}]});
    let id = db.add_with_snapshot(
        UsageProvider::Claude,
        "work",
        &CandidateIdentity::OAuth(Some(format!("member/{org}"))),
        &bytes,
        now.0,
        &snapshot,
    )?;
    let selection = ResetSelection::from_snapshot(id, UsageProvider::Claude, &snapshot, 0, now)?;
    assert!(selection.description().contains("session"));
    let p = Provider::Claude;
    let grants = "/api/oauth/usage?cedar_ember=1&skip_spend=1";
    let listing = |usable| {
        Route::json(200,&json!({"cedar_ember":{"eligible":true,"next_grant_id":"b","grants":[{"id":"a","resets_left":2,"ends_at":now.0+600,"usable_now":usable,"clears":["five_hour"]}]}}).to_string()).requires_header("authorization","Bearer access")
    };
    server.serve(p, Method::Get, grants, listing(false));
    server.serve(p, Method::Get, "/api/oauth/usage", Route::json(200, "{}"));
    server.serve(
        p,
        Method::Get,
        "/api/oauth/profile",
        Route::json(
            200,
            r#"{"organization":{"organization_type":"claude_max"}}"#,
        ),
    );
    let cancel = CancellationToken::new();
    assert!(matches!(
        login.redeem_reset(selection.clone(), &cancel).await,
        Err(LoginError::Reset(ResetError::Changed))
    ));
    assert!(server.mismatches().is_empty(), "{:?}", server.mismatches());
    server.replace(p, Method::Get, grants, listing(true));
    let spend = format!("/api/organizations/{org}/reset_rate_limits");
    server.serve(
        p,
        Method::Post,
        &spend,
        Route::status(503)
            .matching_json(|body| {
                body["grant_id"] == "a"
                    && body["request_id"]
                        .as_str()
                        .is_some_and(|id| FIRST_CLAUDE_REQUEST.set(id.to_owned()).is_ok())
            })
            .requires_header("authorization", "Bearer access"),
    );
    login
        .redeem_reset(selection.clone(), &cancel)
        .await
        .expect_err("an ambiguous reset response must not be treated as confirmed");
    let first = FIRST_CLAUDE_REQUEST.get().ok_or("no initial claim")?;
    let mut after = snapshot.clone();
    after["resets"][0]["remaining"] = json!(1);
    let retry = ResetSelection::from_snapshot(id, UsageProvider::Claude, &after, 0, now)?;
    server.replace(p, Method::Get, grants, listing(false));
    server.replace(
        p,
        Method::Post,
        &spend,
        Route::json(200, r#"{"result":"already_used"}"#)
            .requires_json_body(json!({"program":"cedar_ember","grant_id":"a","request_id":first})),
    );
    assert_eq!(
        login.redeem_reset(retry, &cancel).await?,
        ClaimOutcome::Spent
    );
    server.replace(p, Method::Post, &spend, Route::status(500));
    assert_eq!(
        login.redeem_reset(selection, &cancel).await?,
        ClaimOutcome::Spent
    );
    assert!(server.mismatches().is_empty(), "{:?}", server.mismatches());
    Ok(())
}

#[tokio::test]
async fn chatgpt_ambiguous_consume_can_replay_without_selecting_another_credit() -> Result<()> {
    static FIRST_REQUEST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut db = AccountRegistry::with_vault(&path, Arc::<Vault>::clone(&vault))?;
    let server = UsageServer::start().await?;
    let clock = Arc::new(FakeClock::new());
    let now = clock.now();
    let login = OwnedLogin::new(path, server.endpoints(), clock).with_vault(vault);
    let bytes = SecretBytes::new(serde_json::to_vec(&(
        "scoped",
        Option::<String>::None,
        Option::<i64>::None,
        "workspace",
    ))?);
    let expiry = now.0 + 1000;
    let snap = json!({"resets":[{"kind":"full","remaining":1,"expiresAt":expiry,"usable":true}]});
    let id = db.add_with_snapshot(
        UsageProvider::ChatGpt,
        "work",
        &CandidateIdentity::OAuth(Some("workspace/member".into())),
        &bytes,
        now.0,
        &snap,
    )?;
    let selection = ResetSelection::from_snapshot(id, UsageProvider::ChatGpt, &snap, 0, now)?;
    let p = Provider::Codex;
    let list = "/backend-api/wham/rate-limit-reset-credits";
    let post = format!("{list}/consume");
    let headers = |route: Route| {
        route
            .requires_header("chatgpt-account-id", "workspace")
            .requires_header("originator", "Codex Desktop")
    };
    server.serve(
        p,
        Method::Get,
        list,
        headers(Route::json(
            200,
            &json!({"available_count":1,"credits":[{"id":"credit","expires_at":expiry}]})
                .to_string(),
        )),
    );
    server.serve(
        p,
        Method::Post,
        &post,
        headers(Route::status(503).matching_json(|v| {
            v["credit_id"] == "credit"
                && v["redeem_request_id"]
                    .as_str()
                    .is_some_and(|s| FIRST_REQUEST.set(s.to_owned()).is_ok())
        })),
    );
    server.serve(
        p,
        Method::Get,
        "/backend-api/wham/usage",
        Route::json(200, r#"{"rate_limit_reset_credits":{"available_count":0}}"#),
    );
    server.serve(p,Method::Get,"/backend-api/wham/analytics/daily-workspace-usage-counts?start_date=2025-12-03&end_date=2026-01-01&group_by=day&workspace_user=true",Route::json(200,r#"{"data":[]}"#));
    server.serve(p,Method::Get,"/backend-api/wham/analytics/daily-workspace-usage-counts?start_date=2025-12-31&end_date=2026-01-01&group_by=day&workspace_user=true",Route::json(200,r#"{"data":[]}"#));
    let cancel = CancellationToken::new();
    server.replace(p,Method::Get,list,headers(Route::json(200,&json!({"available_count":2,"credits":[{"id":"other","expires_at":expiry},{"id":"credit","expires_at":expiry}]}).to_string())));
    assert!(matches!(
        login.redeem_reset(selection.clone(), &cancel).await,
        Err(LoginError::Reset(ResetError::Changed))
    ));
    assert!(FIRST_REQUEST.get().is_none());
    server.replace(
        p,
        Method::Get,
        list,
        headers(Route::json(
            200,
            &json!({"available_count":1,"credits":[{"id":"credit","expires_at":expiry}]})
                .to_string(),
        )),
    );
    login
        .redeem_reset(selection.clone(), &cancel)
        .await
        .expect_err("an ambiguous credit response must not be treated as confirmed");
    server.replace(
        p,
        Method::Get,
        list,
        headers(Route::json(200, r#"{"available_count":0,"credits":[]}"#)),
    );
    server.replace(
        p,
        Method::Post,
        &post,
        headers(Route::json(200, r#"{"code":"already_redeemed"}"#)
            .requires_json_body(json!({"credit_id":"credit","redeem_request_id":FIRST_REQUEST.get().ok_or("missing credit selection")?}))),
    );
    assert_eq!(
        login.redeem_reset(selection, &cancel).await?,
        ClaimOutcome::Spent
    );
    assert!(server.mismatches().is_empty(), "{:?}", server.mismatches());
    Ok(())
}

#[tokio::test]
async fn zai_earliest_reset_is_rechecked_and_ambiguous_claim_is_never_retried() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut db = AccountRegistry::with_vault(&path, Arc::<Vault>::clone(&vault))?;
    let server = UsageServer::start().await?;
    let clock = Arc::new(FakeClock::new());
    let now = clock.now();
    let endpoints = server.endpoints();
    let login = OwnedLogin::new(path, endpoints.clone(), clock).with_vault(vault);
    let key = Secret::new("plan-only".into());
    let p = Provider::Zai;
    server.serve(
        p,
        Method::Get,
        "/api/monitor/usage/quota/limit",
        Route::json(200, r#"{"success":true,"code":200,"data":{"limits":[]}}"#),
    );
    server.serve(
        p,
        Method::Get,
        "/api/biz/subscription/list",
        Route::json(200, r#"{"data":[{"productName":"GLM Coding Plan"}]}"#),
    );
    let list = "/api/biz/customer-package-reset/list?targetType=PERSONAL";
    let record = |id, unit: &str| json!({"available":true,"recordId":id,"grantType":"monthly","expireTime":format!("2026-01-01 {unit}:00:00")});
    let listing = |session: Value| {
        Route::json(200,&json!({"success":true,"data":{"fiveHourResets":session,"weekResets":[record(3,"10")]}}).to_string())
    };
    server.serve(
        p,
        Method::Get,
        list,
        listing(json!([record(1, "09"), record(2, "12")])),
    );
    let snapshot = KeyConnection::Zai
        .read_with_policy(&endpoints, &key, now, None, false)
        .await?;
    let id = db.add_with_snapshot(
        UsageProvider::Zai,
        "work",
        &CandidateIdentity::api_key(&key),
        &KeyConnection::Zai.encode(&key)?,
        now.0,
        &snapshot,
    )?;
    assert_eq!(snapshot["resets"][0]["expiresAt"], now.0 + 3600);
    assert_eq!(snapshot["resources"]["rateLimitResets"]["available"], 3);
    ResetSelection::from_snapshot(id, UsageProvider::Zai, &snapshot, 2, now)
        .expect_err("selection must reject an unavailable reset index");
    let selection = ResetSelection::from_snapshot(id, UsageProvider::Zai, &snapshot, 0, now)?;
    let cancel = CancellationToken::new();
    server.replace(
        p,
        Method::Get,
        list,
        listing(json!([record(0, "09"), record(1, "09")])),
    );
    assert!(matches!(
        login.redeem_reset(selection.clone(), &cancel).await,
        Err(LoginError::Reset(ResetError::Changed))
    ));
    server.replace(p, Method::Get, list, listing(json!([record(1, "09")])));
    let spend = "/api/biz/customer-package-reset/use";
    server.serve(p, Method::Post, spend, Route::status(500));
    login
        .redeem_reset(selection.clone(), &cancel)
        .await
        .expect_err("an ambiguous reset response must not be treated as confirmed");
    server.replace(
        p,
        Method::Post,
        spend,
        Route::json(200, r#"{"success":true}"#),
    );
    assert!(matches!(
        login.redeem_reset(selection, &cancel).await,
        Err(LoginError::Reset(ResetError::Unconfirmed))
    ));
    assert!(server.mismatches().is_empty(), "{:?}", server.mismatches());
    Ok(())
}

#[tokio::test]
async fn mirrored_claude_claim_uses_hub_login_and_verified_organization() -> Result<()> {
    const ORG: &str = "00000000-0000-4000-8000-0000000000a1";
    const PROFILE: &str = "https://api.anthropic.com/api/oauth/profile";
    const GRANTS: &str = "https://api.anthropic.com/api/oauth/usage?cedar_ember=1&skip_spend=1";
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let db = AccountRegistry::with_vault(&path, Arc::<Vault>::clone(&vault))?;
    let server = UsageServer::start().await?;
    let clock = Arc::new(FakeClock::new());
    let now = clock.now();
    let login = OwnedLogin::new(path, server.endpoints(), clock).with_vault(vault);
    let p = Provider::Claude;
    server.serve(
        p,
        Method::Get,
        "/v0/management/auth-files",
        Route::json(
            200,
            r#"{"files":[{"id":"login","provider":"claude","auth_index":"slot","email":"work"}]}"#,
        )
        .requires_header("authorization", "Bearer hub-key"),
    );
    let cancel = CancellationToken::new();
    login
        .connect_hub(
            &server.provider_base(p),
            &Secret::new("hub-key".into()),
            None,
            &cancel,
        )
        .await?;
    let mirror = db.list()?.pop().ok_or("no mirror")?;
    let endpoint = "/v0/management/api-call";
    let upstream = |body: Value, check: fn(&Value) -> bool| {
        Route::json(
            200,
            &json!({"status_code":200,"header":{},"body":body.to_string()}).to_string(),
        )
        .requires_header("authorization", "Bearer hub-key")
        .matching_json(check)
    };
    let expiry = now.0 + 900;
    server.serve(
        p,
        Method::Post,
        endpoint,
        upstream(json!({"organization":{"uuid":ORG}}), |v| {
            v["url"] == PROFILE
                && v["method"] == "GET"
                && v["auth_index"] == "slot"
                && v["header"]["Authorization"] == "Bearer $TOKEN$"
        }),
    );
    server.serve(p,Method::Post,endpoint,upstream(json!({"cedar_ember":{"eligible":true,"grants":[{"id":"g","type":"weekly","resets_left":1,"ends_at":expiry,"usable_now":true}]}}),|v|v["url"]==GRANTS && v["method"]=="GET"));
    server.serve(p,Method::Post,endpoint,upstream(json!({"result":"reset"}),|v|{
        let payload=v["data"].as_str().and_then(|s|serde_json::from_str::<Value>(s).ok()).unwrap_or(Value::Null);
        v["url"]=="https://api.anthropic.com/api/organizations/00000000-0000-4000-8000-0000000000a1/reset_rate_limits" && v["method"]=="POST" && v["auth_index"]=="slot" && v["header"]["Authorization"]=="Bearer $TOKEN$" && payload["program"]=="cedar_ember" && payload["grant_id"]=="g" && payload["request_id"].as_str().is_some_and(|s|uuid::Uuid::parse_str(s).is_ok())
    }));
    server.serve(
        p,
        Method::Post,
        endpoint,
        upstream(json!({"seven_day":{"utilization":0}}), |v| {
            v["url"] == "https://api.anthropic.com/api/oauth/usage" && v["method"] == "GET"
        }),
    );
    login.refresh(&mirror, &cancel).await?;
    let snap = db
        .snapshots()?
        .pop()
        .and_then(|(_, _, s)| s)
        .ok_or("missing mirror inventory")?;
    let selection = ResetSelection::from_snapshot(mirror.id, mirror.provider, &snap, 0, now)?;
    assert_eq!(
        login.redeem_reset(selection, &cancel).await?,
        ClaimOutcome::Spent
    );
    assert!(server.mismatches().is_empty(), "{:?}", server.mismatches());
    Ok(())
}
