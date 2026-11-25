// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::{Clock, Provider, Secret};
use jackin_testkit::{FakeClock, Method, Route, UsageServer};
use jackin_usage::accounts::{ConnectionError, UsageProvider};
use jackin_usage::onboarding::{LoginError, LoginPrompt, OwnedLogin, UsageAuth};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Result, Vault, db};

async fn drive(
    login: &OwnedLogin,
    provider: UsageProvider,
    replace: Option<Uuid>,
    expect_field: bool,
) -> Result<std::result::Result<Uuid, LoginError>> {
    let (tx, mut rx) = mpsc::channel(4);
    let cancel = CancellationToken::new();
    let prompts = async {
        if expect_field {
            let Some(LoginPrompt::Field { reply, .. }) = rx.recv().await else {
                return Err("expected client id");
            };
            reply
                .send(Secret::new("registered-client".into()))
                .map_err(|_| "closed")?;
        }
        let Some(LoginPrompt::Device { code, .. }) = rx.recv().await else {
            return Err("expected device code");
        };
        if code.expose() != "USER" {
            return Err("wrong code");
        }
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::time::resume();
        Ok(())
    };
    let (result, ui) = tokio::join!(
        login.connect_with_auth(
            provider,
            Some(UsageAuth::Device),
            Some("cn"),
            replace,
            tx,
            &cancel
        ),
        prompts
    );
    ui?;
    Ok(result)
}

#[tokio::test]
async fn kimi_device_reconnect_checks_identity_and_recovers_lost_vault_registration() -> Result<()>
{
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let db = db(&path, &vault)?;
    let s = UsageServer::start().await?;
    let login =
        OwnedLogin::new(path, s.endpoints(), Arc::new(FakeClock::new())).with_vault(vault.clone());
    let p = Provider::Kimi;
    let token = "/api/oauth/token";
    s.serve(p,Method::Post,"/api/oauth/device_authorization",Route::json(200,r#"{"user_code":"USER","verification_uri_complete":"https://auth.kimi.com/approve","device_code":"dev","expires_in":600,"interval":1}"#).requires_text_body("client_id=registered-client"));
    let granted = Route::json(
        200,
        r#"{"access_token":"access","refresh_token":"refresh","expires_in":120}"#,
    );
    s.serve(p, Method::Post, token, granted.clone());
    s.serve(
        p,
        Method::Get,
        "/coding/v1/me",
        Route::json(
            200,
            r#"{"user_id":"one","nickname":"Work","user_level_name":"Pro"}"#,
        ),
    );
    s.serve(
        p,
        Method::Get,
        "/coding/v1/usages",
        Route::json(200, r#"{"usages":{"limit_5h":{"used_ratio":0.5}}}"#),
    );
    let id = drive(&login, UsageProvider::Kimi, None, true).await??;
    let row = db.list()?.pop().ok_or("missing")?;
    assert_eq!(row.label, "Work");
    let snap = db
        .snapshots()?
        .pop()
        .and_then(|(_, _, v)| v)
        .ok_or("no snapshot")?;
    assert_eq!(snap["authMethod"], "device");
    assert_eq!(snap["region"], "cn");
    assert_eq!(snap["plan"], "Pro");
    s.replace(
        p,
        Method::Post,
        token,
        Route::json(
            200,
            r#"{"access_token":"new","refresh_token":"rotated","expires_in":3600}"#,
        )
        .requires_text_body(
            "grant_type=refresh_token&client_id=registered-client&refresh_token=refresh",
        ),
    );
    login.refresh(&row, &CancellationToken::new()).await?;
    let (before, bytes) = db.credential(id)?;
    let stored: Value = serde_json::from_slice(bytes.expose())?;
    assert_eq!(stored["refresh"], "rotated");
    s.replace(p, Method::Post, token, granted);
    s.replace(
        p,
        Method::Get,
        "/coding/v1/me",
        Route::json(200, r#"{"user_id":"different"}"#),
    );
    assert!(matches!(
        drive(&login, UsageProvider::Kimi, Some(id), false).await?,
        Err(LoginError::Store(ConnectionError::DifferentIdentity))
    ));
    assert_eq!(db.version(id)?, before);
    vault.values.lock().map_err(|_| "vault")?.clear();
    s.replace(
        p,
        Method::Get,
        "/coding/v1/me",
        Route::json(200, r#"{"user_id":"one"}"#),
    );
    assert_eq!(
        drive(&login, UsageProvider::Kimi, Some(id), true).await??,
        id
    );
    s.replace(
        p,
        Method::Post,
        token,
        Route::json(400, r#"{"error":"access_denied"}"#),
    );
    assert!(
        drive(&login, UsageProvider::Kimi, None, true)
            .await?
            .is_err()
    );
    assert_eq!(db.list()?.len(), 1);
    assert!(s.mismatches().is_empty(), "{:?}", s.mismatches());
    Ok(())
}

static CHALLENGE: Mutex<String> = Mutex::new(String::new());
fn form(bytes: &[u8]) -> HashMap<String, String> {
    url::form_urlencoded::parse(bytes).into_owned().collect()
}
fn issue_code(bytes: &[u8]) -> Value {
    let fields = form(bytes);
    if fields["scope"] != "openid profile coding_plan"
        || fields["code_challenge_method"] != "S256"
        || fields["client_id"] != "registered-client"
    {
        return json!({});
    }
    if let Ok(mut value) = CHALLENGE.lock() {
        value.clone_from(&fields["code_challenge"]);
    }
    json!({"state":fields["state"],"user_code":"USER","verification_uri":"https://account.minimaxi.com/confirm","expired_in":1893456000000_i64,"interval":1})
}
fn issue_token(bytes: &[u8]) -> Value {
    let f = form(bytes);
    if f["client_id"] != "registered-client" {
        return json!({});
    }
    if f["grant_type"] == "refresh_token" && f["refresh_token"] == "renew" {
        return json!({"status":"success","access_token":"rotated","expired_in":1893456000000_i64});
    }
    if f["user_code"] != "USER"
        || CHALLENGE.lock().is_ok_and(|c| {
            *c != URL_SAFE_NO_PAD.encode(Sha256::digest(f["code_verifier"].as_bytes()))
        })
    {
        return json!({});
    }
    json!({"status":"success","access_token":"sk-api-oauth","refresh_token":"renew","resource_url":"https://api.minimax.cn","expired_in":1000})
}

#[tokio::test]
async fn minimax_device_checks_pkce_and_uses_grant_product_and_renewal() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let db = db(&path, &vault)?;
    let s = UsageServer::start().await?;
    let clock = Arc::new(FakeClock::new());
    let login = OwnedLogin::new(path, s.endpoints(), clock.clone()).with_vault(vault);
    let p = Provider::Minimax;
    let start = "/oauth2/device/code";
    s.serve(
        p,
        Method::Post,
        start,
        Route::status(200).responding_with(issue_code),
    );
    s.serve(
        p,
        Method::Post,
        "/oauth2/token",
        Route::status(200).responding_with(issue_token),
    );
    let usage = Route::json(
        200,
        r#"{"model_remains":[{"model_name":"M3","current_interval_total_count":30,"current_interval_usage_count":10}],"base_resp":{"status_code":0}}"#,
    );
    s.serve(
        p,
        Method::Get,
        "/v1/token_plan/remains",
        usage
            .clone()
            .requires_header("authorization", "Bearer sk-api-oauth"),
    );
    s.serve(
        p,
        Method::Get,
        "/v1/token_plan/remains",
        usage.requires_header("authorization", "Bearer rotated"),
    );
    let id = drive(&login, UsageProvider::MiniMax, None, true).await??;
    let row = db.list()?.pop().ok_or("missing")?;
    login.refresh(&row, &CancellationToken::new()).await?;
    let (_, bytes) = db.credential(id)?;
    let stored: Value = serde_json::from_slice(bytes.expose())?;
    assert_eq!(stored["access"], "rotated");
    assert_eq!(stored["refresh"], "renew");
    assert!(stored["expires_at_ms"].as_i64() > Some(clock.now().0 * 1000));
    let (tx, mut rx) = mpsc::channel(2);
    let cancel = CancellationToken::new();
    s.replace(
        p,
        Method::Post,
        start,
        Route::json(200, r#"{"state":"wrong"}"#),
    );
    let input = async {
        if let Some(LoginPrompt::Field { reply, .. }) = rx.recv().await {
            let _sent = reply.send(Secret::new("registered-client".into()));
        }
    };
    let (result, ()) = tokio::join!(
        login.connect_with_auth(
            UsageProvider::MiniMax,
            Some(UsageAuth::Device),
            Some("cn"),
            None,
            tx,
            &cancel
        ),
        input
    );
    assert!(result.is_err());
    assert_eq!(db.list()?.len(), 1);
    assert!(s.mismatches().is_empty(), "{:?}", s.mismatches());
    Ok(())
}
