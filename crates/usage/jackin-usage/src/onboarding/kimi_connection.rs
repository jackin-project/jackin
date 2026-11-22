// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Clock, Secret, SecretBytes, Timestamp};
use jackin_usage_provider_core::{Http, ProviderError};
use jackin_usage_provider_kimi::{self as kimi, Region, Tokens};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{Candidate, DeviceLoginError, LoginError, LoginPrompt};
use crate::accounts::CandidateIdentity;

#[derive(Serialize, Deserialize)]
struct Stored<'a> {
    region: &'a str,
    client: &'a str,
    subject: &'a str,
    access: &'a str,
    refresh: &'a str,
    expires_at: i64,
}

pub(super) struct Credential {
    region: Region,
    client: String,
    subject: String,
    access: Secret,
    refresh: Secret,
    expires_at: i64,
}

fn region(value: &str) -> Result<Region, ProviderError> {
    match value {
        "global" => Ok(Region::Global),
        "cn" => Ok(Region::Mainland),
        _ => Err(ProviderError::Unsupported("select the global or cn region")),
    }
}

pub(crate) fn region_name(region: Region) -> &'static str {
    match region {
        Region::Global => "global",
        Region::Mainland => "cn",
    }
}

impl Credential {
    pub(super) fn decode(bytes: &SecretBytes) -> Option<Self> {
        let old: Stored<'_> = serde_json::from_slice(bytes.expose()).ok()?;
        Some(Self {
            region: region(old.region).ok()?,
            client: old.client.into(),
            subject: old.subject.into(),
            access: Secret::new(old.access.into()),
            refresh: Secret::new(old.refresh.into()),
            expires_at: old.expires_at,
        })
    }

    fn encode(&self) -> Result<SecretBytes, LoginError> {
        serde_json::to_vec(&Stored {
            region: region_name(self.region),
            client: &self.client,
            subject: &self.subject,
            access: self.access.expose(),
            refresh: self.refresh.expose(),
            expires_at: self.expires_at,
        })
        .map(SecretBytes::new)
        .map_err(|_| LoginError::Encoding)
    }
    fn update(&mut self, tokens: Tokens, now: Timestamp) -> Result<(), LoginError> {
        self.expires_at = now
            .0
            .checked_add(
                i64::try_from(tokens.expires_in.as_secs())
                    .map_err(|_| ProviderError::OUT_OF_RANGE)?,
            )
            .ok_or(ProviderError::OUT_OF_RANGE)?;
        self.access = tokens.access_token;
        self.refresh = tokens.refresh_token;
        Ok(())
    }

    pub(super) async fn renew(
        &mut self,
        http: &Http,
        now: Timestamp,
    ) -> Result<Option<SecretBytes>, LoginError> {
        if self.expires_at > now.0.saturating_add(300) {
            return Ok(None);
        }
        let tokens = kimi::renew(http, self.region, &self.client, &self.refresh).await?;
        let profile = kimi::identify(http, self.region, tokens.access_token.expose()).await?;
        if profile.user_id != self.subject {
            return Err(crate::accounts::ConnectionError::DifferentIdentity.into());
        }
        self.update(tokens, now)?;
        Ok(Some(self.encode()?))
    }

    pub(super) async fn snapshot(&self, http: &Http) -> Result<Value, LoginError> {
        let profile = kimi::identify(http, self.region, self.access.expose()).await?;
        if self.subject != profile.user_id {
            return Err(crate::accounts::ConnectionError::DifferentIdentity.into());
        }
        let mut snap =
            crate::connected::kimi_snapshot(http, self.region, self.access.expose()).await?;
        snap["authMethod"] = "device".into();
        if let Some(plan) = profile.plan {
            snap["plan"] = plan.into();
        }
        Ok(snap)
    }
}

pub(super) async fn connect(
    http: &Http,
    clock: &dyn Clock,
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
            "Kimi registered OAuth client ID",
            false,
            false,
        )
        .await?;
        let id = input.expose().trim();
        if id.is_empty() {
            return Err(ProviderError::Decode("missing Kimi OAuth client ID").into());
        }
        id.to_owned()
    };
    let login = kimi::start_device(http, selected, &client).await?;
    prompts
        .send(LoginPrompt::Device {
            url: login.verification_url.clone(),
            code: Secret::new(login.user_code.clone()),
        })
        .await
        .map_err(|_| LoginError::Cancelled)?;
    let tokens = login
        .wait(http, cancel)
        .await
        .map_err(DeviceLoginError::from)?;
    let profile = kimi::identify(http, selected, tokens.access_token.expose()).await?;
    let mut snapshot =
        crate::connected::kimi_snapshot(http, selected, tokens.access_token.expose()).await?;
    snapshot["authMethod"] = "device".into();
    if let Some(plan) = profile.plan {
        snapshot["plan"] = plan.into();
    }
    let identity = format!("{}/{}", region_name(selected), profile.user_id);
    let expires_at = clock
        .now()
        .0
        .checked_add(
            i64::try_from(tokens.expires_in.as_secs()).map_err(|_| ProviderError::OUT_OF_RANGE)?,
        )
        .ok_or(ProviderError::OUT_OF_RANGE)?;
    let credential = Credential {
        region: selected,
        client,
        subject: profile.user_id,
        access: tokens.access_token,
        refresh: tokens.refresh_token,
        expires_at,
    };
    Ok(Candidate {
        label: profile.nickname.unwrap_or_else(|| "Kimi Code".into()),
        identity: CandidateIdentity::OAuth(Some(identity)),
        material: credential.encode()?,
        snapshot: Ok(snapshot),
    })
}
