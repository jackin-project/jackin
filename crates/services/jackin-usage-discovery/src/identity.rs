// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Profile identity resolution.

use crate::{
    ProfileCredentialMaterial, ProfileCredentialReader, ProfileReadOutcome, ProfileValidation,
    amp_profile_identity, antigravity_profile_identity, claude_profile_identity,
    codex_profile_identity, cursor_profile_identity, gemini_profile_identity,
    grok_profile_identity, muse_profile_identity, opencode_profile_identity, read_json,
};

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use jackin_core::Agent;

/// Return an opaque revision for the complete credential material read for a
/// profile source. The path-derived source id is intentionally not enough:
/// providers frequently rotate tokens in place without changing the profile
/// path or account identity.
pub(crate) fn profile_credential_revision(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
    operator_home: &Path,
) -> String {
    let mut evidence = Vec::new();
    let mut file = |label: &str, path: PathBuf| {
        append_profile_read(&mut evidence, label, reader.read(&path));
    };
    match agent {
        Agent::Claude => {
            file("claude.credentials", root.join(".credentials.json"));
            file("claude.config", root.join(".claude.json"));
            if root == operator_home.join(".claude") {
                file("claude.home-config", operator_home.join(".claude.json"));
            }
            if let Some(scope) =
                jackin_core::claude_keychain_scope(root, operator_home, operator_home)
            {
                append_profile_read(
                    &mut evidence,
                    "claude.keychain",
                    reader.read_claude_keychain(&scope),
                );
            }
        }
        Agent::Codex => file("codex.auth", root.join("auth.json")),
        Agent::Amp => {
            let direct = root.join("secrets.json");
            let path = if reader.exists(&direct) {
                direct
            } else {
                root.join("data/amp/secrets.json")
            };
            file("amp.secrets", path);
        }
        Agent::Kimi => file("kimi.credentials", root.join("credentials/kimi-code.json")),
        Agent::Grok => file("grok.auth", root.join("auth.json")),
        Agent::Opencode => file("opencode.auth", root.join("auth.json")),
        Agent::Antigravity => append_profile_read(
            &mut evidence,
            "antigravity.keychain",
            reader.read_antigravity_keychain(),
        ),
        Agent::Gemini => file("gemini.oauth", root.join("oauth_creds.json")),
        Agent::Cursor => {
            file("cursor.auth", root.join("auth.json"));
            file("cursor.config", root.join("cli-config.json"));
        }
        Agent::Muse => file("muse.auth", root.join("auth.json")),
        Agent::Omp => file("omp.database", root.join("agent/agent.db")),
        Agent::Hermes => file("hermes.auth", root.join("auth.json")),
    }
    opaque_credential_revision(&evidence.join("|"))
}

pub(crate) fn append_profile_read(
    evidence: &mut Vec<String>,
    label: &str,
    outcome: ProfileReadOutcome,
) {
    match outcome {
        ProfileReadOutcome::Bytes(bytes) => {
            let mut hex = String::with_capacity(bytes.len().saturating_mul(2));
            for byte in &bytes {
                let _ignored = write!(hex, "{byte:02x}");
            }
            evidence.push(format!("{label}:bytes:{}:{hex}", bytes.len()));
        }
        ProfileReadOutcome::Missing => evidence.push(format!("{label}:missing")),
        ProfileReadOutcome::Denied => evidence.push(format!("{label}:denied")),
        ProfileReadOutcome::ConsentRequired => {
            evidence.push(format!("{label}:consent-required"));
        }
    }
}

pub(crate) fn opaque_credential_revision(evidence: &str) -> String {
    let hashed = jackin_core::account_key_hash("usage-credential-material-v2", evidence);
    hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned()
}

pub(crate) fn profile_identity(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
    operator_home: &Path,
) -> ProfileValidation {
    match agent {
        Agent::Claude => claude_profile_identity(reader, root, operator_home),
        Agent::Codex => codex_profile_identity(reader, &root.join("auth.json")),
        Agent::Amp => {
            let direct = root.join("secrets.json");
            let path = if reader.exists(&direct) {
                direct
            } else {
                root.join("data/amp/secrets.json")
            };
            amp_profile_identity(reader, &path)
        }
        Agent::Kimi => {
            let value = match read_json(reader, &root.join("credentials/kimi-code.json")) {
                Ok(Some(value)) => value,
                Ok(None) => return ProfileValidation::Missing,
                Err(outcome) => return outcome,
            };
            jackin_usage_provider_kimi::kimi_local_token_from_value(
                &value,
                chrono::Utc::now().timestamp(),
            )
            .map_or(ProfileValidation::Malformed, |token| {
                ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::Kimi {
                    token,
                })))
            })
        }
        Agent::Grok => grok_profile_identity(reader, &root.join("auth.json")),
        Agent::Opencode => opencode_profile_identity(reader, &root.join("auth.json")),
        // Antigravity wires through the host Keychain grant singleton: the
        // CLI owns the secret, so grant presence alone mints refresh
        // material and refresh shells out to `agy`.
        Agent::Antigravity => antigravity_profile_identity(reader),
        Agent::Gemini => gemini_profile_identity(reader, &root.join("oauth_creds.json")),
        Agent::Cursor => cursor_profile_identity(reader, root),
        // Muse stays explicitly unwired: identity is verified locally but no
        // material is minted — the secret lives in the platform credential
        // store and no pollable usage fetch exists by design
        // (`MuseKeyExchangePolicy::polling_enabled` is false), so refresh
        // cannot dispatch.
        Agent::Muse => muse_profile_identity(reader, &root.join("auth.json")),
        // omp stays explicitly unwired: it is an attribution-only aggregator
        // with no native identity or usage endpoint. SQLite store presence
        // (not content) is verified; table parsing belongs to a later lane.
        Agent::Omp => {
            if reader.exists(&root.join("agent/agent.db")) {
                ProfileValidation::Anonymous(None)
            } else {
                ProfileValidation::Missing
            }
        }
        // Hermes stays explicitly unwired: attribution-only adapter with no
        // Hermes-native quota API; usage needs caller-supplied underlying
        // buckets the refresh lane cannot produce.
        Agent::Hermes => anonymous_when_present(reader, &root.join("auth.json")),
    }
}

/// File present (any JSON shape) → anonymous binding; missing/denied/
/// malformed propagate truthfully. Used for agents whose identity
/// extraction is deferred to the usage lane.
pub(crate) fn anonymous_when_present(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> ProfileValidation {
    match read_json(reader, path) {
        Ok(Some(_)) => ProfileValidation::Anonymous(None),
        Ok(None) => ProfileValidation::Missing,
        Err(outcome) => outcome,
    }
}
