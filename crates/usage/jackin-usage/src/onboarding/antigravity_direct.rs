// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::IsTerminal;

use jackin_core::{Secret, SecretBytes, Timestamp};
use jackin_usage_provider_core::{Auth, Http, JsonReply, PostFields, ProviderError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::{Candidate, GoogleRegistrationFile, LoginError, LoginPrompt, open_browser};
use crate::accounts::CandidateIdentity;
use crate::antigravity_quota::{is_complete, merge, plan_name, project_id};
use crate::browser::BrowserLogin;
use crate::gemini_login::DesktopClient;
use crate::grants::{GrantError, Tokens};

const SCOPES: &str =
    "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const USERINFO: &str = "https://www.googleapis.com/oauth2/v1/userinfo";
const PRODUCTION: &str = "cloudcode-pa.googleapis.com";
const DAILY: &str = "daily-cloudcode-pa.googleapis.com";
const LOAD: &str = "/v1internal:loadCodeAssist";
const QUOTA: &str = "/v1internal:retrieveUserQuotaSummary";
const PLAN_TTL: i64 = 3600;
const RENEW_EARLY: i64 = 300;
const HOUR: u64 = 3600;

const NO_PROJECT: &str =
    "Antigravity couldn't find a project with quota access for this account. Try again shortly.";
const NO_SUBSCRIPTION: &str = "Google reports no Antigravity quota subscription for this project.";
const UNAVAILABLE: &str = "Antigravity usage is temporarily unavailable. Try again shortly.";

pub(super) struct Credential {
    client: DesktopClient,
    tokens: Tokens,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    product: String,
    client_id: String,
    client_secret: Option<String>,
    access: String,
    refresh: Option<String>,
    expires_at: Option<i64>,
}

pub(super) enum ReadError {
    Refused,
    Failed(LoginError),
}

impl Credential {
    pub(super) fn decode(bytes: &SecretBytes) -> Result<Self, LoginError> {
        let stored: Stored =
            serde_json::from_slice(bytes.expose()).map_err(|_| LoginError::Encoding)?;
        if stored.product != "antigravity" {
            return Err(LoginError::Encoding);
        }
        Ok(Self {
            client: DesktopClient::new(stored.client_id, stored.client_secret.map(Secret::new))?,
            tokens: Tokens {
                access: Secret::new(stored.access),
                refresh: stored.refresh.map(Secret::new),
                expires_at: stored.expires_at.map(Timestamp),
            },
        })
    }

    fn encode(&self) -> Result<SecretBytes, LoginError> {
        let stored = Stored {
            product: "antigravity".to_owned(),
            client_id: self.client.id.clone(),
            client_secret: self
                .client
                .secret
                .as_ref()
                .map(|secret| secret.expose().to_owned()),
            access: self.tokens.access.expose().to_owned(),
            refresh: self
                .tokens
                .refresh
                .as_ref()
                .map(|secret| secret.expose().to_owned()),
            expires_at: self.tokens.expires_at.map(|at| at.0),
        };
        serde_json::to_vec(&stored)
            .map(SecretBytes::new)
            .map_err(|_| LoginError::Encoding)
    }

    pub(super) async fn renew_if_due(
        &mut self,
        http: &Http,
        now: Timestamp,
    ) -> Result<Option<SecretBytes>, LoginError> {
        if self
            .tokens
            .expires_at
            .is_none_or(|at| at.0 > now.0.saturating_add(RENEW_EARLY))
        {
            return Ok(None);
        }
        let refresh = self
            .tokens
            .refresh
            .as_ref()
            .ok_or(GrantError::SignInAgain)?;
        match exchange_refresh(http, &self.client, refresh, now).await {
            Ok(tokens) => {
                self.tokens = tokens;
                Ok(Some(self.encode()?))
            }
            Err(LoginError::Provider(_) | LoginError::Grant(GrantError::Provider(_)))
                if self.tokens.expires_at.is_some_and(|at| at.0 > now.0) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) async fn force_renew(
        &mut self,
        http: &Http,
        now: Timestamp,
    ) -> Result<SecretBytes, LoginError> {
        let refresh = self
            .tokens
            .refresh
            .as_ref()
            .ok_or(GrantError::SignInAgain)?;
        self.tokens = exchange_refresh(http, &self.client, refresh, now).await?;
        self.encode()
    }

    pub(super) async fn read(
        &self,
        http: &Http,
        previous: Option<&Value>,
        now: Timestamp,
    ) -> Result<Value, ReadError> {
        let cached = cached_plan(previous, now.0);
        let (project, plan, plan_read_at) = if let Some(saved) = cached {
            saved
        } else {
            match load_plan(http, self.tokens.access.expose()).await {
                Ok(body) => (
                    project_id(&body).or_else(|| {
                        previous
                            .and_then(|value| value.get("project"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    }),
                    plan_name(&body).or_else(|| {
                        previous
                            .and_then(|value| value.get("plan"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    }),
                    Some(now.0),
                ),
                Err(LoadFailure::Auth | LoadFailure::Unavailable) => previous_plan(previous),
            }
        };
        let bodies = quota_bodies(http, self.tokens.access.expose(), project.as_deref()).await?;
        let resources = merge(&bodies).map_err(|error| ReadError::Failed(error.into()))?;
        let mut snapshot = json!({"resources": resources});
        if let Some(plan) = plan {
            snapshot["plan"] = plan.into();
        }
        if let Some(project) = project {
            snapshot["project"] = project.into();
        }
        if let Some(at) = plan_read_at {
            snapshot["plan_read_at"] = at.into();
        }
        Ok(snapshot)
    }
}

fn cached_plan(
    previous: Option<&Value>,
    now: i64,
) -> Option<(Option<String>, Option<String>, Option<i64>)> {
    let previous = previous?;
    let at = previous.get("plan_read_at").and_then(Value::as_i64)?;
    if now >= at.saturating_add(PLAN_TTL) {
        return None;
    }
    Some((
        previous
            .get("project")
            .and_then(Value::as_str)
            .map(str::to_owned),
        previous
            .get("plan")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Some(at),
    ))
}

fn previous_plan(previous: Option<&Value>) -> (Option<String>, Option<String>, Option<i64>) {
    (
        previous
            .and_then(|value| value.get("project"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        previous
            .and_then(|value| value.get("plan"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        previous
            .and_then(|value| value.get("plan_read_at"))
            .and_then(Value::as_i64),
    )
}

enum LoadFailure {
    Auth,
    Unavailable,
}

async fn load_plan(http: &Http, token: &str) -> Result<Value, LoadFailure> {
    let payload = json!({
        "metadata": {
            "ideType": "ANTIGRAVITY",
            "platform": "PLATFORM_UNSPECIFIED",
            "pluginType": "GEMINI"
        }
    });
    for marker in ["production", "daily"] {
        let host = if marker == "production" {
            PRODUCTION
        } else {
            DAILY
        };
        match post_cloud(http, host, marker, LOAD, token, &payload).await {
            Ok(reply) if (200..300).contains(&reply.status) => return Ok(reply.body),
            Ok(reply) if reply.status == 401 || reply.status == 403 => {
                return Err(LoadFailure::Auth);
            }
            Err(ProviderError::Unauthorized | ProviderError::Forbidden) => {
                return Err(LoadFailure::Auth);
            }
            Ok(_) | Err(_) => {}
        }
    }
    Err(LoadFailure::Unavailable)
}

async fn quota_bodies(
    http: &Http,
    token: &str,
    project: Option<&str>,
) -> Result<Vec<Value>, ReadError> {
    let payload = match project {
        Some(project) => json!({"project": project}),
        None => json!({}),
    };
    let mut replies = Vec::new();
    let mut transport = None;
    for (index, (host, marker)) in [(DAILY, "daily"), (PRODUCTION, "production")]
        .into_iter()
        .enumerate()
    {
        match post_cloud(http, host, marker, QUOTA, token, &payload).await {
            Ok(reply) => {
                let complete =
                    index == 0 && (200..300).contains(&reply.status) && is_complete(&reply.body);
                replies.push(reply);
                if complete {
                    break;
                }
            }
            Err(error @ ProviderError::Network(_)) => transport = Some(error),
            Err(_) => {}
        }
    }
    if replies.is_empty() {
        return Err(ReadError::Failed(
            transport
                .unwrap_or(ProviderError::Unsupported(UNAVAILABLE))
                .into(),
        ));
    }
    let bodies: Vec<Value> = replies
        .iter()
        .filter(|reply| (200..300).contains(&reply.status))
        .map(|reply| reply.body.clone())
        .collect();
    if !bodies.is_empty() {
        return Ok(bodies);
    }
    if replies.iter().any(|reply| reply.status == 401) {
        return Err(ReadError::Refused);
    }
    if replies.iter().any(subscription_required) {
        let message = if project.is_none() {
            NO_PROJECT
        } else {
            NO_SUBSCRIPTION
        };
        return Err(ReadError::Failed(
            ProviderError::Unsupported(message).into(),
        ));
    }
    if replies
        .iter()
        .any(|reply| reply.status == 401 || reply.status == 403)
    {
        return Err(ReadError::Failed(ProviderError::Unauthorized.into()));
    }
    if let Some(reply) = replies.iter().find(|reply| reply.status == 429) {
        return Err(ReadError::Failed(
            ProviderError::RateLimited {
                retry_after_seconds: reply
                    .retry_after_seconds
                    .filter(|seconds| *seconds > 0)
                    .map(|seconds| seconds.min(HOUR)),
            }
            .into(),
        ));
    }
    Err(ReadError::Failed(
        ProviderError::Unsupported(UNAVAILABLE).into(),
    ))
}

fn subscription_required(reply: &JsonReply) -> bool {
    reply.status == 403
        && reply
            .body
            .pointer("/error/details")
            .and_then(Value::as_array)
            .is_some_and(|details| {
                details.iter().any(|item| {
                    item.get("reason").and_then(Value::as_str) == Some("SUBSCRIPTION_REQUIRED")
                })
            })
}

async fn post_cloud(
    http: &Http,
    host: &str,
    marker: &str,
    path: &str,
    token: &str,
    body: &Value,
) -> Result<JsonReply, ProviderError> {
    let url = if http.rewrites_origin() {
        format!("https://jackin.invalid/{marker}{path}")
    } else {
        format!("https://{host}{path}")
    };
    let agent = user_agent();
    http.post_reply(&url, Auth::Bearer(token), &[("User-Agent", &agent)], body)
        .await
}

fn user_agent() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "amd64"
    } else {
        "unknown"
    };
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };
    format!("antigravity/hub/2.24.0 (aidev_client; os_type={os}; arch={arch})")
}

async fn exchange_refresh(
    http: &Http,
    client: &DesktopClient,
    refresh: &Secret,
    now: Timestamp,
) -> Result<Tokens, LoginError> {
    let mut fields = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.expose()),
        ("client_id", client.id.as_str()),
    ];
    if let Some(secret) = &client.secret {
        fields.push(("client_secret", secret.expose()));
    }
    let mut tokens = token_response(http, &fields, now).await?;
    if tokens.refresh.is_none() {
        tokens.refresh = Some(Secret::new(refresh.expose().to_owned()));
    }
    Ok(tokens)
}

async fn exchange_code(
    http: &Http,
    client: &DesktopClient,
    code: &str,
    verifier: &str,
    redirect: &str,
    now: Timestamp,
) -> Result<Tokens, LoginError> {
    let mut fields = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("code_verifier", verifier),
        ("redirect_uri", redirect),
        ("client_id", client.id.as_str()),
    ];
    if let Some(secret) = &client.secret {
        fields.push(("client_secret", secret.expose()));
    }
    let tokens = token_response(http, &fields, now).await?;
    if tokens.refresh.is_none() {
        return Err(ProviderError::Decode("Google did not grant offline access").into());
    }
    Ok(tokens)
}

async fn token_response(
    http: &Http,
    fields: &[(&str, &str)],
    now: Timestamp,
) -> Result<Tokens, LoginError> {
    let url = if http.rewrites_origin() {
        "https://jackin.invalid/token"
    } else {
        TOKEN
    };
    let reply = http.post_fields(url, PostFields::Form(fields)).await?;
    if reply.body.get("error").and_then(Value::as_str) == Some("invalid_grant") {
        return Err(GrantError::SignInAgain.into());
    }
    reply.ensure_success()?;
    let expires = reply
        .body
        .get("expires_in")
        .and_then(Value::as_i64)
        .filter(|seconds| *seconds > 0)
        .and_then(|seconds| now.0.checked_add(seconds))
        .ok_or(ProviderError::Decode("invalid Google token lifetime"))?;
    let access = reply
        .body
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or(ProviderError::Decode("missing Google access token"))?;
    Ok(Tokens {
        access: Secret::new(access.to_owned()),
        refresh: reply
            .body
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(|value| Secret::new(value.to_owned())),
        expires_at: Some(Timestamp(expires)),
    })
}

pub(super) async fn connect(
    http: &Http,
    now: Timestamp,
    existing: Option<&SecretBytes>,
    registration: Option<&GoogleRegistrationFile>,
    prompts: mpsc::Sender<LoginPrompt>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Candidate, LoginError> {
    let client = client_for(existing, registration, &prompts).await?;
    let mut login = BrowserLogin::listen(
        0,
        "/oauth-callback",
        "127.0.0.1",
        "https://accounts.google.com/o/oauth2/v2/auth",
        &client.id,
        SCOPES,
    )
    .await?;
    login.add_parameters(&[("access_type", "offline"), ("prompt", "consent")]);
    let url = login.authorization_url().clone();
    let (paste, input) = tokio::sync::oneshot::channel();
    prompts
        .send(LoginPrompt::Browser {
            url: url.clone(),
            paste,
        })
        .await
        .map_err(|_| LoginError::Cancelled)?;
    if std::io::stdout().is_terminal() && open_browser(&url).await.is_err() {
        prompts
            .send(LoginPrompt::BrowserUnavailable)
            .await
            .map_err(|_| LoginError::Cancelled)?;
    }
    let grant = login.wait(input, cancel).await?;
    let tokens = exchange_code(
        http,
        &client,
        grant.code.expose(),
        grant.verifier.expose(),
        &grant.redirect,
        now,
    )
    .await?;
    let identity = identify(http, tokens.access.expose()).await?;
    let credential = Credential { client, tokens };
    let snapshot = credential
        .read(http, None, now)
        .await
        .map_err(|error| match error {
            ReadError::Refused => ProviderError::Unauthorized.into(),
            ReadError::Failed(error) => error,
        })?;
    Ok(Candidate {
        label: identity.label,
        identity: CandidateIdentity::OAuth(Some(identity.key)),
        material: credential.encode()?,
        snapshot: Ok(snapshot),
    })
}

struct Identity {
    key: String,
    label: String,
}

async fn identify(http: &Http, token: &str) -> Result<Identity, LoginError> {
    let url = if http.rewrites_origin() {
        "https://jackin.invalid/userinfo"
    } else {
        USERINFO
    };
    let reply = http
        .get_reply(url, Auth::Bearer(token), &[("User-Agent", &user_agent())])
        .await?;
    reply.ensure_success()?;
    let email = reply
        .body
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let id = reply
        .body
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let key = email
        .clone()
        .map(|value| value.to_ascii_lowercase())
        .or(id)
        .ok_or(ProviderError::Decode(
            "Google did not return an account identity",
        ))?;
    let label = email.unwrap_or_else(|| "Google Account".to_owned());
    Ok(Identity { key, label })
}

async fn client_for(
    existing: Option<&SecretBytes>,
    registration: Option<&GoogleRegistrationFile>,
    prompts: &mpsc::Sender<LoginPrompt>,
) -> Result<DesktopClient, LoginError> {
    if let Some(saved) = existing.and_then(|bytes| Credential::decode(bytes).ok()) {
        return Ok(saved.client);
    }
    if let Some(file) = registration {
        return file.client().await;
    }
    let id = super::gemini_connection::text_field(
        prompts,
        "Jackin Google desktop OAuth client ID",
        false,
    )
    .await?;
    let secret = super::gemini_connection::field(
        prompts,
        "Jackin Google desktop OAuth client secret",
        true,
        true,
    )
    .await?;
    Ok(DesktopClient::new(
        id,
        (!secret.expose().is_empty()).then_some(secret),
    )?)
}
