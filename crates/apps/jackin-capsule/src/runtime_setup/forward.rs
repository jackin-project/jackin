// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Forwarded credential seeding and application.

use super::{
    AuthMaterialization, AuthMode, copy_file_with_mode, nonempty_env, remove_file_if_exists,
    seed_agent_home_from_enum,
};

use anyhow::Result;
use std::path::Path;

/// One agent's host-forwarded single-file credential, seeded into its
/// in-container config dir. The capsule analogue of the host-side
/// `provision_single_file_credential` (see `jackin-runtime` `instance/auth.rs`):
/// both ends of the host→container sync read the same shape so a reader does not
/// have to relearn each agent.
pub(crate) struct ForwardedCredential<'a> {
    /// Short log label, e.g. `"codex"`.
    pub(crate) label: &'a str,
    /// Mounted host credential inside the container (`/jackin/<agent>/<file>`).
    pub(crate) forwarded: &'a Path,
    /// In-container destination, already resolved to honor the agent's
    /// config-dir env var (`CODEX_HOME`, `XDG_DATA_HOME`, …) where it has one.
    pub(crate) target: &'a Path,
    /// API-key env vars that are an alternative to forwarded auth; any one set
    /// suppresses the "needs interactive login" warning.
    pub(crate) api_key_envs: &'a [&'a str],
}

/// Seed a host-forwarded credential into an agent's config dir with one uniform
/// policy for every sync-capable agent:
///
/// - **First seed**: copy the forwarded file when present; otherwise clear any
///   stale destination and warn — unless an API-key env var supplies auth.
/// - **Later launches**: if the destination is missing but the forwarded file is
///   present, re-seed (the first seed raced the host login). Guarded on
///   `!target.exists()` so a token the agent refreshed in-container is never
///   clobbered.
pub(crate) fn seed_forwarded_credential(
    agent: jackin_core::Agent,
    mode: AuthMode,
    seed_base: &Path,
    spec: &ForwardedCredential<'_>,
) -> Result<AuthMaterialization> {
    let first_seed = seed_agent_home_from_enum(agent, seed_base)?.is_first_seed();
    apply_forwarded_credential(first_seed, mode, spec)
}

/// The credential-seeding policy, decoupled from the home-seed signal so a
/// multi-file agent (Claude: credentials.json + account.json) can seed its home
/// once and apply this same policy to each file under that one `first_seed`.
pub(crate) fn apply_forwarded_credential(
    first_seed: bool,
    mode: AuthMode,
    spec: &ForwardedCredential<'_>,
) -> Result<AuthMaterialization> {
    use jackin_telemetry::schema::enums::{
        CredentialSourceType as Source, ErrorType, OutcomeValue as Outcome,
    };
    if matches!(mode, AuthMode::Ignore) {
        remove_file_if_exists(spec.target)?;
        return Ok(AuthMaterialization {
            source: Source::None,
            outcome: Outcome::Skip,
            error: None,
        });
    }
    if matches!(mode, AuthMode::ApiKey | AuthMode::OauthToken) {
        remove_file_if_exists(spec.target)?;
        let available = spec
            .api_key_envs
            .iter()
            .any(|key| nonempty_env(key).is_some());
        return Ok(AuthMaterialization {
            source: if available {
                Source::Environment
            } else {
                Source::None
            },
            outcome: if available {
                Outcome::Success
            } else {
                Outcome::Failure
            },
            error: (!available).then_some(ErrorType::CredentialUnavailable),
        });
    }
    let mut copied = false;
    if first_seed {
        if spec.forwarded.is_file() {
            copy_file_with_mode(spec.forwarded, spec.target, 0o600)?;
            copied = true;
        } else {
            remove_file_if_exists(spec.target)?;
            crate::output::stderr_line(format_args!(
                "[entrypoint] {}: no forwarded credential and no api key in env - agent will require interactive login",
                spec.label
            ));
        }
    } else if !spec.target.exists() && spec.forwarded.is_file() {
        copy_file_with_mode(spec.forwarded, spec.target, 0o600)?;
        copied = true;
    }
    let available = spec.target.is_file();
    Ok(AuthMaterialization {
        source: if copied {
            Source::AgentHome
        } else if available {
            Source::OauthStore
        } else {
            Source::None
        },
        outcome: if available {
            Outcome::Success
        } else {
            Outcome::Failure
        },
        error: (!available).then_some(ErrorType::CredentialUnavailable),
    })
}
