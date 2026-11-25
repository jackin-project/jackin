// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::{Secret, SecretBytes, Timestamp};
use jackin_usage_provider_core::{DevicePoll, Http, PostFields, ProviderError, poll_device};
use jackin_usage_provider_minimax::Region;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::{Candidate, DeviceLoginError, LoginError, LoginPrompt};
use crate::accounts::CandidateIdentity;

#[derive(Serialize, Deserialize)]
struct Stored<'a> {
    region: &'a str,
    client: &'a str,
    access: &'a str,
    refresh: Option<&'a str>,
    expires_at_ms: i64,
}

pub(super) struct Credential {
    region: Region,
    client: String,
    access: Secret,
    refresh: Option<Secret>,
    expires_at_ms: i64,
}

fn region(value: &str) -> Result<Region, ProviderError> {
    match value {
        "global" => Ok(Region::Global),
        "cn" => Ok(Region::China),
        _ => Err(ProviderError::Unsupported("select the global or cn region")),
    }
}

fn region_name(region: Region) -> &'static str {
    match region {
        Region::Global => "global",
        Region::China => "cn",
    }
}

fn auth(region: Region) -> &'static str {
    match region {
        Region::Global => "https://account.minimax.io",
        Region::China => "https://account.minimaxi.com",
    }
}

fn text<'a>(body: &'a Value, k: &str) -> Result<&'a str, ProviderError> {
    body[k]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(ProviderError::Decode(
            "invalid MiniMax authorization response",
        ))
}

impl Credential {
    pub(super) fn decode(bytes: &SecretBytes) -> Option<Self> {
        let old: Stored<'_> = serde_json::from_slice(bytes.expose()).ok()?;
        Some(Self {
            region: region(old.region).ok()?,
            client: old.client.into(),
            access: Secret::new(old.access.into()),
            refresh: old.refresh.map(|r| Secret::new(r.into())),
            expires_at_ms: old.expires_at_ms,
        })
    }
    fn encode(&self) -> Result<SecretBytes, LoginError> {
        serde_json::to_vec(&Stored {
            region: region_name(self.region),
            client: &self.client,
            access: self.access.expose(),
            refresh: self.refresh.as_ref().map(Secret::expose),
            expires_at_ms: self.expires_at_ms,
        })
        .map(SecretBytes::new)
        .map_err(|_| LoginError::Encoding)
    }

    fn granted(region: Region, client: String, body: &Value) -> Result<Self, ProviderError> {
        if body["status"] != "success" {
            return Err(ProviderError::Unauthorized);
        }
        if let Some(origin) = body["resource_url"].as_str().filter(|s| !s.is_empty()) {
            let trusted = if region == Region::China {
                "https://api.minimax.cn"
            } else {
                "https://api.minimax.io"
            };
            if origin.trim_end_matches('/') != trusted {
                return Err(ProviderError::Unsupported(
                    "MiniMax returned an unrecognized regional API host",
                ));
            }
        }
        Ok(Self {
            region,
            client,
            access: Secret::new(text(body, "access_token")?.into()),
            refresh: body["refresh_token"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| Secret::new(s.into())),
            expires_at_ms: body["expired_in"]
                .as_i64()
                .filter(|n| *n > 0)
                .ok_or(ProviderError::Decode("invalid MiniMax expiry"))?,
        })
    }

    pub(super) async fn renew(
        &mut self,
        http: &Http,
        now: Timestamp,
    ) -> Result<Option<SecretBytes>, LoginError> {
        if self.expires_at_ms > now.0.saturating_add(300).saturating_mul(1000) {
            return Ok(None);
        }
        let refresh = self
            .refresh
            .as_ref()
            .ok_or(crate::grants::GrantError::SignInAgain)?;
        let reply = http
            .post_fields(
                &format!("{}/oauth2/token", auth(self.region)),
                PostFields::Form(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh.expose()),
                    ("client_id", &self.client),
                ]),
            )
            .await?;
        reply.ensure_success()?;
        let mut next = Self::granted(self.region, self.client.clone(), &reply.body)?;
        if next.refresh.is_none() {
            next.refresh = self.refresh.take();
        }
        *self = next;
        Ok(Some(self.encode()?))
    }

    pub(super) async fn snapshot(&self, http: &Http) -> Result<Value, LoginError> {
        let mut snap =
            crate::connected::minimax_snapshot(http, self.region, self.access.expose(), true)
                .await?;
        snap["authMethod"] = "device".into();
        Ok(snap)
    }
}

pub(super) async fn connect(
    http: &Http,
    now: Timestamp,
    region_input: Option<&str>,
    old: Option<Credential>,
    prompts: mpsc::Sender<LoginPrompt>,
    cancel: &CancellationToken,
) -> Result<Candidate, LoginError> {
    let selected = region_input
        .map(region)
        .transpose()?
        .or_else(|| old.as_ref().map(|c| c.region))
        .unwrap_or(Region::Global);
    let client = if let Some(old) = old.filter(|c| c.region == selected) {
        old.client
    } else {
        let input = super::gemini_connection::field(
            &prompts,
            "MiniMax registered OAuth client ID",
            false,
            false,
        )
        .await?;
        let id = input.expose().trim();
        if id.is_empty() {
            return Err(ProviderError::Decode("missing MiniMax OAuth client ID").into());
        }
        id.into()
    };
    let verifier = crate::browser::random_secret()?;
    let state = crate::browser::random_secret()?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.expose().as_bytes()));
    let host = auth(selected);
    let reply = http
        .post_fields(
            &format!("{host}/oauth2/device/code"),
            PostFields::Form(&[
                ("client_id", &client),
                ("scope", "openid profile coding_plan"),
                ("code_challenge", &challenge),
                ("code_challenge_method", "S256"),
                ("state", state.expose()),
            ]),
        )
        .await?;
    reply.ensure_success()?;
    let body = reply.body;
    if body["state"].as_str() != Some(state.expose()) {
        return Err(ProviderError::Decode("MiniMax authorization state mismatch").into());
    }
    let code = Secret::new(text(&body, "user_code")?.into());
    let url = Url::parse(text(&body, "verification_uri")?)
        .ok()
        .filter(|u| {
            u.scheme() == "https"
                && u.has_host()
                && u.username().is_empty()
                && u.password().is_none()
        })
        .ok_or(ProviderError::Decode("invalid MiniMax verification URL"))?;
    let ms = body["expired_in"]
        .as_i64()
        .and_then(|n| n.checked_sub(now.0.checked_mul(1000)?))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or(ProviderError::Decode("MiniMax device login expired"))?;
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(ms))
        .ok_or(ProviderError::OUT_OF_RANGE)?;
    let interval =
        Duration::from_millis(body["interval"].as_u64().filter(|n| *n > 0).unwrap_or(3000));
    prompts
        .send(LoginPrompt::Device {
            url,
            code: Secret::new(code.expose().into()),
        })
        .await
        .map_err(|_| LoginError::Cancelled)?;
    let endpoint = format!("{host}/oauth2/token");
    let credential = poll_device(interval, deadline, cancel, || async {
        let reply = http
            .post_fields(
                &endpoint,
                PostFields::Form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", &client),
                    ("user_code", code.expose()),
                    ("code_verifier", verifier.expose()),
                ]),
            )
            .await?;
        reply.ensure_success()?;
        match reply.body["status"].as_str() {
            Some("pending") => Ok(DevicePoll::Pending),
            Some("denied" | "access_denied") => Ok(DevicePoll::Denied),
            _ => {
                Credential::granted(selected, client.clone(), &reply.body).map(DevicePoll::Granted)
            }
        }
    })
    .await
    .map_err(DeviceLoginError::from)?;
    Ok(Candidate {
        label: "MiniMax Token Plan".into(),
        identity: CandidateIdentity::OAuth(None),
        snapshot: Ok(credential.snapshot(http).await?),
        material: credential.encode()?,
    })
}
