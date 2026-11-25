// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use std::fmt;
use std::time::Duration;

use jackin_core::{Clock, Secret, Timestamp};
use jackin_usage_provider_core::{
    Auth, DevicePoll, DevicePollError, Http, JsonReply, PostFields, ProviderError, poll_device,
};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::browser::{BrowserError, BrowserLogin, BrowserProvider, CHATGPT_CLIENT};
use crate::grants::{
    GrantError, SignedIn, Tokens, claims, device_tokens, error_code, exchange_chatgpt, field,
    renewed_tokens,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceProvider {
    ChatGpt,
    Grok,
    Muse,
    OpenCode,
}

struct DeviceContract {
    client: &'static str,
    start: &'static str,
    token: &'static str,
}

impl DeviceProvider {
    fn contract(self) -> DeviceContract {
        match self {
            Self::ChatGpt => DeviceContract {
                client: CHATGPT_CLIENT,
                start: "https://auth.openai.com/api/accounts/deviceauth/usercode",
                token: "https://auth.openai.com/api/accounts/deviceauth/token",
            },
            Self::Grok => DeviceContract {
                client: "b1a00492-073a-47ea-816f-4c329264a828",
                start: "https://auth.x.ai/oauth2/device/code",
                token: "https://auth.x.ai/oauth2/token",
            },
            Self::Muse => DeviceContract {
                client: "1031625952748946",
                start: "https://auth.meta.com/oidc/device/authorization/",
                token: "https://auth.meta.com/oidc/device/token/",
            },
            Self::OpenCode => DeviceContract {
                client: "opencode-cli",
                start: "https://opencode.ai/console/auth/device/code",
                token: "https://opencode.ai/console/auth/device/token",
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum DeviceLoginError {
    #[error(transparent)]
    Browser(#[from] BrowserError),
    #[error(transparent)]
    Poll(#[from] DevicePollError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Grant(#[from] GrantError),
}

#[derive(Debug)]
pub enum ChatGptLogin {
    Browser(BrowserLogin),
    Device(DeviceLogin),
}

impl ChatGptLogin {
    pub async fn start(http: &Http, cancel: &CancellationToken) -> Result<Self, DeviceLoginError> {
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(DevicePollError::Cancelled.into()),
            result = BrowserLogin::start(BrowserProvider::ChatGpt) => match result {
                Ok(login) => Ok(Self::Browser(login)),
                Err(BrowserError::Bind) => Ok(Self::Device(DeviceLogin::start(DeviceProvider::ChatGpt,http,cancel).await?)),
                Err(error) => Err(error.into()),
            }
        }
    }
}

pub struct DeviceLogin {
    provider: DeviceProvider,
    verification: Url,
    user_code: Secret,
    device_code: Secret,
    interval: Duration,
    deadline: Instant,
}

impl fmt::Debug for DeviceLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceLogin")
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

impl DeviceLogin {
    pub fn verification_url(&self) -> &Url {
        &self.verification
    }

    pub fn user_code(&self) -> &Secret {
        &self.user_code
    }

    pub async fn start(
        provider: DeviceProvider,
        http: &Http,
        cancel: &CancellationToken,
    ) -> Result<Self, DeviceLoginError> {
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(DevicePollError::Cancelled.into()),
            result = Self::request_code(provider,http) => Ok(result?),
        }
    }

    async fn request_code(provider: DeviceProvider, http: &Http) -> Result<Self, ProviderError> {
        let contract = provider.contract();
        let reply = if provider == DeviceProvider::ChatGpt {
            http.post_fields(
                contract.start,
                PostFields::Json(&json!({"client_id":contract.client})),
            )
            .await?
        } else {
            let mut fields = vec![("client_id", contract.client)];
            match provider {
                DeviceProvider::Grok => {
                    fields.push(("scope", "openid email offline_access grok-cli:access"));
                }
                DeviceProvider::OpenCode => fields.push(("supports_org_scope", "true")),
                _ => {}
            }
            http.post_fields(contract.start, PostFields::Form(&fields))
                .await?
        };
        reply.ensure_success()?;
        let body = &reply.body;
        let invalid = ProviderError::Decode("invalid device authorization response");
        let user_code = field(body, "/user_code").ok_or(invalid.clone())?;
        let (verification, device_code, lifetime) = if provider == DeviceProvider::ChatGpt {
            (
                Url::parse("https://auth.openai.com/codex/device").map_err(|_| invalid.clone())?,
                field(body, "/device_auth_id"),
                Some(900),
            )
        } else {
            let origin = Url::parse(contract.start).map_err(|_| invalid.clone())?;
            let verification = ["/verification_uri_complete", "/verification_uri"]
                .into_iter()
                .filter_map(|key| origin.join(body.pointer(key)?.as_str()?).ok())
                .find(|url| {
                    url.scheme() == "https"
                        && url.has_host()
                        && url.username().is_empty()
                        && url.password().is_none()
                })
                .ok_or(invalid.clone())?;
            (
                verification,
                field(body, "/device_code"),
                body["expires_in"].as_u64(),
            )
        };
        let interval = body["interval"]
            .as_u64()
            .or_else(|| body["interval"].as_str()?.parse().ok())
            .unwrap_or(5)
            .max(1);
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(lifetime.ok_or(invalid.clone())?))
            .ok_or(invalid.clone())?;
        Ok(Self {
            provider,
            verification,
            user_code: Secret::new(user_code),
            device_code: Secret::new(device_code.ok_or(invalid)?),
            interval: Duration::from_secs(interval),
            deadline,
        })
    }

    pub async fn finish(
        self,
        http: &Http,
        clock: &dyn Clock,
        cancel: &CancellationToken,
    ) -> Result<SignedIn, DeviceLoginError> {
        let result = poll_device(self.interval, self.deadline, cancel, || self.poll(http)).await?;
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(DevicePollError::Cancelled.into()),
            result = self.identify(http,result,clock.now()) => result,
        }
    }

    async fn poll(&self, http: &Http) -> Result<DevicePoll<JsonReply>, ProviderError> {
        let contract = self.provider.contract();
        let reply = if self.provider == DeviceProvider::ChatGpt {
            http.post_fields(contract.token,PostFields::Json(&json!({"device_auth_id":self.device_code.expose(),"user_code":self.user_code.expose()}))).await
        } else {
            http.post_fields(
                contract.token,
                PostFields::Form(&[
                    ("client_id", contract.client),
                    ("device_code", self.device_code.expose()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ]),
            )
            .await
        };
        let reply = match reply {
            Err(ProviderError::Network(_)) => return Ok(DevicePoll::Pending),
            other => other?,
        };
        if (200..300).contains(&reply.status) && error_code(&reply.body).is_none() {
            return Ok(DevicePoll::Granted(reply));
        }
        if self.provider == DeviceProvider::ChatGpt && matches!(reply.status, 403 | 404) {
            return Ok(DevicePoll::Pending);
        }
        match error_code(&reply.body) {
            Some("authorization_pending" | "deviceauth_authorization_pending") => {
                Ok(DevicePoll::Pending)
            }
            Some("slow_down") => Ok(DevicePoll::SlowDown),
            Some("expired_token") => Ok(DevicePoll::Expired),
            Some("access_denied") => Ok(DevicePoll::Denied),
            _ => Err(reply
                .ensure_success()
                .err()
                .unwrap_or(ProviderError::Decode("unexpected device poll response"))),
        }
    }

    async fn identify(
        &self,
        http: &Http,
        reply: JsonReply,
        now: Timestamp,
    ) -> Result<SignedIn, DeviceLoginError> {
        let invalid = ProviderError::Decode("invalid account identification response");
        if self.provider == DeviceProvider::ChatGpt {
            let code = field(&reply.body, "/authorization_code").ok_or(invalid.clone())?;
            let verifier = field(&reply.body, "/code_verifier").ok_or(invalid)?;
            return Ok(exchange_chatgpt(
                http,
                Secret::new(code),
                Secret::new(verifier),
                "https://auth.openai.com/deviceauth/callback",
                now,
            )
            .await?);
        }
        let org = field(&reply.body, "/org_id");
        let mut tokens = device_tokens(reply, now)?;
        let token = tokens.access.expose();
        let (label, identity, organization) = match self.provider {
            DeviceProvider::Muse => {
                let profile = http
                    .post(
                        "https://api.meta.ai/muse-code/key",
                        Auth::Bearer(token),
                        &[("x-api-version", "1.0.0")],
                        &json!({"onboard":false}),
                    )
                    .await?;
                profile["is_subs_active"].as_bool().ok_or(invalid)?;
                let email = field(&profile, "/user_email");
                let identity = email.as_ref().map(|e| format!("muse:{e}"));
                tokens.refresh = None;
                tokens.expires_at = None;
                (
                    email.unwrap_or_else(|| "Muse Code Account".into()),
                    identity,
                    None,
                )
            }
            DeviceProvider::Grok => {
                let claims = claims(token).unwrap_or(Value::Null);
                let email = field(&claims, "/email");
                let identity = field(&claims, "/sub").or_else(|| email.clone());
                (
                    email.unwrap_or_else(|| "Grok Account".into()),
                    identity,
                    None,
                )
            }
            DeviceProvider::OpenCode => {
                let org = org.ok_or(invalid.clone())?;
                let profile = http
                    .get(
                        "https://opencode.ai/console/api/user",
                        Auth::Bearer(token),
                        &[("x-org-id", &org)],
                    )
                    .await?;
                let id = field(&profile, "/id").ok_or(invalid.clone())?;
                let email = field(&profile, "/email").ok_or(invalid)?;
                (email, Some(format!("opencode:{id}:{org}")), Some(org))
            }
            DeviceProvider::ChatGpt => return Err(invalid.into()),
        };
        Ok(SignedIn {
            label,
            identity,
            organization,
            tokens,
        })
    }
}

pub async fn renew_device(
    http: &Http,
    provider: DeviceProvider,
    refresh: &Secret,
    now: Timestamp,
) -> Result<Tokens, GrantError> {
    match provider {
        DeviceProvider::ChatGpt => {
            return crate::grants::renew(http, BrowserProvider::ChatGpt, refresh, now).await;
        }
        DeviceProvider::Muse => return Err(GrantError::SignInAgain),
        _ => {}
    }
    let contract = provider.contract();
    let fields = [
        ("grant_type", "refresh_token"),
        ("client_id", contract.client),
        ("refresh_token", refresh.expose()),
    ];
    let reply = http
        .post_fields(contract.token, PostFields::Form(&fields))
        .await?;
    renewed_tokens(reply, refresh, now)
}
