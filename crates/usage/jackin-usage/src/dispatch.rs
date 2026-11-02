// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{CredentialSource, Provider, Timestamp};
use jackin_usage_provider_claude as claude;
use jackin_usage_provider_codex as codex;
use jackin_usage_provider_core::{Bucket, Http, ProviderError};
use jackin_usage_provider_gemini as gemini;
use jackin_usage_provider_kimi as kimi;
use jackin_usage_provider_minimax as minimax;
use jackin_usage_provider_openrouter as openrouter;
use jackin_usage_provider_zai as zai;
use tokio_util::sync::CancellationToken;

use crate::credential::{Material, Shape, Unresolved, resolve};
use crate::deps::UsageDeps;
use crate::model::UnavailableReason;
use crate::request::UsageAccount;

pub(crate) struct Call<'a> {
    pub(crate) deps: &'a UsageDeps,
    pub(crate) account: &'a UsageAccount,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) http: &'a Http,
    pub(crate) now: i64,
}

type Collected = Result<Vec<Bucket>, UnavailableReason>;

pub(crate) async fn collect(call: &Call<'_>) -> Collected {
    match call.account.provider {
        Provider::Claude => claude_usage(call).await,
        Provider::Codex => codex_usage(call).await,
        Provider::Gemini => gemini_usage(call).await,
        Provider::Kimi => kimi_usage(call).await,
        Provider::Minimax => minimax_usage(call).await,
        Provider::Openrouter => openrouter_usage(call).await,
        Provider::Zai => zai_usage(call).await,
        Provider::Amp
        | Provider::Antigravity
        | Provider::Cursor
        | Provider::Grok
        | Provider::Hermes
        | Provider::Muse
        | Provider::Opencode => Err(unsupported(None)),
    }
}

async fn codex_usage(call: &Call<'_>) -> Collected {
    let CredentialSource::CodexLogin {
        chatgpt_account_id, ..
    } = &call.account.credential
    else {
        return Err(unsupported(Some(codex::API_KEY_REASON)));
    };
    let login = call.material().await?;
    if login.shape != Shape::Login {
        return Err(UnavailableReason::Provider(
            "the Codex login is not a login file".to_owned(),
        ));
    }
    codex::fetch(
        call.http,
        login.secret.expose(),
        chatgpt_account_id.as_str(),
        call.now,
    )
    .await
    .map_err(|error| reason(error, call.now))
}

async fn claude_usage(call: &Call<'_>) -> Collected {
    let material = call.material().await?;
    claude::fetch(call.http, material.credential())
        .await
        .map_err(|error| reason(error, call.now))
}

async fn gemini_usage(call: &Call<'_>) -> Collected {
    let material = call.material().await?;
    gemini::fetch(call.http, material.credential(), call.now)
        .await
        .map_err(|error| reason(error, call.now))
}

async fn kimi_usage(call: &Call<'_>) -> Collected {
    let material = call.material().await?;
    kimi::fetch(call.http, material.credential(), call.now)
        .await
        .map_err(|error| reason(error, call.now))
}

async fn zai_usage(call: &Call<'_>) -> Collected {
    let key = call.token().await?;
    zai::fetch(call.http, key.secret.expose())
        .await
        .map_err(|error| reason(error, call.now))
}

async fn minimax_usage(call: &Call<'_>) -> Collected {
    let key = call.token().await?;
    minimax::fetch(call.http, key.secret.expose(), call.now)
        .await
        .map_err(|error| reason(error, call.now))
}

async fn openrouter_usage(call: &Call<'_>) -> Collected {
    let key = call.token().await?;
    openrouter::fetch(call.http, key.secret.expose())
        .await
        .map_err(|error| reason(error, call.now))
}

impl Call<'_> {
    async fn token(&self) -> Result<Material, UnavailableReason> {
        let material = self.material().await?;
        if material.shape == Shape::Login {
            return Err(UnavailableReason::Provider(
                "this provider does not accept a login file".to_owned(),
            ));
        }
        Ok(material)
    }

    async fn material(&self) -> Result<Material, UnavailableReason> {
        resolve(self.deps, self.account, self.cancel)
            .await
            .map_err(|unresolved| match unresolved {
                Unresolved::Missing => UnavailableReason::NoCredentials,
                Unresolved::Unreadable(message) => UnavailableReason::Provider(message.to_owned()),
            })
    }
}

fn unsupported(detail: Option<&str>) -> UnavailableReason {
    UnavailableReason::Unsupported {
        detail: detail.map(str::to_owned),
    }
}

fn reason(error: ProviderError, now: i64) -> UnavailableReason {
    match error {
        ProviderError::Unauthorized | ProviderError::Forbidden => UnavailableReason::ExpiredLogin,
        ProviderError::RateLimited {
            retry_after_seconds,
        } => UnavailableReason::RateLimited {
            retry_at: retry_after_seconds
                .and_then(|seconds| i64::try_from(seconds).ok())
                .map(|seconds| Timestamp(now.saturating_add(seconds))),
        },
        ProviderError::Network(message) => UnavailableReason::Network(message.to_owned()),
        ProviderError::Status(_) | ProviderError::Decode(_) => {
            UnavailableReason::Provider(error.to_string())
        }
        ProviderError::Unsupported(detail) => unsupported(Some(detail)),
    }
}
