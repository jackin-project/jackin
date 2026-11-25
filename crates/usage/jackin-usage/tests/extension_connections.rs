// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::{Provider, Secret, Timestamp};
use jackin_testkit::{Method, Route, UsageServer};
use jackin_usage::browser::BrowserGrant;
use jackin_usage::connected::KeyConnection;
use jackin_usage::gemini_login::{self, DesktopClient};
use jackin_usage::grants::GrantError;
use jackin_usage_provider_core::Http;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[tokio::test]
async fn regional_and_balance_credentials_reopen_the_same_usage_source() -> Result<()> {
    let server = UsageServer::start().await?;
    let key = Secret::new("sk-fixture".to_owned());
    for (provider, region) in [
        (Provider::Kimi, Some("cn")),
        (Provider::Kimi, Some("global")),
        (Provider::Minimax, Some("cn")),
        (Provider::Minimax, Some("global")),
        (Provider::Mimo, None),
        (Provider::Mimo, Some("ams")),
    ] {
        let method = KeyConnection::for_provider(provider, region)?;
        let (source, credential) = KeyConnection::decode(provider, &method.encode(&key)?)?;
        assert_eq!(source, method);
        if provider == Provider::Mimo && region.is_none() {
            server.serve(Provider::Mimo,Method::Get,"/v1/platform/balance",Route::json(200,r#"{"object":"balance","is_available":false,"balance_infos":[{"currency":"CNY","cash_balance":"0.00","gift_balance":"0.00","frozen_balance":"0.00","credit_limit":"1.04","credit_available":"0.00","total_available":"0.00"}]}"#).requires_header("authorization","Bearer sk-fixture"));
            let snapshot = source.read(&server.endpoints(), &credential).await?;
            assert_eq!(snapshot["resources"]["CNY/cash"]["available"], 0);
            assert_eq!(snapshot["resources"]["CNY/creditLimit"]["available"], 104);
            assert_eq!(snapshot["resources"]["CNY/cash"]["currency"], "CNY");
        }
    }
    Ok(())
}

async fn grant(client: &DesktopClient) -> Result<BrowserGrant> {
    let login = gemini_login::start(client).await?;
    let param = |name: &str| {
        login
            .authorization_url()
            .query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    };
    assert_eq!(param("access_type").as_deref(), Some("offline"));
    assert_eq!(param("client_id").as_deref(), Some(client.id.as_str()));
    let redirect = Url::parse(&param("redirect_uri").ok_or("no redirect")?)?;
    assert_eq!(redirect.host_str(), Some("127.0.0.1"));
    assert_eq!(redirect.path(), "/oauth2callback");
    let challenge = param("code_challenge").ok_or("no PKCE")?;
    let path = format!(
        "/oauth2callback?state={}&code=sample",
        param("state").ok_or("no state")?
    );
    let callback =
        tokio::spawn(async move { login.wait_callback(&CancellationToken::new()).await });
    let mut socket = TcpStream::connect(("127.0.0.1", redirect.port().ok_or("no port")?)).await?;
    socket
        .write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
        .await?;
    let mut reply = String::new();
    socket.read_to_string(&mut reply).await?;
    assert!(reply.starts_with("HTTP/1.1 200"));
    let grant = callback.await??;
    assert_eq!(
        URL_SAFE_NO_PAD.encode(Sha256::digest(grant.verifier.expose().as_bytes())),
        challenge
    );
    Ok(grant)
}

#[tokio::test]
async fn google_reauthentication_uses_the_subject_and_entitled_project_and_keeps_rotation()
-> Result<()> {
    let client = DesktopClient::new("fixture.apps.googleusercontent.com".into(), None)?;
    let server = UsageServer::start().await?;
    let http =
        Http::new(Duration::from_secs(3)).with_base(&server.provider_base(Provider::Gemini))?;
    server.serve(Provider::Gemini,Method::Post,"/token",Route::json(200,r#"{"access_token":"sample-access","expires_in":3600,"refresh_token":"sample-refresh"}"#));
    server.serve(
        Provider::Gemini,
        Method::Get,
        "/oauth2/v2/userinfo",
        Route::json(200, r#"{"id":"123","email":"person@example.invalid"}"#),
    );
    let load = "/v1internal:loadCodeAssist";
    server.serve(
        Provider::Gemini,
        Method::Post,
        load,
        Route::json(
            200,
            r#"{"currentTier":{"id":"standard"},"cloudaicompanionProject":"one"}"#,
        ),
    );
    let signed = gemini_login::exchange(
        &http,
        &client,
        grant(&client).await?,
        Some("one"),
        Timestamp(50),
    )
    .await?;
    assert_eq!(signed.identity.as_deref(), Some(r#"["123","one"]"#));
    assert_eq!(signed.organization.as_deref(), Some("one"));
    let refresh = signed.tokens.refresh.ok_or("no refresh")?;
    server.replace(
        Provider::Gemini,
        Method::Post,
        "/token",
        Route::json(200, r#"{"access_token":"rotated","expires_in":1800}"#),
    );
    let renewed = gemini_login::renew(&http, &client, &refresh, Timestamp(100)).await?;
    assert_eq!(
        renewed.refresh.ok_or("no retained refresh")?.expose(),
        refresh.expose()
    );
    assert_eq!(renewed.expires_at, Some(Timestamp(1900)));
    server.replace(
        Provider::Gemini,
        Method::Post,
        "/token",
        Route::json(400, r#"{"error":"invalid_grant"}"#),
    );
    assert!(matches!(
        gemini_login::renew(&http, &client, &refresh, Timestamp(200)).await,
        Err(GrantError::SignInAgain)
    ));
    server.replace(
        Provider::Gemini,
        Method::Post,
        "/token",
        Route::json(
            200,
            r#"{"access_token":"sample","expires_in":3600,"refresh_token":"sample-refresh"}"#,
        ),
    );
    server.replace(
        Provider::Gemini,
        Method::Post,
        load,
        Route::json(
            200,
            &json!({"ineligibleTiers":[{"reasonCode":"INELIGIBLE"}]}).to_string(),
        ),
    );
    assert!(
        gemini_login::exchange(&http, &client, grant(&client).await?, None, Timestamp(201))
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn mimo_daily_history_preserves_partial_reads_and_exact_units() -> Result<()> {
    let server = UsageServer::start().await?;
    let provider = Provider::Mimo;
    let auth = |s: &str| Route::json(200, s).requires_header("authorization", "Bearer sk-fixture");
    server.serve(provider,Method::Get,"/v1/platform/balance",auth(r#"{"object":"balance","is_available":true,"balance_infos":[{"currency":"CNY","cash_balance":"0","gift_balance":"0","frozen_balance":"0","credit_limit":"0","credit_available":"0","total_available":"0"}]}"#));
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 22).ok_or("date")?;
    let now = day
        .and_hms_opt(0, 0, 0)
        .ok_or("midnight")?
        .and_utc()
        .timestamp();
    let usage = "/v1/platform/usage/daily?start_date=2026-10-24&end_date=2026-11-22";
    let bills = "/v1/platform/billing/daily?start_date=2026-10-24&end_date=2026-11-22";
    server.serve(provider,Method::Get,usage,auth(r#"{"object":"list","data":[{"date":"2026-11-22","currency":"CNY","resource_type":"model","name":"test","consumed_amount":"0.000001","total_tokens":9007199254740993,"request_count":2,"masked_api_key":"sensitive"},{"date":"2026-10-24","currency":"USD","resource_type":"plugin","name":"tool","consumed_amount":"1.200001","request_count":1}]}"#));
    server.serve(provider, Method::Get, bills, Route::status(500));
    let method = KeyConnection::MimoBalance;
    let key = Secret::new("sk-fixture".to_owned());
    let endpoints = server.endpoints();
    let first = method
        .read_with_policy(&endpoints, &key, Timestamp(now), None, false)
        .await?;
    assert_eq!(
        first["daily"]["consumption"]["rows"][0]["consumed_amount"],
        1
    );
    assert_eq!(
        first["daily"]["consumption"]["rows"][0]["total_tokens"],
        9007199254740993_u64
    );
    assert!(
        first["daily"]["consumption"]["rows"][1]
            .get("total_tokens")
            .is_none()
    );
    assert!(!first.to_string().contains("sensitive"));
    assert_eq!(first["daily"]["bills"]["unavailable"], true);
    let details = &first["resources"]["daily/consumption"]["details"];
    assert!(details.to_string().contains("Total 1.200001 USD"));
    assert!(details.to_string().contains("Total 0.000001 CNY"));
    server.replace(provider, Method::Get, usage, Route::status(500));
    server.replace(provider,Method::Get,bills,auth(r#"{"object":"list","data":[{"date":"2026-11-22","currency":"USD","original_amount":"1.50","discount_amount":"0.25","gift_amount":"0.11","final_paid_amount":"1.14"}]}"#));
    let second = method
        .read_with_policy(&endpoints, &key, Timestamp(now + 300), Some(&first), false)
        .await?;
    assert_eq!(
        second["daily"]["bills"]["rows"][0]["final_paid_amount"],
        114
    );
    assert_eq!(
        second["daily"]["consumption"],
        first["daily"]["consumption"]
    );
    let partial = method
        .read_with_policy(
            &endpoints,
            &key,
            Timestamp(now + 3601),
            Some(&second),
            false,
        )
        .await?;
    assert_eq!(
        partial["daily"]["consumption"]["rows"],
        first["daily"]["consumption"]["rows"]
    );
    assert_eq!(partial["daily"]["consumption"]["unavailable"], true);
    server.replace(
        provider,
        Method::Get,
        usage,
        auth(r#"{"object":"list","data":[]}"#),
    );
    let recovered = method
        .read_with_policy(
            &endpoints,
            &key,
            Timestamp(now + 3902),
            Some(&partial),
            false,
        )
        .await?;
    assert_eq!(recovered["daily"]["consumption"]["rows"], json!([]));
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn minimax_metered_key_selects_regional_balance_and_keeps_provider_units() -> Result<()> {
    let s = UsageServer::start().await?;
    let p = Provider::Minimax;
    let path = "/account/query_balance";
    let response = json!({"available_amount":"0.00","cash_balance":"-1.02","voucher_balance":"0.001","credit_balance":"10","owed_amount":"2.50","balance_alert_switch":false,"balance_alert_threshold":"","base_resp":{"status_code":0}});
    s.serve(
        p,
        Method::Get,
        path,
        Route::json(200, &response.to_string())
            .requires_header("authorization", "Bearer sk-api-key"),
    );
    let method = KeyConnection::for_provider(p, Some("cn"))?;
    let key = Secret::new("sk-api-key".into());
    let snapshot = method.read(&s.endpoints(), &key).await?;
    let resources = &snapshot["resources"];
    assert_eq!(resources["balance/totalAvailable"]["available"], 0);
    assert_eq!(resources["balance/cash"]["available"], -102);
    assert_eq!(resources["balance/vouchers"]["exponent"], 3);
    assert!(
        resources["balance/totalAvailable"]
            .get("currency")
            .is_none()
    );
    let mut missing = response;
    missing["cash_balance"] = serde_json::Value::Null;
    s.replace(p, Method::Get, path, Route::json(200, &missing.to_string()));
    assert!(method.read(&s.endpoints(), &key).await.is_err());
    assert!(s.mismatches().is_empty());
    Ok(())
}
