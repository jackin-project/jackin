// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::{Provider, Secret, Timestamp};
use jackin_testkit::{Method, Route, UsageServer};
use jackin_usage::browser::{BrowserLogin, BrowserProvider};
use jackin_usage::grants::{GrantError, exchange, exchange_chatgpt, renew};
use jackin_usage_provider_core::{Http, ProviderError};
use serde_json::{Value, json};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn http(server: &UsageServer, provider: Provider) -> Result<Http> {
    Ok(Http::new(Duration::from_secs(2)).with_base(&server.provider_base(provider))?)
}

fn secret(text: &str) -> Secret {
    Secret::new(text.to_owned())
}

fn jwt(payload: Value) -> String {
    format!(
        "e30.{}.example",
        URL_SAFE_NO_PAD.encode(payload.to_string())
    )
}

#[tokio::test]
async fn claude_exchange_requires_profile_and_preserves_organization() -> Result<()> {
    let server = UsageServer::start().await?;
    let http = http(&server, Provider::Claude)?;
    for (profile, identity) in [
        (
            json!({"account":{"uuid":"member","email":"test@example.invalid"},"organization":{"uuid":"work"}}),
            Some("member/work"),
        ),
        (
            json!({"account":{"uuid":"member","email":"test@example.invalid"},"organization":{"uuid":"other"}}),
            Some("member/other"),
        ),
        (
            json!({"account":{"uuid":"member","email":"test@example.invalid"}}),
            Some("member"),
        ),
        (json!({}), None),
    ] {
        let login = BrowserLogin::start(BrowserProvider::Claude).await?;
        let state = login
            .authorization_url()
            .query_pairs()
            .find(|(key, _)| key == "state")
            .ok_or("state missing")?
            .1
            .into_owned();
        let (tx, rx) = oneshot::channel();
        tx.send(secret(&format!("sample-code#{state}")))
            .map_err(|_| "receiver closed")?;
        let grant = login.wait(rx, &CancellationToken::new()).await?;
        server.replace(Provider::Claude,Method::Post,"/v1/oauth/token",
            Route::json(200,r#"{"access_token":"sample-access","refresh_token":"sample-refresh","expires_in":7200}"#)
            .requires_header("Content-Type","application/json").requires_json_body(json!({
              "grant_type":"authorization_code","client_id":"9d1c250a-e61b-44d9-88ed-5944d1962f5e","code":"sample-code","code_verifier":grant.verifier.expose(),"state":state,"redirect_uri":grant.redirect
            })));
        server.replace(
            Provider::Claude,
            Method::Get,
            "/api/oauth/profile",
            Route::json(200, &profile.to_string())
                .requires_header("authorization", "Bearer sample-access"),
        );
        let signed = exchange(&http, BrowserProvider::Claude, grant, Timestamp(100)).await;
        if identity.is_none() {
            assert!(matches!(
                signed,
                Err(GrantError::Provider(ProviderError::Decode(_)))
            ));
        } else {
            let signed = signed?;
            assert_eq!(signed.identity.as_deref(), identity);
            assert_eq!(signed.label, "test@example.invalid");
            assert_eq!(signed.tokens.expires_at, Some(Timestamp(7300)));
        }
    }
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn chatgpt_exchanges_form_and_reads_optional_workspace_and_email() -> Result<()> {
    let server = UsageServer::start().await?;
    let http = http(&server, Provider::Codex)?;
    for has_claims in [true, false] {
        let token = if has_claims {
            jwt(json!({"https://api.openai.com/auth":{"chatgpt_account_id":"workspace"}}))
        } else {
            "opaque".to_owned()
        };
        let id_token = if has_claims {
            jwt(json!({"email":"work@example.invalid"}))
        } else {
            "opaque".to_owned()
        };
        let response = json!({"access_token":token,"id_token":id_token,"expires_in":1800});
        server.replace(Provider::Codex, Method::Post, "/oauth/token",Route::json(200,&response.to_string())
            .requires_header("content-type","application/x-www-form-urlencoded")
            .requires_text_body("grant_type=authorization_code&client_id=app_EMoamEEZ73f0CkXaXp7hrann&code=a%26b&code_verifier=c%2Fd&redirect_uri=https%3A%2F%2Fauth.openai.com%2Fdeviceauth%2Fcallback"));
        let signed = exchange_chatgpt(
            &http,
            secret("a&b"),
            secret("c/d"),
            "https://auth.openai.com/deviceauth/callback",
            Timestamp(100),
        )
        .await?;
        assert_eq!(
            signed.identity.as_deref(),
            has_claims.then_some("workspace/work@example.invalid")
        );
        assert_eq!(
            signed.organization.as_deref(),
            has_claims.then_some("workspace")
        );
        assert_eq!(
            signed.label,
            if has_claims {
                "work@example.invalid"
            } else {
                "ChatGPT Account"
            }
        );
        assert_eq!(signed.tokens.expires_at, Some(Timestamp(1900)));
    }
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn only_invalid_scope_triggers_legacy_claude_grant() -> Result<()> {
    let server = UsageServer::start().await?;
    let http = http(&server, Provider::Claude)?;
    let request = |scope: &str| json!({"grant_type":"refresh_token","refresh_token":"sample-refresh","client_id":"9d1c250a-e61b-44d9-88ed-5944d1962f5e","scope":scope});
    let route = |status, body: &str, scope: &str| {
        Route::json(status, body).requires_json_body(request(scope))
    };
    server.replace(
        Provider::Claude,
        Method::Post,
        "/v1/oauth/token",
        route(
            400,
            r#"{"error":"invalid_scope"}"#,
            "user:profile user:inference",
        ),
    );
    server.serve(
        Provider::Claude,
        Method::Post,
        "/v1/oauth/token",
        route(
            200,
            r#"{"access_token":"access","refresh_token":"rotated","expires_in":45}"#,
            "user:profile",
        ),
    );
    let tokens = renew(
        &http,
        BrowserProvider::Claude,
        &secret("sample-refresh"),
        Timestamp(100),
    )
    .await?;
    assert_eq!(tokens.expires_at, Some(Timestamp(145)));
    assert_eq!(tokens.refresh.as_ref().map(Secret::expose), Some("rotated"));
    server.replace(
        Provider::Claude,
        Method::Post,
        "/v1/oauth/token",
        route(
            400,
            r#"{"error":{"code":"invalid_grant"}}"#,
            "user:profile user:inference",
        ),
    );
    assert!(matches!(
        renew(
            &http,
            BrowserProvider::Claude,
            &secret("sample-refresh"),
            Timestamp(100)
        )
        .await,
        Err(GrantError::SignInAgain)
    ));
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn chatgpt_renewal_uses_jwt_expiry_and_distinguishes_failures() -> Result<()> {
    let server = UsageServer::start().await?;
    let http = http(&server, Provider::Codex)?;
    let token = jwt(json!({"exp":1300}));
    let route = |status, body: &str| {
        Route::json(status,body)
        .requires_text_body("grant_type=refresh_token&client_id=app_EMoamEEZ73f0CkXaXp7hrann&refresh_token=sample-refresh")
    };
    server.replace(
        Provider::Codex,
        Method::Post,
        "/oauth/token",
        route(
            200,
            &json!({"access_token":token,"expires_in":50}).to_string(),
        ),
    );
    let tokens = renew(
        &http,
        BrowserProvider::ChatGpt,
        &secret("sample-refresh"),
        Timestamp(300),
    )
    .await?;
    assert_eq!(tokens.expires_at, Some(Timestamp(1300)));
    assert_eq!(
        tokens.refresh.as_ref().map(Secret::expose),
        Some("sample-refresh")
    );
    for (status, body, error) in [
        (503, "sample-refresh", ProviderError::Status(503)),
        (
            401,
            r#"{"error":"unrelated","message":"sample-refresh"}"#,
            ProviderError::Status(401),
        ),
        (
            429,
            "",
            ProviderError::RateLimited {
                retry_after_seconds: Some(45),
            },
        ),
        (
            200,
            "sample-refresh",
            ProviderError::Decode("the response is not valid JSON"),
        ),
    ] {
        server.replace(
            Provider::Codex,
            Method::Post,
            "/oauth/token",
            route(status, body).reply_header("retry-after", "45"),
        );
        let result = renew(
            &http,
            BrowserProvider::ChatGpt,
            &secret("sample-refresh"),
            Timestamp(300),
        )
        .await
        .expect_err("invalid grant must fail");
        assert!(!format!("{result:?} {result}").contains("sample-refresh"));
        assert!(matches!(result,GrantError::Provider(reason) if reason == error));
    }
    assert!(server.mismatches().is_empty());
    Ok(())
}
