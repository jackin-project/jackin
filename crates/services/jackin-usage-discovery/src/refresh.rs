// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential binding refresh.

use crate::{
    ProfileCredentialMaterial, ProfileCredentialReader, ProfileReadOutcome, ProfileValidation,
    ValidatedCredentialBinding, ValidatedCredentialSource,
};
use jackin_usage_host_credentials::{
    ProviderCredentialEnvResolver, ProviderCredentialRefreshOutcome,
};

use std::path::Path;

pub(crate) fn read_json(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> Result<Option<serde_json::Value>, ProfileValidation> {
    match reader.read(path) {
        ProfileReadOutcome::Bytes(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| ProfileValidation::Malformed),
        ProfileReadOutcome::Missing => Ok(None),
        ProfileReadOutcome::Denied => Err(ProfileValidation::Denied),
        ProfileReadOutcome::ConsentRequired => Err(ProfileValidation::ConsentRequired),
    }
}

pub(crate) fn first_recursive_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            for key in keys {
                if let Some(found) = map
                    .get(*key)
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|found| !found.is_empty())
                {
                    return Some(found.to_owned());
                }
            }
            map.values()
                .find_map(|nested| first_recursive_string(nested, keys))
        }
        serde_json::Value::Array(values) => values
            .iter()
            .find_map(|nested| first_recursive_string(nested, keys)),
        _ => None,
    }
}

pub fn refresh_credential_binding(
    binding: &ValidatedCredentialBinding,
    env_resolver: &dyn ProviderCredentialEnvResolver,
) -> ProviderCredentialRefreshOutcome {
    let mut provider_error = None;
    let (view, rate_limit) = match &binding.source {
        ValidatedCredentialSource::Env {
            handle,
            dispatch_key,
            ..
        } => {
            return env_resolver.refresh_provider_credential(binding.surface, dispatch_key, handle);
        }
        ValidatedCredentialSource::Capability => {
            return ProviderCredentialRefreshOutcome::Malformed;
        }
        // Deliberate no-poll, never a provider outage: the honest
        // `Unsupported` view flows through the success path, outside
        // retry/backoff.
        ValidatedCredentialSource::Unpollable => (
            jackin_usage_provider_core::unpollable_snapshot(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Claude(resolved)) => {
            let (view, rate_limit, failure) = jackin_usage_provider_claude::claude_view_from_wave(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
                jackin_usage_provider_claude::ClaudeWaveResolution::Resolved(Box::new(
                    resolved.clone(),
                )),
            );
            provider_error = failure;
            (view, rate_limit)
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Codex {
            credentials,
            root,
        }) => jackin_usage_provider_codex::codex_profile_snapshot_with_rate_limit(
            binding.surface.agent_slug(),
            credentials,
            root,
            chrono::Utc::now().timestamp(),
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Amp { key }) => (
            jackin_usage_provider_amp::amp_api_key_snapshot(
                binding.surface.agent_slug(),
                key,
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Grok { auth_path }) => {
            let now = chrono::Utc::now().timestamp();
            let result = jackin_usage_provider_grok::fetch_grok_rest_billing(auth_path, now).map(
                |response| {
                    jackin_usage_provider_grok::GrokBillingSnapshot::Rest(Box::new(response))
                },
            );
            jackin_usage_provider_grok::grok_snapshot_from_rpc_result_with_rate_limit(
                binding.surface.agent_slug(),
                now,
                auth_path,
                true,
                false,
                false,
                result,
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Kimi { token }) => {
            let now = chrono::Utc::now().timestamp();
            (
                jackin_usage_provider_kimi::kimi_snapshot(
                    binding.surface.agent_slug(),
                    Some(token.as_str()),
                    now,
                ),
                None,
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::OpenCode { auth_path }) => (
            jackin_usage_provider_opencode::opencode_profile_snapshot(
                binding.surface.agent_slug(),
                auth_path,
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Cursor { auth_path }) => (
            jackin_usage_provider_cursor::cursor_profile_snapshot(
                binding.surface.agent_slug(),
                auth_path,
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Gemini { creds_path }) => {
            // Re-prove OAuth presence at refresh: a file deleted after
            // discovery is NeedsSecret, never a stale Unsupported.
            let has_oauth = creds_path.is_file();
            (
                jackin_usage_provider_gemini::gemini_snapshot_with_presence(
                    binding.surface.agent_slug(),
                    binding.surface.provider_label(),
                    has_oauth,
                    false,
                    "OAuth · configured profile",
                    chrono::Utc::now().timestamp(),
                ),
                None,
            )
        }
        // The Keychain grant needs no secret material here: `agy` owns the
        // grant and the collector shells out to it.
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Antigravity) => (
            jackin_usage_provider_antigravity::antigravity_snapshot(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
    };
    ProviderCredentialRefreshOutcome::Snapshot {
        view: Box::new(view),
        rate_limit,
        provider_error,
    }
}
