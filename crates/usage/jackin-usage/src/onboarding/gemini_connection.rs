// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Clock, Secret, SecretBytes, Timestamp};
use jackin_usage_provider_core::{Http, ProviderError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::{Candidate, LoginError, LoginPrompt, open_browser};
use crate::accounts::CandidateIdentity;
use crate::gemini_login::{self, DesktopClient};
use crate::grants::Tokens;

pub(super) struct Credential {
    client: DesktopClient,
    tokens: Tokens,
    project: String,
}

#[derive(Serialize, Deserialize)]
struct Stored<'a> {
    client_id: &'a str,
    client_secret: Option<&'a str>,
    project: &'a str,
    access: &'a str,
    refresh: Option<&'a str>,
    expires_at: Option<i64>,
}

fn secret(text: &str) -> Secret {
    Secret::new(text.to_owned())
}

impl Credential {
    pub(super) fn decode(bytes: &SecretBytes) -> Result<Self, LoginError> {
        let old: Stored<'_> =
            serde_json::from_slice(bytes.expose()).map_err(|_| LoginError::Encoding)?;
        Ok(Self {
            client: DesktopClient::new(old.client_id.to_owned(), old.client_secret.map(secret))?,
            tokens: Tokens {
                access: secret(old.access),
                refresh: old.refresh.map(secret),
                expires_at: old.expires_at.map(Timestamp),
            },
            project: old.project.to_owned(),
        })
    }

    fn encode(&self) -> Result<SecretBytes, LoginError> {
        let stored = Stored {
            client_id: &self.client.id,
            client_secret: self.client.secret.as_ref().map(Secret::expose),
            project: &self.project,
            access: self.tokens.access.expose(),
            refresh: self.tokens.refresh.as_ref().map(Secret::expose),
            expires_at: self.tokens.expires_at.map(|t| t.0),
        };
        serde_json::to_vec(&stored)
            .map(SecretBytes::new)
            .map_err(|_| LoginError::Encoding)
    }

    pub(super) async fn renew(
        &mut self,
        http: &Http,
        now: Timestamp,
    ) -> Result<Option<SecretBytes>, LoginError> {
        if self
            .tokens
            .expires_at
            .is_none_or(|at| at.0 > now.0.saturating_add(300))
        {
            return Ok(None);
        }
        let refresh = self
            .tokens
            .refresh
            .as_ref()
            .ok_or(crate::grants::GrantError::SignInAgain)?;
        self.tokens = gemini_login::renew(http, &self.client, refresh, now).await?;
        Ok(Some(self.encode()?))
    }

    pub(super) async fn snapshot(&self, http: &Http) -> Result<Value, LoginError> {
        Ok(gemini_login::snapshot(http, &self.tokens.access, &self.project).await?)
    }
}

pub(super) async fn field(
    prompts: &mpsc::Sender<LoginPrompt>,
    label: &'static str,
    optional: bool,
    masked: bool,
) -> Result<Secret, LoginError> {
    let (reply, input) = oneshot::channel();
    prompts
        .send(LoginPrompt::Field {
            label,
            optional,
            masked,
            reply,
        })
        .await
        .map_err(|_| LoginError::Cancelled)?;
    input.await.map_err(|_| LoginError::Cancelled)
}

pub(super) async fn connect(
    http: &Http,
    clock: &dyn Clock,
    existing: Option<&SecretBytes>,
    prompts: mpsc::Sender<LoginPrompt>,
    cancel: &CancellationToken,
) -> Result<Candidate, LoginError> {
    let (client, project) =
        if let Some(old) = existing.and_then(|bytes| Credential::decode(bytes).ok()) {
            (old.client, Some(old.project))
        } else {
            let id = field(&prompts, "Google desktop OAuth client ID", false, false).await?;
            let secret = field(&prompts, "Google desktop OAuth client secret", true, true).await?;
            let project = field(
                &prompts,
                "Google Cloud project ID (leave empty for a managed project)",
                true,
                false,
            )
            .await?;
            (
                DesktopClient::new(
                    id.expose().trim().to_owned(),
                    (!secret.expose().is_empty()).then_some(secret),
                )?,
                (!project.expose().trim().is_empty()).then(|| project.expose().trim().to_owned()),
            )
        };
    let login = gemini_login::start(&client).await?;
    let url = login.authorization_url().clone();
    prompts
        .send(LoginPrompt::Callback { url: url.clone() })
        .await
        .map_err(|_| LoginError::Cancelled)?;
    if open_browser(&url).await.is_err() {
        prompts
            .send(LoginPrompt::BrowserUnavailable)
            .await
            .map_err(|_| LoginError::Cancelled)?;
    }
    let grant = login.wait_callback(cancel).await?;
    let signed =
        gemini_login::exchange(http, &client, grant, project.as_deref(), clock.now()).await?;
    let project = signed
        .organization
        .ok_or(ProviderError::Decode("missing Google project ID"))?;
    let credential = Credential {
        client,
        tokens: signed.tokens,
        project,
    };
    Ok(Candidate {
        label: signed.label,
        identity: CandidateIdentity::OAuth(signed.identity),
        material: credential.encode()?,
        snapshot: Ok(credential.snapshot(http).await?),
    })
}
