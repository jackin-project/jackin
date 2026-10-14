// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    AuthMaterialization, AuthMode, copy_file_with_mode, nonempty_env, remove_file_if_exists,
    seed_agent_home_from_enum,
};

use anyhow::Result;
use std::path::Path;

pub(crate) struct ForwardedCredential<'a> {
    pub(crate) label: &'a str,
    pub(crate) forwarded: &'a Path,
    pub(crate) target: &'a Path,
    pub(crate) api_key_envs: &'a [&'a str],
}

pub(crate) fn seed_forwarded_credential(
    agent: jackin_core::Agent,
    mode: AuthMode,
    seed_base: &Path,
    spec: &ForwardedCredential<'_>,
) -> Result<AuthMaterialization> {
    let first_seed = seed_agent_home_from_enum(agent, seed_base)?.is_first_seed();
    apply_forwarded_credential(first_seed, mode, spec)
}

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
