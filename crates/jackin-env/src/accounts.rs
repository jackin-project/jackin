// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Resolve credentials exclusively from assigned accounts.

use crate::{OpRunner, resolve_env_value};
use jackin_config::AppConfig;
use jackin_core::{Agent, WorkspaceName};
use std::collections::BTreeMap;

/// Environment names owned by account selection, including endpoint routing.
#[must_use]
pub fn is_account_env(name: &str) -> bool {
    jackin_core::is_account_env(name)
        || matches!(
            name,
            "HOME"
                | "CLAUDE_CONFIG_DIR"
                | "CODEX_HOME"
                | "KIMI_HOME"
                | "AMP_HOME"
                | "OPENCODE_CONFIG"
                | "OPENCODE_CONFIG_DIR"
                | "OPENCODE_CONFIG_CONTENT"
                | "XDG_CONFIG_HOME"
                | "XDG_DATA_HOME"
        )
}

/// Resolve the selected accounts for every agent supported by a role.
///
/// # Errors
/// Returns an error for invalid bindings or unavailable credentials. Each agent receives
/// its own credential map, including when providers use the same key name.
pub fn resolve_account_env_with<R, H>(
    config: &AppConfig,
    agents: &[Agent],
    workspace: Option<&WorkspaceName>,
    role: &str,
    runner: &R,
    host_env: H,
) -> anyhow::Result<BTreeMap<String, BTreeMap<String, String>>>
where
    R: OpRunner + ?Sized,
    H: Fn(&str) -> Result<String, std::env::VarError> + Send + Sync,
{
    let mut agents_env = BTreeMap::new();
    for &agent in agents {
        let Some(account) = jackin_config::resolve_account(config, agent, workspace, role)? else {
            continue;
        };
        let declarations = account.credential_env(agent)?;
        if declarations
            .values()
            .any(|value| matches!(value, jackin_core::EnvValue::OpRef(_)))
        {
            runner.probe()?;
        }
        let resolved = declarations
            .into_iter()
            .map(|(key, value)| {
                anyhow::ensure!(
                    !value.is_on_demand(),
                    "account credential {key} must resolve at launch"
                );
                let resolved =
                    resolve_env_value("selected account", &key, &value, runner, &host_env)?;
                anyhow::ensure!(
                    !resolved.trim().is_empty(),
                    "selected account credential {key} is empty"
                );
                Ok((key, resolved))
            })
            .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
        if !resolved.is_empty() {
            agents_env.insert(agent.slug().to_owned(), resolved);
        }
    }
    Ok(agents_env)
}

#[cfg(test)]
mod tests;
