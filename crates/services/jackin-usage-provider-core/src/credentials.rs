// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential selection and identity resolution.

use std::fs;

use std::path::{Path, PathBuf};

use jackin_core::UsageCredentialOwner;
use jackin_telemetry::ResultTelemetryExt as _;

/// Resolve a credential from an ordered candidate list, returning the first path
/// that yields a usable value via `load` together with that winning path. Used
/// by Amp for its single file credential; the home-first / handoff-last ordering
/// it encodes — the agent's own home location(s) first (the live source of truth
/// the agent reads and refreshes), then the runtime-forwarded `/jackin/<provider>/`
/// handoff as the last-resort fallback — is the same ordering `resolve_identity`
/// applies for the dual-concern providers (Claude, Codex), so credential order is
/// uniform across providers. The winning path is returned so the `Auth:` origin
/// can name the file that actually produced the credential instead of re-`stat`ing
/// and guessing.
pub fn first_credential_with_path<T>(
    paths: &[PathBuf],
    load: impl Fn(&Path) -> Option<T>,
) -> Option<(PathBuf, T)> {
    paths
        .iter()
        .find_map(|path| load(path.as_path()).map(|value| (path.clone(), value)))
}

pub fn first_credential<T>(paths: &[PathBuf], load: impl Fn(&Path) -> Option<T>) -> Option<T> {
    first_credential_with_path(paths, load).map(|(_, value)| value)
}

/// Read and parse a JSON credential/config file, distinguishing expected
/// absence from a present-but-broken typed telemetry error.
pub fn read_json_file(path: &Path) -> Option<serde_json::Value> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        result => result
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
            .ok()?,
    };
    serde_json::from_str(&text)
        .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::ConfigError)
        .ok()
}

/// Resolve a provider credential (with the winning path, for the `Auth:`
/// origin) and its account label in one home-first walk, reading and parsing
/// each candidate file at most once. `extract_credential` pulls the token from a
/// parsed file; `extract_label` pulls the account email/label. The walk stops as
/// soon as both are found, so a later candidate never re-reads a resolved file.
pub fn resolve_identity<T>(
    candidates: &[PathBuf],
    extract_credential: impl Fn(&serde_json::Value) -> Option<T>,
    extract_label: impl Fn(&serde_json::Value) -> Option<String>,
) -> (Option<(PathBuf, T)>, Option<String>) {
    let (result, label, _) =
        resolve_identity_with_extra(candidates, extract_credential, extract_label, |_| {
            None::<String>
        });
    (result, label)
}

/// Like `resolve_identity` but also extracts a third field in the same walk,
/// avoiding a second pass over the candidate files.
pub fn resolve_identity_with_extra<T>(
    candidates: &[PathBuf],
    extract_credential: impl Fn(&serde_json::Value) -> Option<T>,
    extract_label: impl Fn(&serde_json::Value) -> Option<String>,
    extract_extra: impl Fn(&serde_json::Value) -> Option<String>,
) -> (Option<(PathBuf, T)>, Option<String>, Option<String>) {
    let mut credential = None;
    let mut label = None;
    let mut extra = None;
    for path in candidates {
        if credential.is_some() && label.is_some() && extra.is_some() {
            break;
        }
        let Some(value) = read_json_file(path) else {
            continue;
        };
        if credential.is_none()
            && let Some(found) = extract_credential(&value)
        {
            credential = Some((path.clone(), found));
        }
        if label.is_none() {
            label = extract_label(&value);
        }
        if extra.is_none() {
            extra = extract_extra(&value);
        }
    }
    (credential, label, extra)
}

/// Canonical provider usage key for one credential owner.
pub fn canonical_usage_key(owner: UsageCredentialOwner) -> &'static str {
    match owner {
        UsageCredentialOwner::Claude => jackin_core::ANTHROPIC_API_KEY_ENV_NAME,
        UsageCredentialOwner::Codex => jackin_core::OPENAI_API_KEY_ENV_NAME,
        UsageCredentialOwner::Amp => jackin_core::AMP_API_KEY_ENV_NAME,
        UsageCredentialOwner::Kimi => jackin_core::KIMI_CODE_API_KEY_ENV_NAME,
        UsageCredentialOwner::Grok => jackin_core::XAI_API_KEY_ENV_NAME,
        UsageCredentialOwner::Zai => jackin_core::ZAI_API_KEY_ENV_NAME,
        UsageCredentialOwner::Minimax => jackin_core::MINIMAX_API_KEY_ENV_NAME,
        UsageCredentialOwner::OpenCode => jackin_core::OPENCODE_API_KEY_ENV_NAME,
        UsageCredentialOwner::Google => jackin_core::GEMINI_API_KEY_ENV_NAME,
        UsageCredentialOwner::Cursor => jackin_core::CURSOR_API_KEY_ENV_NAME,
        UsageCredentialOwner::Meta => jackin_core::META_API_KEY_ENV_NAME,
        UsageCredentialOwner::OpenRouter => jackin_core::OPENROUTER_API_KEY_ENV_NAME,
    }
}

/// Normalize launch aliases to the provider route that controls refresh
/// semantics. API-key aliases share their owner's route; OAuth and Grok
/// deployment credentials remain distinct because the provider adapter treats
/// them differently.
pub fn dispatch_key_for_route(owner: UsageCredentialOwner, governed_name: &str) -> &'static str {
    match owner {
        UsageCredentialOwner::Claude
            if governed_name == jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME =>
        {
            jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME
        }
        UsageCredentialOwner::Grok
            if governed_name == jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME =>
        {
            jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME
        }
        _ => canonical_usage_key(owner),
    }
}
