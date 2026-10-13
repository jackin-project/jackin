// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Amp` snapshot entry points.

use super::{
    AmpSuccessContext, amp_view_from_usage, fetch_amp_api_usage, fetch_amp_cli_usage,
    load_amp_api_key,
};
use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    AMP_HANDOFF_SECRETS_PATH, UsageSurface, UsageViewInput, bucket, env_value,
    first_credential_with_path, home_path, split_fetch, usage_view,
};
use std::path::Path;

pub fn amp_snapshot(agent: &str, now: i64) -> FocusedUsageView {
    let data = home_path(".local/share/amp");
    let amp_secrets = data.join("secrets.json");
    let handoff_secrets = Path::new(AMP_HANDOFF_SECRETS_PATH);
    let amp_env_key = env_value("AMP_API_KEY");
    let env_present = amp_env_key.is_some();
    // Resolve the file key only when the env var is absent (env wins), capturing
    // the winning path so the origin names the file that actually produced the
    // key instead of re-`stat`ing and guessing. Home secrets first, handoff last.
    let amp_file = if env_present {
        None
    } else {
        first_credential_with_path(
            &[amp_secrets.clone(), handoff_secrets.to_path_buf()],
            load_amp_api_key,
        )
    };
    let amp_api_key = amp_env_key
        .clone()
        .or_else(|| amp_file.as_ref().map(|(_, key)| key.clone()));
    let (api_usage, api_error) = split_fetch(amp_api_key.as_deref().map(fetch_amp_api_usage));
    let (cli_usage, cli_error) = split_fetch(api_usage.is_none().then(fetch_amp_cli_usage));
    let provider_error = api_error.as_ref().or(cli_error.as_ref()).cloned();
    let has_auth = amp_api_key.is_some() || amp_secrets.exists() || handoff_secrets.exists();
    // credential_origin names the file that actually produced the key (env wins
    // first). A present-but-unparseable home `secrets.json` no longer mislabels a
    // key that actually resolved from the handoff.
    let credential_origin = if env_present {
        Some("API key · env AMP_API_KEY".to_owned())
    } else if let Some((path, _)) = amp_file.as_ref() {
        Some(if path.as_path() == handoff_secrets {
            format!("API key · {AMP_HANDOFF_SECRETS_PATH}")
        } else {
            "API key · amp secrets.json".to_owned()
        })
    } else {
        None
    };

    // Success path is one shared, credential-free view builder so the plan-label
    // and detail-only credit contract is executable without provider I/O.
    if let Some(usage) = api_usage {
        return amp_view_from_usage(
            AmpSuccessContext {
                agent,
                credential_origin,
                source: UsageSource::ProviderApi,
            },
            usage,
            now,
        );
    }
    if let Some(usage) = cli_usage {
        return amp_view_from_usage(
            AmpSuccessContext {
                agent,
                credential_origin,
                source: UsageSource::Cli,
            },
            usage,
            now,
        );
    }

    let status = if has_auth {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsLogin
    };
    let account_label = if has_auth {
        "local Amp auth".to_owned()
    } else {
        "needs Amp login".to_owned()
    };
    let buckets = vec![bucket(
        "Amp Free",
        None,
        None,
        None,
        None,
        provider_error
            .as_deref()
            .or(Some("Amp API/CLI usage unavailable")),
        status,
    )];
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Amp,
        account_label,
        username: None,
        plan_label: None,
        credential_origin,
        buckets,
        status,
        source: UsageSource::None,
        confidence: if has_auth {
            UsageConfidence::PresenceOnly
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsLogin => Some("Amp auth not available to Capsule".to_owned()),
            UsageSnapshotStatus::Unsupported => Some(
                provider_error
                    .unwrap_or_else(|| "Amp API/CLI usage unavailable to Capsule".to_owned()),
            ),
            _ => None,
        },
    })
}

pub fn amp_api_key_snapshot(agent: &str, key: &str, now: i64) -> FocusedUsageView {
    match fetch_amp_api_usage(key) {
        Ok(usage) => amp_view_from_usage(
            AmpSuccessContext {
                agent,
                credential_origin: Some("API key · configured source".to_owned()),
                source: UsageSource::ProviderApi,
            },
            usage,
            now,
        ),
        Err(error) => usage_view(UsageViewInput {
            agent,
            provider: None,
            surface: UsageSurface::Amp,
            account_label: String::new(),
            username: None,
            plan_label: None,
            credential_origin: Some("API key · configured source".to_owned()),
            buckets: vec![bucket(
                "Amp Free",
                None,
                None,
                None,
                None,
                Some(&error),
                UsageSnapshotStatus::Error,
            )],
            status: UsageSnapshotStatus::Error,
            source: UsageSource::None,
            confidence: UsageConfidence::None,
            now,
            last_error: Some(error),
        }),
    }
}
