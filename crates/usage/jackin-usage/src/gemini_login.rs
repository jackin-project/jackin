// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Secret, Timestamp};
use jackin_usage_provider_core::{Http, PostFields, ProviderError, json_text};
use jackin_usage_provider_gemini::{fetch_buckets, identify, load_project};
use serde_json::{Map, Value, json};

use crate::browser::{BrowserError, BrowserGrant, BrowserLogin};
use crate::grants::{GrantError, SignedIn, Tokens};

const ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";

#[derive(Debug)]
pub struct DesktopClient {
    pub id: String,
    pub secret: Option<Secret>,
}

impl DesktopClient {
    pub fn new(id: String, secret: Option<Secret>) -> Result<Self, ProviderError> {
        if !id.ends_with(".apps.googleusercontent.com")
            || id.len() < 28
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
        {
            return Err(ProviderError::Unsupported(
                "enter the OAuth client ID of a Google Desktop app",
            ));
        }
        Ok(Self { id, secret })
    }
}

pub async fn start(client: &DesktopClient) -> Result<BrowserLogin, BrowserError> {
    let mut login = BrowserLogin::listen(
        0,
        "/oauth2callback",
        "127.0.0.1",
        "https://accounts.google.com/o/oauth2/v2/auth",
        &client.id,
        SCOPES,
    )
    .await?;
    login.add_parameters(&[
        ("access_type", "offline"),
        ("prompt", "consent select_account"),
    ]);
    Ok(login)
}

pub async fn exchange(
    http: &Http,
    client: &DesktopClient,
    grant: BrowserGrant,
    project_id: Option<&str>,
    now: Timestamp,
) -> Result<SignedIn, GrantError> {
    let fields = vec![
        ("grant_type", "authorization_code"),
        ("code", grant.code.expose()),
        ("code_verifier", grant.verifier.expose()),
        ("redirect_uri", &grant.redirect),
    ];
    let tokens = exchange_fields(http, client, fields, now).await?;
    if tokens.refresh.is_none() {
        return Err(ProviderError::Decode("Google did not grant offline access").into());
    }
    let profile = identify(http, tokens.access.expose()).await?;
    let project = load_project(http, tokens.access.expose(), project_id).await?;
    let identity = json!([profile.subject, project.id]).to_string();
    Ok(SignedIn {
        tokens,
        label: profile
            .email
            .or(profile.name)
            .unwrap_or_else(|| "Gemini Account".into()),
        identity: Some(identity),
        organization: Some(project.id),
    })
}

pub async fn renew(
    http: &Http,
    client: &DesktopClient,
    refresh: &Secret,
    now: Timestamp,
) -> Result<Tokens, GrantError> {
    let fields = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.expose()),
    ];
    let mut token = exchange_fields(http, client, fields, now).await?;
    if token.refresh.is_none() {
        token.refresh = Some(Secret::new(refresh.expose().to_owned()));
    }
    Ok(token)
}

async fn exchange_fields<'a>(
    http: &Http,
    client: &'a DesktopClient,
    mut fields: Vec<(&'a str, &'a str)>,
    now: Timestamp,
) -> Result<Tokens, GrantError> {
    fields.push(("client_id", client.id.as_str()));
    if let Some(secret) = &client.secret {
        fields.push(("client_secret", secret.expose()));
    }
    let reply = http
        .post_fields(ENDPOINT, PostFields::Form(&fields))
        .await?;
    if reply.body.get("error").and_then(json_text) == Some("invalid_grant") {
        return Err(GrantError::SignInAgain);
    }
    reply.ensure_success()?;
    let expires = reply
        .body
        .get("expires_in")
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .and_then(|n| now.0.checked_add(n))
        .ok_or(ProviderError::Decode("invalid Google token lifetime"))?;
    Ok(Tokens {
        access: Secret::new(
            reply
                .body
                .get("access_token")
                .and_then(json_text)
                .ok_or(ProviderError::Decode("missing Google access token"))?
                .to_owned(),
        ),
        refresh: reply
            .body
            .get("refresh_token")
            .and_then(json_text)
            .map(|s| Secret::new(s.to_owned())),
        expires_at: Some(Timestamp(expires)),
    })
}

pub async fn snapshot(http: &Http, token: &Secret, project: &str) -> Result<Value, ProviderError> {
    let buckets = fetch_buckets(http, token.expose(), project).await?;
    let mut resources = Map::new();
    for b in buckets {
        let id = json!([b.model, b.token_type]).to_string();
        let entry = json!({"kind":"quota","model":b.model,"unit":b.token_type,"remaining":b.remaining_amount,
            "remaining_fraction":b.remaining_fraction,"resets_at":b.resets_at});
        if resources.insert(id, entry).is_some() {
            return Err(ProviderError::Decode(
                "Code Assist returned duplicate quota buckets",
            ));
        }
    }
    Ok(json!({"project":project,"resources":resources}))
}
