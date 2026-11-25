// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::{Secret, Timestamp};
use jackin_usage_provider_core::{Auth, Http, JsonReply, PostFields, ProviderError};
use serde::Deserialize;
use serde_json::{Value, json};
use thiserror::Error;

use crate::browser::{BrowserGrant, BrowserProvider, CHATGPT_CLIENT, CLAUDE_CLIENT, CLAUDE_SCOPES};

const CLAUDE_TOKEN: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_PROFILE: &str = "https://api.anthropic.com/api/oauth/profile";
const CHATGPT_TOKEN: &str = "https://auth.openai.com/oauth/token";

#[derive(Debug)]
pub struct Tokens {
    pub access: Secret,
    pub refresh: Option<Secret>,
    pub expires_at: Option<Timestamp>,
}

#[derive(Debug)]
pub struct SignedIn {
    pub label: String,
    pub identity: Option<String>,
    pub organization: Option<String>,
    pub tokens: Tokens,
}

#[derive(Debug, Error)]
pub enum GrantError {
    #[error("the provider refused the refresh token; sign in again")]
    SignInAgain,
    #[error(transparent)]
    Provider(#[from] ProviderError),
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
}

impl TokenReply {
    fn decode(reply: JsonReply) -> Result<TokenReply, GrantError> {
        reply.ensure_success()?;
        let decoded: Self = serde_json::from_value(reply.body)
            .map_err(|_| ProviderError::Decode("invalid token response"))?;
        if decoded.access_token.trim().is_empty() {
            return Err(ProviderError::Decode("the token response lacks an access token").into());
        }
        Ok(decoded)
    }

    fn tokens(self, now: Timestamp) -> Result<Tokens, GrantError> {
        let expires_at = self
            .expires_in
            .map(|seconds| {
                now.0
                    .checked_add(seconds)
                    .map(Timestamp)
                    .ok_or(ProviderError::OUT_OF_RANGE)
            })
            .transpose()?;
        Ok(Tokens {
            access: Secret::new(self.access_token),
            refresh: self
                .refresh_token
                .filter(|token| !token.is_empty())
                .map(Secret::new),
            expires_at,
        })
    }
}

pub async fn exchange(
    http: &Http,
    provider: BrowserProvider,
    grant: BrowserGrant,
    now: Timestamp,
) -> Result<SignedIn, GrantError> {
    match provider {
        BrowserProvider::Claude => {
            let body = json!({
                "grant_type": "authorization_code",
                "client_id": CLAUDE_CLIENT,
                "code": grant.code.expose(),
                "state": grant.state.expose(),
                "redirect_uri": grant.redirect,
                "code_verifier": grant.verifier.expose(),
            });
            let reply = http
                .post_fields(CLAUDE_TOKEN, PostFields::Json(&body))
                .await?;
            let tokens = TokenReply::decode(reply)?.tokens(now)?;
            let profile = http
                .get(CLAUDE_PROFILE, Auth::Bearer(tokens.access.expose()), &[])
                .await?;
            let account = field(&profile, "/account/uuid").ok_or(ProviderError::Decode(
                "Claude did not return an account UUID",
            ))?;
            let organization = field(&profile, "/organization/uuid");
            let identity = organization
                .as_ref()
                .map_or_else(|| account.clone(), |org| format!("{account}/{org}"));
            let label = field(&profile, "/account/email")
                .or_else(|| field(&profile, "/email"))
                .unwrap_or_else(|| "Claude Account".to_owned());
            Ok(SignedIn {
                tokens,
                label,
                identity: Some(identity),
                organization,
            })
        }
        BrowserProvider::ChatGpt => {
            exchange_chatgpt(http, grant.code, grant.verifier, &grant.redirect, now).await
        }
    }
}

pub async fn exchange_chatgpt(
    http: &Http,
    code: Secret,
    verifier: Secret,
    redirect: &str,
    now: Timestamp,
) -> Result<SignedIn, GrantError> {
    let fields = [
        ("grant_type", "authorization_code"),
        ("client_id", CHATGPT_CLIENT),
        ("code", code.expose()),
        ("code_verifier", verifier.expose()),
        ("redirect_uri", redirect),
    ];
    let reply = http
        .post_fields(CHATGPT_TOKEN, PostFields::Form(&fields))
        .await?;
    let decoded = TokenReply::decode(reply)?;
    let organization = claims(&decoded.access_token)
        .and_then(|claim| field(&claim, "/https:~1~1api.openai.com~1auth/chatgpt_account_id"));
    let email = decoded
        .id_token
        .as_deref()
        .and_then(claims)
        .and_then(|claim| field(&claim, "/email"));
    let identity = organization
        .as_ref()
        .zip(email.as_ref())
        .map(|(account, email)| format!("{account}/{email}"));
    Ok(SignedIn {
        label: email.unwrap_or_else(|| "ChatGPT Account".to_owned()),
        organization,
        identity,
        tokens: decoded.tokens(now)?,
    })
}

pub(crate) fn field(body: &Value, path: &str) -> Option<String> {
    Some(body.pointer(path)?.as_str()?.trim())
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

pub(crate) fn claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&decoded).ok()
}

pub async fn renew(
    http: &Http,
    provider: BrowserProvider,
    refresh: &Secret,
    now: Timestamp,
) -> Result<Tokens, GrantError> {
    let reply = match provider {
        BrowserProvider::Claude => {
            let mut body = json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh.expose(),
                "client_id": CLAUDE_CLIENT,
                "scope": CLAUDE_SCOPES,
            });
            let reply = http
                .post_fields(CLAUDE_TOKEN, PostFields::Json(&body))
                .await?;
            if reply.status == 400 && error_code(&reply.body) == Some("invalid_scope") {
                body["scope"] = Value::from("user:profile");
                http.post_fields(CLAUDE_TOKEN, PostFields::Json(&body))
                    .await?
            } else {
                reply
            }
        }
        BrowserProvider::ChatGpt => {
            let fields = [
                ("grant_type", "refresh_token"),
                ("client_id", CHATGPT_CLIENT),
                ("refresh_token", refresh.expose()),
            ];
            http.post_fields(CHATGPT_TOKEN, PostFields::Form(&fields))
                .await?
        }
    };
    let mut tokens = renewed_tokens(reply, refresh, now)?;
    if provider == BrowserProvider::ChatGpt {
        tokens.expires_at = claims(tokens.access.expose())
            .and_then(|claim| claim.get("exp")?.as_i64())
            .map(Timestamp);
    }
    Ok(tokens)
}

pub(crate) fn device_tokens(reply: JsonReply, now: Timestamp) -> Result<Tokens, GrantError> {
    TokenReply::decode(reply)?.tokens(now)
}

pub(crate) fn renewed_tokens(
    reply: JsonReply,
    refresh: &Secret,
    now: Timestamp,
) -> Result<Tokens, GrantError> {
    if matches!(reply.status, 400 | 401 | 403)
        && matches!(
            error_code(&reply.body),
            Some(
                "invalid_grant"
                    | "refresh_token_expired"
                    | "refresh_token_reused"
                    | "refresh_token_invalidated"
            )
        )
    {
        return Err(GrantError::SignInAgain);
    }
    if !(200..300).contains(&reply.status) {
        return Err(match reply.status {
            429 => ProviderError::RateLimited {
                retry_after_seconds: reply.retry_after_seconds,
            },
            status => ProviderError::Status(status),
        }
        .into());
    }
    let mut tokens = TokenReply::decode(reply)?.tokens(now)?;
    if tokens.refresh.is_none() {
        tokens.refresh = Some(Secret::new(refresh.expose().to_owned()));
    }
    Ok(tokens)
}

pub(crate) fn error_code(body: &Value) -> Option<&str> {
    ["/error", "/error/code", "/code"]
        .into_iter()
        .find_map(|path| body.pointer(path)?.as_str())
}
