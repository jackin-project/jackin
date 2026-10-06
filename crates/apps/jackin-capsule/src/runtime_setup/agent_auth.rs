// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Agent setup dispatch, auth modes, and materialization records.

use super::{
    nonempty_env, record_recovered_degradation, setup_amp, setup_antigravity, setup_claude,
    setup_codex, setup_cursor, setup_gemini, setup_grok, setup_hermes, setup_kimi, setup_muse,
    setup_omp, setup_opencode,
};

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

pub(crate) fn run_agent_setup() -> Result<()> {
    let agent = std::env::var("JACKIN_AGENT").context("JACKIN_AGENT must be set")?;
    let mode = AuthMode::from_env()?;
    // Home emptiness is the gate: first seed copies auth; subsequent starts
    // leave in-container credentials untouched (agent refreshes tokens in-place
    // inside the durable home). No external marker file.
    let materialization = match agent.as_str() {
        "claude" => setup_claude(mode),
        "codex" => setup_codex(mode),
        "amp" => setup_amp(mode),
        "kimi" => setup_kimi(mode),
        "opencode" => setup_opencode(mode),
        "grok" => setup_grok(mode),
        "antigravity" => setup_antigravity(mode),
        "gemini" => setup_gemini(mode),
        "cursor" => setup_cursor(mode),
        "muse" => setup_muse(mode),
        "omp" => setup_omp(mode),
        "hermes" => setup_hermes(mode),
        other => bail!("unknown JACKIN_AGENT: {other}"),
    };
    emit_capsule_auth_provision(&agent, mode, materialization.as_ref());
    materialization?;

    // Install/repair the agent-status reporter on every launch (drift repair).
    // Observability must never break the agent: a failure is logged, not fatal.
    if let Err(e) = install_agent_status_reporter(&agent) {
        let message = reporter_install_failure_message(&agent, &e);
        record_recovered_degradation();
        crate::output::stderr_line(format_args!("[entrypoint] {message}"));
    }

    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum AuthMode {
    Sync,
    ApiKey,
    OauthToken,
    Ignore,
}

impl AuthMode {
    pub(crate) fn from_env() -> Result<Self> {
        let mode = std::env::var(jackin_protocol::AUTH_MODE_ENV)
            .context("JACKIN_AUTH_MODE must be set")?;
        match mode.as_str() {
            "sync" => Ok(Self::Sync),
            "api_key" => Ok(Self::ApiKey),
            "oauth_token" => Ok(Self::OauthToken),
            "ignore" => Ok(Self::Ignore),
            _ => bail!("JACKIN_AUTH_MODE must contain a bounded auth mode"),
        }
    }

    pub(crate) const fn as_schema(self) -> jackin_telemetry::schema::enums::AuthMode {
        use jackin_telemetry::schema::enums::AuthMode as SchemaMode;
        match self {
            Self::Sync => SchemaMode::Sync,
            Self::ApiKey => SchemaMode::ApiKey,
            Self::OauthToken => SchemaMode::OauthToken,
            Self::Ignore => SchemaMode::Ignore,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AuthMaterialization {
    pub(crate) source: jackin_telemetry::schema::enums::CredentialSourceType,
    pub(crate) outcome: jackin_telemetry::schema::enums::OutcomeValue,
    pub(crate) error: Option<jackin_telemetry::schema::enums::ErrorType>,
}

pub(crate) fn emit_capsule_auth_provision(
    agent: &str,
    mode: AuthMode,
    result: Result<&AuthMaterialization, &anyhow::Error>,
) {
    use jackin_telemetry::{Attr, FieldSet, Value, event, schema};
    let fallback = AuthMaterialization {
        source: configured_credential_source(mode),
        outcome: schema::enums::OutcomeValue::Error,
        error: Some(schema::enums::ErrorType::IoError),
    };
    let materialization = result.copied().unwrap_or(fallback);
    let mut attrs = vec![
        Attr {
            key: schema::attrs::GEN_AI_AGENT_NAME,
            value: Value::Str(agent),
        },
        Attr {
            key: schema::attrs::AUTH_MODE,
            value: Value::Str(mode.as_schema().as_str()),
        },
        Attr {
            key: schema::attrs::CREDENTIAL_SOURCE_TYPE,
            value: Value::Str(materialization.source.as_str()),
        },
        Attr {
            key: schema::attrs::OUTCOME,
            value: Value::Str(materialization.outcome.as_str()),
        },
    ];
    if let Some(error) = materialization.error {
        attrs.push(Attr {
            key: schema::attrs::std_attrs::ERROR_TYPE,
            value: Value::Str(error.as_str()),
        });
    }
    let _emitted =
        jackin_telemetry::emit_event(&event::AUTH_PROVISION, FieldSet::new(&attrs, None));
}

pub(crate) const fn configured_credential_source(
    mode: AuthMode,
) -> jackin_telemetry::schema::enums::CredentialSourceType {
    use jackin_telemetry::schema::enums::CredentialSourceType as Source;
    match mode {
        AuthMode::Sync => Source::AgentHome,
        AuthMode::ApiKey | AuthMode::OauthToken => Source::Environment,
        AuthMode::Ignore => Source::None,
    }
}

pub(crate) fn reporter_install_failure_message(agent: &str, error: &anyhow::Error) -> String {
    format!("agent-status: reporter install for {agent} failed (non-fatal): {error:#}")
}

/// Install the container-local agent-status reporter for `agent` into the agent
/// home, repairing drift each launch. Claude/Codex install too — their forwarded
/// events are gated to identity/freshness only (Decision 0a), the screen pack
/// owns their state. Kimi is rule-pack-only (no reporter); Amp installs no
/// reporter (it has no plugins.json node-plugin mechanism — writing one crashes
/// it; a real Amp reporter needs its MCP/toolbox surface); unknown/grok have no
/// reporter yet.
pub(crate) fn install_agent_status_reporter(agent: &str) -> Result<()> {
    use crate::agent_status::hook_installer::{
        ClaudeHookInstaller, CodexHookInstaller, HookInstaller, PluginInstaller,
    };
    let installer: Option<Box<dyn HookInstaller>> = match agent {
        "claude" => Some(Box::new(ClaudeHookInstaller::default())),
        "codex" => Some(Box::new(CodexHookInstaller::default())),
        // Amp has no `~/.config/amp/plugins.json` node-plugin mechanism (that
        // was assumed from OpenCode's model); writing one crashes Amp on load. A
        // reporter must never break the agent it observes, so Amp installs no
        // reporter — its status falls back to screen + physics evidence. A real
        // Amp reporter needs Amp's actual extension surface (MCP/toolbox) and is
        // tracked as remaining work.
        "opencode" => Some(Box::new(PluginInstaller::opencode())),
        _ => None,
    };
    if let Some(installer) = installer {
        let home = Path::new("/home/agent");
        // The daemon exports the instance's folder var before setup runs;
        // fall back to the legacy config home when unset (manual runs).
        let parsed = jackin_core::Agent::from_slug(agent);
        let config_dir = parsed
            .and_then(|agent| agent.runtime().state_paths().folder_env_var)
            .and_then(|var| nonempty_env(var.name))
            .map_or_else(
                || {
                    parsed.map_or_else(
                        || home.to_path_buf(),
                        |agent| home.join(agent.runtime().state_paths().credential_dir),
                    )
                },
                PathBuf::from,
            );
        if !installer.verify(home, &config_dir) {
            installer
                .install(home, &config_dir)
                .with_context(|| format!("install {agent} agent-status reporter"))?;
        }
    }
    Ok(())
}
