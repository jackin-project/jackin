// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Grok` snapshot entry points and account labels.

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    GROK_HANDOFF_AUTH_PATH, ManagedCliLaunchGate, UsageSurface, UsageViewInput, bucket, env_value,
    first_string_key, home_path, read_json_file, usage_view,
};
use jackin_usage_provider_core::{ProviderError, ProviderRateLimit};
use std::path::{Path, PathBuf};

use super::{GrokBillingAuth, GrokBillingSnapshot, fetch_grok_billing, resolve_grok_billing_auth};

pub(crate) fn grok_snapshot(
    agent: &str,
    now: i64,
    rpc_gate: &mut ManagedCliLaunchGate,
) -> FocusedUsageView {
    let data = home_path(".grok");
    let home_auth = data.join("auth.json");
    let handoff_auth = PathBuf::from(GROK_HANDOFF_AUTH_PATH);
    let home_exists = home_auth.exists();
    let auth = if home_exists { home_auth } else { handoff_auth };
    // `home_exists` short-circuits when home won, so the resolved path is
    // checked at most once.
    let has_auth = home_exists || auth.exists();
    let has_xai_api_key = env_value("XAI_API_KEY").is_some();
    let has_deployment_key = env_value("GROK_DEPLOYMENT_KEY").is_some();
    let billing_result = fetch_grok_billing(&auth, now, rpc_gate);
    // Subscription-over-key precedence: with no stored subscription auth the
    // REST call never went out on an inference key, so replace the confusing
    // file/RPC failure with the honest billing gap.
    let billing_result = match &billing_result {
        Err(_)
            if resolve_grok_billing_auth(has_auth, has_xai_api_key, has_deployment_key)
                == GrokBillingAuth::EnvKeyOnly =>
        {
            Err(ProviderError::from(
                "Grok consumer billing needs subscription auth; XAI_API_KEY is inference-only"
                    .to_owned(),
            ))
        }
        _ => billing_result,
    };
    grok_snapshot_from_rpc_result_with_rate_limit(
        agent,
        now,
        &auth,
        has_auth,
        has_xai_api_key,
        has_deployment_key,
        billing_result,
    )
    .0
}

pub(crate) fn grok_snapshot_from_rpc_result(
    agent: &str,
    now: i64,
    auth: &Path,
    has_auth: bool,
    has_xai_api_key: bool,
    has_deployment_key: bool,
    billing_result: Result<GrokBillingSnapshot, ProviderError>,
) -> FocusedUsageView {
    grok_snapshot_from_rpc_result_with_rate_limit(
        agent,
        now,
        auth,
        has_auth,
        has_xai_api_key,
        has_deployment_key,
        billing_result,
    )
    .0
}

pub(crate) fn grok_snapshot_from_rpc_result_with_rate_limit<E>(
    agent: &str,
    now: i64,
    auth: &Path,
    has_auth: bool,
    has_xai_api_key: bool,
    has_deployment_key: bool,
    billing_result: Result<GrokBillingSnapshot, E>,
) -> (FocusedUsageView, Option<ProviderRateLimit>)
where
    E: Into<ProviderError>,
{
    let has_credentials = has_auth || has_xai_api_key || has_deployment_key;
    let (billing_usage, billing_error, rate_limit) = match billing_result {
        Ok(usage) => (Some(usage), None, None),
        Err(error) => {
            let error = error.into();
            let rate_limit = error.rate_limit();
            (None, Some(error.to_string()), rate_limit)
        }
    };
    // credential_origin reflects the resolver arm that actually won
    // (`auth` is the resolved path — home `~/.grok/auth.json` or the handoff).
    let credential_origin = if has_auth {
        Some(if auth == Path::new(GROK_HANDOFF_AUTH_PATH) {
            format!("OAuth · {GROK_HANDOFF_AUTH_PATH}")
        } else {
            "OAuth · ~/.grok/auth.json".to_owned()
        })
    } else if has_xai_api_key {
        Some("API token · env XAI_API_KEY".to_owned())
    } else if has_deployment_key {
        Some("API token · env GROK_DEPLOYMENT_KEY".to_owned())
    } else {
        None
    };
    let account =
        grok_account_label_or_presence(auth, has_auth, has_xai_api_key, has_deployment_key);
    let status = if billing_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else if has_credentials {
        UsageSnapshotStatus::Error
    } else {
        UsageSnapshotStatus::NeedsLogin
    };
    let buckets = billing_usage
        .as_ref()
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![bucket(
                "Credits",
                None,
                None,
                None,
                None,
                Some("ACP billing unavailable"),
                status,
            )]
        });
    let view = usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Grok,
        account_label: account,
        username: None,
        plan_label: billing_usage
            .as_ref()
            .and_then(GrokBillingSnapshot::plan_label),
        credential_origin,
        buckets,
        status,
        source: billing_usage
            .as_ref()
            .map_or(UsageSource::None, GrokBillingSnapshot::source),
        confidence: if billing_usage.is_some() {
            UsageConfidence::Authoritative
        } else if has_credentials {
            UsageConfidence::PresenceOnly
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsLogin => Some(
                billing_error.unwrap_or_else(|| "Grok auth not available to Capsule".to_owned()),
            ),
            UsageSnapshotStatus::Error => {
                billing_error.or_else(|| Some("Grok billing unavailable to Capsule".to_owned()))
            }
            _ => None,
        },
    });
    (view, rate_limit)
}

pub(crate) fn grok_account_label(path: &Path) -> Option<String> {
    let value = read_json_file(path)?;
    first_string_key(&value, "email")
        .or_else(|| first_string_key(&value, "user_id"))
        .or_else(|| first_string_key(&value, "team_id"))
}

pub(crate) fn grok_account_label_or_presence(
    auth_path: &Path,
    has_auth: bool,
    has_xai_api_key: bool,
    has_deployment_key: bool,
) -> String {
    grok_account_label(auth_path).unwrap_or_else(|| {
        if has_auth {
            "local Grok auth".to_owned()
        } else if has_xai_api_key {
            "XAI_API_KEY present".to_owned()
        } else if has_deployment_key {
            "GROK_DEPLOYMENT_KEY present".to_owned()
        } else {
            "needs Grok login".to_owned()
        }
    })
}
