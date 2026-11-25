// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{Result, Vault, db};
use jackin_core::{Clock, Provider, Secret, SecretBytes};
use jackin_testkit::{FakeClock, Method, Route, UsageServer};
use jackin_usage::accounts::{CandidateIdentity, UsageProvider};
use jackin_usage::onboarding::{LoginError, OwnedLogin};
use jackin_usage_provider_core::ProviderError;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn a_verified_mirrored_claude_login_shares_its_durable_request_window() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut registry = db(&path, &vault)?;
    let server = UsageServer::start().await?;
    let clock = Arc::new(FakeClock::new());
    let service = || {
        OwnedLogin::new(
            path.clone(),
            server.endpoints(),
            Arc::<FakeClock>::clone(&clock),
        )
        .with_vault(Arc::<Vault>::clone(&vault))
    };
    let p = Provider::Claude;
    let cancel = CancellationToken::new();
    let usage = "/api/oauth/usage";
    let profile = "/api/oauth/profile";
    let grants = "/api/oauth/usage?cedar_ember=1&skip_spend=1";
    server.serve(p,Method::Get,"/v0/management/auth-files",Route::json(200,r#"{"files":[{"provider":"claude","id":"known","auth_index":"one"},{"provider":"claude","id":"unidentified","auth_index":"two"}]}"#));
    service()
        .connect_hub(
            &server.provider_base(p),
            &Secret::new("hub".into()),
            None,
            &cancel,
        )
        .await?;
    let mirrors = registry.list()?;
    let known = mirrors
        .iter()
        .find(|r| r.label == "known")
        .ok_or("missing mirror")?;
    let unknown = mirrors
        .iter()
        .find(|r| r.label == "unidentified")
        .ok_or("missing second mirror")?;
    serve_profiles(&server);
    service().refresh(known, &cancel).await?;
    service().refresh(unknown, &cancel).await?;
    let token = SecretBytes::new(serde_json::to_vec(&(
        "direct",
        Option::<String>::None,
        Option::<i64>::None,
        "org",
    ))?);
    let id = registry.add(
        &jackin_core::CriticalWrite::begin(),
        UsageProvider::Claude,
        "direct",
        &CandidateIdentity::OAuth(Some("member/org".into())),
        &token,
    )?;
    let direct = registry
        .list()?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or("missing direct")?;
    server.serve(
        p,
        Method::Get,
        usage,
        Route::json(200, r#"{"five_hour":{"utilization":42}}"#),
    );
    server.serve(
        p,
        Method::Get,
        grants,
        Route::status(429).reply_header("Retry-After", "400"),
    );
    let start = clock.now().0;
    service().refresh(&direct, &cancel).await?;
    let saved = registry
        .snapshots()?
        .into_iter()
        .find(|(a, _, _)| a.id == id)
        .and_then(|(_, _, v)| v)
        .ok_or("missing primary quota")?;
    assert_eq!(saved["resources"]["session"]["used"], 42.0);
    let held = |e: LoginError| match e {
        LoginError::Provider(ProviderError::RateLimited {
            retry_after_seconds,
        }) => retry_after_seconds,
        _ => None,
    };
    assert_eq!(
        held(
            service()
                .refresh(known, &cancel)
                .await
                .err()
                .ok_or("shared window was bypassed")?
        ),
        Some(400)
    );
    service().refresh(unknown, &cancel).await?;
    assert_eq!(
        registry
            .failure(known.id)?
            .ok_or("missing mirror hold")?
            .retry_at,
        start + 400
    );
    clock.advance(Duration::from_secs(400));
    server.replace(
        p,
        Method::Get,
        usage,
        Route::status(429).reply_header("Retry-After", "3"),
    );
    assert_eq!(
        held(
            service()
                .refresh(&direct, &cancel)
                .await
                .err()
                .ok_or("second strike was discarded")?
        ),
        Some(600)
    );
    clock.advance(Duration::from_secs(600));
    server.replace(p, Method::Get, usage, Route::json(200, "{}"));
    server.replace(p, Method::Get, grants, Route::json(200, "{}"));
    server.serve(
        p,
        Method::Get,
        profile,
        Route::status(429).reply_header("Retry-After", "5"),
    );
    service().refresh(&direct, &cancel).await?;
    assert_eq!(
        held(
            service()
                .refresh(known, &cancel)
                .await
                .err()
                .ok_or("profile hold was bypassed")?
        ),
        Some(300)
    );
    assert!(server.mismatches().is_empty(), "{:?}", server.mismatches());
    Ok(())
}

fn serve_profiles(server: &UsageServer) {
    let p = Provider::Claude;
    let proxy = "/v0/management/api-call";
    let remote = |body: Value, check: fn(&Value) -> bool| {
        Route::json(
            200,
            &json!({"status_code":200,"header":{},"body":body.to_string()}).to_string(),
        )
        .matching_json(check)
    };
    server.serve(
        p,
        Method::Post,
        proxy,
        remote(
            json!({"account":{"uuid":"member"},"organization":{"uuid":"org"}}),
            |v| {
                v["auth_index"] == "one"
                    && v["url"] == "https://api.anthropic.com/api/oauth/profile"
            },
        ),
    );
    server.serve(
        p,
        Method::Post,
        proxy,
        remote(
            json!({"account":{"uuid":"member"},"organization":{"uuid":true}}),
            |v| {
                v["auth_index"] == "two"
                    && v["url"] == "https://api.anthropic.com/api/oauth/profile"
            },
        ),
    );
    server.serve(
        p,
        Method::Post,
        proxy,
        remote(json!({}), |v| {
            v["url"]
                .as_str()
                .is_some_and(|s| s.starts_with("https://api.anthropic.com/api/oauth/usage"))
        }),
    );
}
