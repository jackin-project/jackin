// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{CredentialSource, Provider, Timestamp};
use jackin_usage_provider_claude as claude;
use jackin_usage_provider_codex as codex;
use jackin_usage_provider_core::{Bucket, Http, ProviderError};
use jackin_usage_provider_cursor as cursor;
use jackin_usage_provider_gemini as gemini;
use jackin_usage_provider_grok as grok;
use jackin_usage_provider_kimi as kimi;
use jackin_usage_provider_minimax as minimax;
use jackin_usage_provider_opencode as opencode;
use jackin_usage_provider_openrouter as openrouter;
use jackin_usage_provider_zai as zai;

use crate::credential::{Material, Shape};
use crate::model::UnavailableReason;
use crate::request::UsageAccount;

pub(crate) struct Call<'a> {
    pub(crate) account: &'a UsageAccount,
    pub(crate) http: &'a Http,
    pub(crate) now: i64,
    pub(crate) material: &'a Material,
}

pub(crate) struct Failure {
    pub(crate) reason: UnavailableReason,
    pub(crate) transient: bool,
}

impl From<UnavailableReason> for Failure {
    fn from(reason: UnavailableReason) -> Failure {
        Failure {
            reason,
            transient: false,
        }
    }
}

type Collected = Result<Vec<Bucket>, Failure>;

pub(crate) fn unsupported_without_credentials(account: &UsageAccount) -> Option<UnavailableReason> {
    match account.provider {
        Provider::Amp
        | Provider::Antigravity
        | Provider::Hermes
        | Provider::Mimo
        | Provider::Muse => Some(unsupported(None)),
        Provider::Codex => match &account.credential {
            CredentialSource::CodexLogin { .. } => None,
            _ => Some(unsupported(Some(codex::API_KEY_REASON))),
        },
        _ => None,
    }
}

pub(crate) async fn collect(call: &Call<'_>) -> Collected {
    match call.account.provider {
        Provider::Claude => claude_usage(call).await,
        Provider::Codex => codex_usage(call).await,
        Provider::Cursor => cursor_usage(call).await,
        Provider::Gemini => gemini_usage(call).await,
        Provider::Grok => grok_usage(call).await,
        Provider::Kimi => kimi_usage(call).await,
        Provider::Minimax => minimax_usage(call).await,
        Provider::Opencode => opencode_usage(call).await,
        Provider::Openrouter => openrouter_usage(call).await,
        Provider::Zai => zai_usage(call).await,
        Provider::Amp
        | Provider::Antigravity
        | Provider::Hermes
        | Provider::Mimo
        | Provider::Muse => Err(unsupported(None).into()),
    }
}

async fn codex_usage(call: &Call<'_>) -> Collected {
    let CredentialSource::CodexLogin {
        chatgpt_account_id, ..
    } = &call.account.credential
    else {
        return Err(unsupported(Some(codex::API_KEY_REASON)).into());
    };
    if call.material.shape != Shape::Login {
        return Err(
            UnavailableReason::Provider("the Codex login is not a login file".to_owned()).into(),
        );
    }
    codex::fetch(
        call.http,
        call.material.secret.expose(),
        chatgpt_account_id.as_str(),
        call.now,
    )
    .await
    .map_err(|error| failure(error, call.now))
}

async fn claude_usage(call: &Call<'_>) -> Collected {
    claude::fetch(call.http, call.material.credential(), call.now)
        .await
        .map_err(|error| failure(error, call.now))
}

async fn cursor_usage(call: &Call<'_>) -> Collected {
    cursor::fetch(call.http, call.material.credential())
        .await
        .map_err(|error| failure(error, call.now))
}

async fn gemini_usage(call: &Call<'_>) -> Collected {
    gemini::fetch(call.http, call.material.credential(), call.now)
        .await
        .map_err(|error| failure(error, call.now))
}

async fn grok_usage(call: &Call<'_>) -> Collected {
    grok::fetch(call.http, call.material.credential(), call.now)
        .await
        .map_err(|error| failure(error, call.now))
}

async fn kimi_usage(call: &Call<'_>) -> Collected {
    let base = call
        .material
        .base
        .as_deref()
        .unwrap_or(kimi::MAINLAND_CODING_BASE);
    kimi::fetch(call.http, call.material.credential(), base, call.now)
        .await
        .map_err(|error| failure(error, call.now))
}

async fn zai_usage(call: &Call<'_>) -> Collected {
    let key = token(call)?;
    zai::fetch(call.http, key.secret.expose())
        .await
        .map_err(|error| failure(error, call.now))
}

async fn minimax_usage(call: &Call<'_>) -> Collected {
    minimax::fetch(call.http, call.material.credential(), call.now)
        .await
        .map_err(|error| failure(error, call.now))
}

async fn opencode_usage(call: &Call<'_>) -> Collected {
    opencode::fetch(call.http, call.material.credential(), call.now)
        .await
        .map_err(|error| failure(error, call.now))
}

async fn openrouter_usage(call: &Call<'_>) -> Collected {
    let key = token(call)?;
    openrouter::fetch(call.http, key.secret.expose())
        .await
        .map_err(|error| failure(error, call.now))
}

fn token<'a>(call: &Call<'a>) -> Result<&'a Material, UnavailableReason> {
    if call.material.shape == Shape::Login {
        return Err(UnavailableReason::Provider(
            "this provider does not accept a login file".to_owned(),
        ));
    }
    Ok(call.material)
}

fn unsupported(detail: Option<&str>) -> UnavailableReason {
    UnavailableReason::Unsupported {
        detail: detail.map(str::to_owned),
    }
}

fn failure(error: ProviderError, now: i64) -> Failure {
    let transient = matches!(
        error,
        ProviderError::RateLimited { .. }
            | ProviderError::Network(_)
            | ProviderError::Status(500..=599)
    );
    Failure {
        reason: reason(error, now),
        transient,
    }
}

fn reason(error: ProviderError, now: i64) -> UnavailableReason {
    let text = error.to_string();
    match error {
        ProviderError::Unauthorized => UnavailableReason::ExpiredLogin,
        ProviderError::Forbidden => UnavailableReason::Provider(text),
        ProviderError::RateLimited {
            retry_after_seconds,
        } => UnavailableReason::RateLimited {
            retry_at: retry_after_seconds
                .and_then(|seconds| i64::try_from(seconds).ok())
                .and_then(|seconds| now.checked_add(seconds))
                .map(Timestamp),
        },
        ProviderError::Network(message) => UnavailableReason::Network(message.to_owned()),
        ProviderError::Status(_) | ProviderError::Decode(_) => UnavailableReason::Provider(text),
        ProviderError::Unsupported(detail) => unsupported(Some(detail)),
    }
}
