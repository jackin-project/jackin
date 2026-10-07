// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Codex` main snapshot view builder.

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_telemetry::ResultTelemetryExt as _;
use jackin_usage_provider_core::{
    ManagedCliLaunchGate, UsageSurface, UsageViewInput, bucket, codex_account_from_value,
    env_dir_or_home, oauth_origin, resolve_identity, split_provider_fetch,
    usage_error_is_unauthorized, usage_view,
};

use super::{
    codex_auth_candidates, codex_oauth_from_value, codex_plan_display_name,
    fetch_codex_oauth_reset_credits, fetch_codex_oauth_usage_refreshing, fetch_codex_rpc_usage,
};

pub fn codex_snapshot(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    rpc_gate: &mut ManagedCliLaunchGate,
) -> FocusedUsageView {
    let codex_home = env_dir_or_home("CODEX_HOME", ".codex");
    // Home auth first, runtime-forwarded handoff last; one walk yields the
    // credential (with its winning path, for the `Auth:` origin) and the account
    // label, reading each file once.
    let codex_candidates = codex_auth_candidates(&codex_home);
    let (resolved, account_from_file) = resolve_identity(
        &codex_candidates,
        codex_oauth_from_value,
        codex_account_from_value,
    );
    let (oauth_path, credentials) = resolved.unzip();
    // account_label is the email identity only; the auth source (the resolver
    // arm that actually won) goes on `credential_origin`.
    let auth_email = credentials
        .as_ref()
        .and_then(|credentials| credentials.account_label.clone())
        .or(account_from_file);
    let has_env_key = std::env::var("OPENAI_API_KEY").is_ok_and(|v| !v.is_empty());
    let needs_login = credentials.is_none() && auth_email.is_none() && !has_env_key;
    let credential_origin = if let Some(path) = oauth_path.as_deref() {
        Some(oauth_origin(path))
    } else if has_env_key {
        Some("API token · env OPENAI_API_KEY".to_owned())
    } else {
        None
    };
    let (rpc_usage, rpc_error) = match fetch_codex_rpc_usage(rpc_gate) {
        Ok(usage) => (Some(usage), None),
        Err(error) => (None, Some(error)),
    };
    let rpc_quota = rpc_usage.as_ref().map(|usage| &usage.response);
    let (oauth_quota, oauth_error) =
        split_provider_fetch(credentials.as_ref().map(|credentials| {
            fetch_codex_oauth_usage_refreshing(credentials, &codex_home).map(|mut usage| {
                usage.reset_credits = fetch_codex_oauth_reset_credits(credentials, &codex_home)
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::HttpError)
                    .ok();
                usage
            })
        }));
    let provider_error = rpc_error.as_ref().or(oauth_error.as_ref()).cloned();
    let auth_error = oauth_error.as_ref().or(rpc_error.as_ref());
    let provider_error_message = provider_error
        .as_ref()
        .map(|error| error.message().to_owned());
    let quota = rpc_quota.or(oauth_quota.as_ref());
    let account = rpc_usage
        .as_ref()
        .and_then(|usage| usage.account_label.clone())
        .or(auth_email)
        .unwrap_or_default();
    let status = if needs_login {
        UsageSnapshotStatus::NeedsLogin
    } else if quota.is_some() {
        UsageSnapshotStatus::Fresh
    } else if auth_error.is_some_and(usage_error_is_unauthorized) {
        // The on-disk token is present but rejected (expired/revoked). Codex
        // refreshes its own token on launch; jackin reads the token as-is, so a
        // stale `auth.json` 401s here. Surface an honest "login" rather than a
        // blank/stale meter — the root cause (no in-process refresh) is named in
        // FINDINGS §9.2 E2.
        UsageSnapshotStatus::NeedsLogin
    } else {
        UsageSnapshotStatus::Stale
    };
    let buckets = quota
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![
                bucket(
                    "Session",
                    None,
                    None,
                    None,
                    None,
                    provider_error_message
                        .as_deref()
                        .or(Some("app-server/OAuth quota pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
                bucket(
                    "Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error_message
                        .as_deref()
                        .or(Some("app-server/OAuth quota pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
                bucket(
                    "Codex Spark 5-hour",
                    None,
                    None,
                    None,
                    None,
                    provider_error_message
                        .as_deref()
                        .or(Some("provider API pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
                bucket(
                    "Codex Spark Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error_message
                        .as_deref()
                        .or(Some("provider API pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
            ]
        });
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Codex,
        account_label: account,
        username: None,
        plan_label: quota
            .and_then(|usage| usage.plan_type.as_deref())
            .and_then(codex_plan_display_name),
        credential_origin,
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            if rpc_quota.is_some() {
                UsageSource::Cli
            } else {
                UsageSource::ProviderApi
            }
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsLogin => {
                Some("Codex auth not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Stale => Some(provider_error_message.unwrap_or_else(|| {
                "Codex provider usage unavailable; cached quota is stale".to_owned()
            })),
            _ => None,
        },
    })
}
