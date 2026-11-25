// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::{Provider, Secret, Timestamp};
use jackin_testkit::{FakeClock, Method, Route, UsageServer};
use jackin_usage::device_login::{DeviceLogin, DeviceLoginError, DeviceProvider, renew_device};
use jackin_usage::grants::GrantError;
use jackin_usage_provider_core::{DevicePollError, Http, ProviderError};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const CODE: &str = r#"{"device_code":"challenge","user_code":"TEST-CODE","expires_in":30,"interval":1,"verification_uri_complete":"/approve?code=TEST-CODE"}"#;

fn http(s: &UsageServer, p: Provider) -> Result<Http> {
    Ok(Http::new(Duration::from_secs(5)).with_base(&s.provider_base(p))?)
}
fn token_reply(access: &str) -> Value {
    json!({"access_token":access,"refresh_token":"unrelated","expires_in":90})
}
fn poll_form(client: &str) -> String {
    format!(
        "client_id={client}&device_code=challenge&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"
    )
}

#[tokio::test]
async fn opencode_identifies_the_workspace_or_rejects_the_grant() -> Result<()> {
    let s = UsageServer::start().await?;
    let p = Provider::Opencode;
    let http = http(&s, p)?;
    s.serve(
        p,
        Method::Post,
        "/console/auth/device/code",
        Route::json(200, CODE).requires_text_body("client_id=opencode-cli&supports_org_scope=true"),
    );
    let cancel = CancellationToken::new();
    for org in [Some("alpha"), Some("beta"), None] {
        let login = DeviceLogin::start(DeviceProvider::OpenCode, &http, &cancel).await?;
        assert_eq!(login.user_code().expose(), "TEST-CODE");
        assert_eq!(
            login.verification_url().as_str(),
            "https://opencode.ai/approve?code=TEST-CODE"
        );
        let mut token = token_reply("scoped");
        if let Some(org) = org {
            token["org_id"] = json!(org);
            s.replace(
                p,
                Method::Get,
                "/console/api/user",
                Route::json(200, r#"{"id":"member","email":"member@example.invalid"}"#)
                    .requires_header("x-org-id", org)
                    .requires_header("authorization", "Bearer scoped"),
            );
        }
        s.replace(
            p,
            Method::Post,
            "/console/auth/device/token",
            Route::json(200, &token.to_string()).requires_text_body(&poll_form("opencode-cli")),
        );
        let signed = login.finish(&http, &FakeClock::new(), &cancel).await;
        match org {
            Some(org) => assert_eq!(signed?.identity, Some(format!("opencode:member:{org}"))),
            None => assert!(matches!(
                signed,
                Err(DeviceLoginError::Provider(ProviderError::Decode(_)))
            )),
        }
    }
    assert!(s.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn muse_profile_identifies_nonrenewable_signin() -> Result<()> {
    let s = UsageServer::start().await?;
    let p = Provider::Muse;
    let http = http(&s, p)?;
    s.serve(
        p,
        Method::Post,
        "/oidc/device/authorization/",
        Route::json(200, CODE).requires_text_body("client_id=1031625952748946"),
    );
    s.serve(
        p,
        Method::Post,
        "/oidc/device/token/",
        Route::json(200, &token_reply("device-only").to_string())
            .requires_text_body(&poll_form("1031625952748946")),
    );
    s.serve(p,Method::Post,"/muse-code/key",Route::json(200,r#"{"is_subs_active":true,"user_email":" user@example.invalid ","api_key":"never-read-this"}"#).requires_header("x-api-version","1.0.0").requires_header("authorization","Bearer device-only").requires_json_body(json!({"onboard":false})));
    let cancel = CancellationToken::new();
    let signed = DeviceLogin::start(DeviceProvider::Muse, &http, &cancel)
        .await?
        .finish(&http, &FakeClock::new(), &cancel)
        .await?;
    assert_eq!(
        signed.identity.as_deref(),
        Some("muse:user@example.invalid")
    );
    assert_eq!(signed.tokens.access.expose(), "device-only");
    assert!(signed.tokens.refresh.is_none());
    assert!(signed.tokens.expires_at.is_none());
    assert!(matches!(
        renew_device(
            &http,
            DeviceProvider::Muse,
            &Secret::new("unused".into()),
            Timestamp(50)
        )
        .await,
        Err(GrantError::SignInAgain)
    ));
    assert!(s.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn grok_uses_subject_and_rotates_refresh_token() -> Result<()> {
    let s = UsageServer::start().await?;
    let p = Provider::Grok;
    let http = http(&s, p)?;
    let client = "b1a00492-073a-47ea-816f-4c329264a828";
    s.serve(
        p,
        Method::Post,
        "/oauth2/device/code",
        Route::json(200, CODE).requires_text_body(&format!(
            "client_id={client}&scope=openid+email+offline_access+grok-cli%3Aaccess"
        )),
    );
    let access = format!(
        "e30.{}.test",
        URL_SAFE_NO_PAD.encode(br#"{"email":"grok@example.invalid","sub":"subject"}"#)
    );
    s.serve(
        p,
        Method::Post,
        "/oauth2/token",
        Route::json(200, &token_reply(&access).to_string()).requires_text_body(&poll_form(client)),
    );
    let cancel = CancellationToken::new();
    let signed = DeviceLogin::start(DeviceProvider::Grok, &http, &cancel)
        .await?
        .finish(&http, &FakeClock::new(), &cancel)
        .await?;
    assert_eq!(signed.identity.as_deref(), Some("subject"));
    assert_eq!(signed.label, "grok@example.invalid");
    s.serve(
        p,
        Method::Post,
        "/oauth2/token",
        Route::json(
            200,
            r#"{"access_token":"current","refresh_token":"rotated","expires_in":60}"#,
        )
        .requires_text_body(&format!(
            "grant_type=refresh_token&client_id={client}&refresh_token=unrelated"
        )),
    );
    let tokens = renew_device(
        &http,
        DeviceProvider::Grok,
        &signed.tokens.refresh.ok_or("missing token")?,
        Timestamp(100),
    )
    .await?;
    assert_eq!(tokens.refresh.as_ref().map(Secret::expose), Some("rotated"));
    assert_eq!(tokens.expires_at, Some(Timestamp(160)));
    assert!(s.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn chatgpt_device_pending_exchange_and_cancelled_profile() -> Result<()> {
    let s = UsageServer::start().await?;
    let p = Provider::Codex;
    let http = http(&s, p)?;
    s.serve(
        p,
        Method::Post,
        "/api/accounts/deviceauth/usercode",
        Route::json(
            200,
            r#"{"device_auth_id":"challenge","user_code":"TEST-CODE","interval":"1"}"#,
        )
        .requires_json_body(json!({"client_id":"app_EMoamEEZ73f0CkXaXp7hrann"})),
    );
    let path = "/api/accounts/deviceauth/token";
    let poll = |status, body: &str| {
        Route::json(status, body)
            .requires_json_body(json!({"device_auth_id":"challenge","user_code":"TEST-CODE"}))
    };
    s.serve(p, Method::Post, path, poll(403, ""));
    let held = s.hold(p, Method::Post, path);
    let cancel = CancellationToken::new();
    let login = DeviceLogin::start(DeviceProvider::ChatGpt, &http, &cancel).await?;
    assert_eq!(
        login.verification_url().as_str(),
        "https://auth.openai.com/codex/device"
    );
    let clock = FakeClock::new();
    let advance = async {
        held.arrived().await;
        held.release();
        tokio::time::sleep(Duration::from_millis(100)).await;
        s.replace(
            p,
            Method::Post,
            path,
            poll(
                200,
                r#"{"authorization_code":"approved","code_verifier":"match"}"#,
            ),
        );
    };
    s.serve(p,Method::Post,"/oauth/token",Route::json(200,&token_reply("opaque").to_string()).requires_text_body("grant_type=authorization_code&client_id=app_EMoamEEZ73f0CkXaXp7hrann&code=approved&code_verifier=match&redirect_uri=https%3A%2F%2Fauth.openai.com%2Fdeviceauth%2Fcallback"));
    let (signed, ()) = tokio::join!(login.finish(&http, &clock, &cancel), advance);
    assert_eq!(signed?.tokens.access.expose(), "opaque");
    let login = DeviceLogin::start(DeviceProvider::ChatGpt, &http, &cancel).await?;
    let exchange = s.hold(p, Method::Post, "/oauth/token");
    let back = async {
        exchange.arrived().await;
        cancel.cancel();
    };
    let (result, ()) = tokio::join!(login.finish(&http, &clock, &cancel), back);
    exchange.release();
    assert!(matches!(
        result,
        Err(DeviceLoginError::Poll(DevicePollError::Cancelled))
    ));
    assert!(s.mismatches().is_empty());
    Ok(())
}
