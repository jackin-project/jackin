// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Per-agent credential setup routines.

use super::{
    AGENT_HOME, AuthMaterialization, AuthMode, ForwardedCredential, GROK_AUTH_PATH, MUSE_AUTH_PATH,
    amp_secrets_path, antigravity_settings_path, codex_auth_path, codex_home, copy_dir_contents,
    cursor_auth_path, cursor_home, dir_nonempty, forwarded_dir, forwarded_file, gemini_home,
    gemini_oauth_creds_path, hermes_home, nonempty_env, omp_agent_db_path, omp_home,
    opencode_auth_path, seed_agent_home_from_enum, seed_forwarded_credential, xdg_data_home,
};

use std::fs;

use anyhow::{Context, Result};
use std::path::Path;

use jackin_core::container_paths;

pub(crate) fn setup_codex(mode: AuthMode) -> Result<AuthMaterialization> {
    let forwarded = forwarded_file(container_paths::CODEX_AUTH);
    let target = codex_auth_path();
    seed_forwarded_credential(
        jackin_core::Agent::Codex,
        mode,
        &codex_home(),
        &ForwardedCredential {
            label: "codex",
            forwarded: &forwarded,
            target: &target,
            api_key_envs: &["OPENAI_API_KEY"],
        },
    )
}

pub(crate) fn setup_amp(mode: AuthMode) -> Result<AuthMaterialization> {
    let forwarded = forwarded_file(container_paths::AMP_SECRETS);
    let target = amp_secrets_path();
    seed_forwarded_credential(
        jackin_core::Agent::Amp,
        mode,
        &xdg_data_home().join("amp"),
        &ForwardedCredential {
            label: "amp",
            forwarded: &forwarded,
            target: &target,
            api_key_envs: &["AMP_API_KEY"],
        },
    )
}

/// Kimi is the one sync-capable agent whose credential store is a directory, so
/// it cannot use [`seed_forwarded_credential`]. It applies the same closed mode
/// policy to the whole store: sync seeds/reuses it, environment modes and ignore
/// remove it so stale credentials cannot silently override the selected mode.
pub(crate) fn setup_kimi(mode: AuthMode) -> Result<AuthMaterialization> {
    use jackin_telemetry::schema::enums::{
        CredentialSourceType as Source, ErrorType, OutcomeValue as Outcome,
    };
    let target = Path::new("/home/agent/.kimi-code");
    let first_seed = seed_agent_home_from_enum(jackin_core::Agent::Kimi, target)?.is_first_seed();
    let forwarded = forwarded_dir(container_paths::KIMI_CODE_DIR);
    let forwarded_present = forwarded.is_dir() && dir_nonempty(&forwarded)?;
    if matches!(mode, AuthMode::Ignore) {
        if target.exists() {
            fs::remove_dir_all(target).context("failed to clear ignored Kimi credentials")?;
        }
        return Ok(AuthMaterialization {
            source: Source::None,
            outcome: Outcome::Skip,
            error: None,
        });
    }
    if matches!(mode, AuthMode::ApiKey | AuthMode::OauthToken) {
        if target.exists() {
            fs::remove_dir_all(target).context("failed to clear Kimi credential store")?;
        }
        let available = nonempty_env("KIMI_CODE_API_KEY").is_some();
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
        if forwarded_present {
            copy_dir_contents(&forwarded, target)?;
            copied = true;
        } else {
            crate::output::stderr_line(format_args!(
                "[entrypoint] kimi: no forwarded credential and no api key in env - agent will require interactive login"
            ));
        }
    } else if forwarded_present && !(target.is_dir() && dir_nonempty(target)?) {
        copy_dir_contents(&forwarded, target)?;
        copied = true;
    }
    let available = target.is_dir() && dir_nonempty(target)?;
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

pub(crate) fn setup_opencode(mode: AuthMode) -> Result<AuthMaterialization> {
    let forwarded = forwarded_file(container_paths::OPENCODE_AUTH);
    let target = opencode_auth_path();
    seed_forwarded_credential(
        jackin_core::Agent::Opencode,
        mode,
        &xdg_data_home().join("opencode"),
        &ForwardedCredential {
            label: "opencode",
            forwarded: &forwarded,
            target: &target,
            api_key_envs: &["OPENCODE_API_KEY"],
        },
    )
}

pub(crate) fn setup_grok(mode: AuthMode) -> Result<AuthMaterialization> {
    let forwarded = forwarded_file(container_paths::GROK_AUTH);
    let seed_base = Path::new(GROK_AUTH_PATH)
        .parent()
        .unwrap_or(Path::new(AGENT_HOME));
    seed_forwarded_credential(
        jackin_core::Agent::Grok,
        mode,
        seed_base,
        &ForwardedCredential {
            label: "grok",
            forwarded: &forwarded,
            target: Path::new(GROK_AUTH_PATH),
            api_key_envs: &["XAI_API_KEY", "GROK_DEPLOYMENT_KEY"],
        },
    )
}

pub(crate) fn setup_antigravity(mode: AuthMode) -> Result<AuthMaterialization> {
    let forwarded = forwarded_file(container_paths::ANTIGRAVITY_SETTINGS);
    let target = antigravity_settings_path();
    seed_forwarded_credential(
        jackin_core::Agent::Antigravity,
        mode,
        &gemini_home().join("antigravity-cli"),
        &ForwardedCredential {
            label: "antigravity",
            forwarded: &forwarded,
            target: &target,
            api_key_envs: &["GEMINI_API_KEY"],
        },
    )
}

pub(crate) fn setup_gemini(mode: AuthMode) -> Result<AuthMaterialization> {
    let forwarded = forwarded_file(container_paths::GEMINI_AUTH);
    let target = gemini_oauth_creds_path();
    seed_forwarded_credential(
        jackin_core::Agent::Gemini,
        mode,
        &gemini_home(),
        &ForwardedCredential {
            label: "gemini",
            forwarded: &forwarded,
            target: &target,
            api_key_envs: &["GEMINI_API_KEY"],
        },
    )
}

pub(crate) fn setup_cursor(mode: AuthMode) -> Result<AuthMaterialization> {
    let forwarded = forwarded_file(container_paths::CURSOR_AUTH);
    let target = cursor_auth_path();
    seed_forwarded_credential(
        jackin_core::Agent::Cursor,
        mode,
        &cursor_home(),
        &ForwardedCredential {
            label: "cursor",
            forwarded: &forwarded,
            target: &target,
            api_key_envs: &["CURSOR_API_KEY"],
        },
    )
}

pub(crate) fn setup_muse(mode: AuthMode) -> Result<AuthMaterialization> {
    let forwarded = forwarded_file(container_paths::MUSE_AUTH);
    let seed_base = Path::new(MUSE_AUTH_PATH)
        .parent()
        .unwrap_or(Path::new(AGENT_HOME));
    seed_forwarded_credential(
        jackin_core::Agent::Muse,
        mode,
        seed_base,
        &ForwardedCredential {
            label: "muse",
            forwarded: &forwarded,
            target: Path::new(MUSE_AUTH_PATH),
            api_key_envs: &["META_API_KEY"],
        },
    )
}

pub(crate) fn setup_omp(mode: AuthMode) -> Result<AuthMaterialization> {
    let forwarded = forwarded_file(container_paths::OMP_AGENT_DB);
    let target = omp_agent_db_path();
    seed_forwarded_credential(
        jackin_core::Agent::Omp,
        mode,
        &omp_home(),
        &ForwardedCredential {
            label: "omp",
            forwarded: &forwarded,
            target: &target,
            // No native key: any routed provider key suppresses the warning.
            api_key_envs: &[
                "OPENROUTER_API_KEY",
                "ANTHROPIC_API_KEY",
                "OPENAI_API_KEY",
                "GEMINI_API_KEY",
                "CURSOR_API_KEY",
                "META_API_KEY",
                "XAI_API_KEY",
            ],
        },
    )
}

/// Hermes's store is a directory (like Kimi), so it cannot use
/// [`seed_forwarded_credential`]; same closed mode policy as [`setup_kimi`].
pub(crate) fn setup_hermes(mode: AuthMode) -> Result<AuthMaterialization> {
    use jackin_telemetry::schema::enums::{
        CredentialSourceType as Source, ErrorType, OutcomeValue as Outcome,
    };
    let target = hermes_home();
    let first_seed =
        seed_agent_home_from_enum(jackin_core::Agent::Hermes, &target)?.is_first_seed();
    let forwarded = forwarded_dir(container_paths::HERMES_DIR);
    let forwarded_present = forwarded.is_dir() && dir_nonempty(&forwarded)?;
    if matches!(mode, AuthMode::Ignore) {
        if target.exists() {
            fs::remove_dir_all(&target).context("failed to clear ignored Hermes credentials")?;
        }
        return Ok(AuthMaterialization {
            source: Source::None,
            outcome: Outcome::Skip,
            error: None,
        });
    }
    if matches!(mode, AuthMode::ApiKey | AuthMode::OauthToken) {
        if target.exists() {
            fs::remove_dir_all(&target).context("failed to clear Hermes credential store")?;
        }
        let available = [
            "OPENROUTER_API_KEY",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "CURSOR_API_KEY",
            "META_API_KEY",
            "XAI_API_KEY",
        ]
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
        if forwarded_present {
            copy_dir_contents(&forwarded, &target)?;
            copied = true;
        } else {
            crate::output::stderr_line(format_args!(
                "[entrypoint] hermes: no forwarded credential and no api key in env - agent will require interactive login"
            ));
        }
    } else if forwarded_present && !(target.is_dir() && dir_nonempty(&target)?) {
        copy_dir_contents(&forwarded, &target)?;
        copied = true;
    }
    let available = target.is_dir() && dir_nonempty(&target)?;
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
