// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::ConfigError;
use std::collections::BTreeMap;

use jackin_core::{EnvValue, RoleSelector, WorkspaceName};

use super::AppConfig;
use crate::auth::GithubAuthMode;
use crate::schema::RoleSource;

fn workspace_key(workspace: Option<&WorkspaceName>) -> &str {
    workspace.map_or("", WorkspaceName::as_str)
}

pub fn resolve_github_mode(
    cfg: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> GithubAuthMode {
    let ws = workspace_key(workspace);
    if let Some(m) = cfg
        .workspaces
        .get(ws)
        .and_then(|w| w.roles.get(role))
        .and_then(|ro| ro.github.as_ref().map(|g| g.auth_forward))
    {
        return m;
    }

    if let Some(m) = cfg
        .workspaces
        .get(ws)
        .and_then(|w| w.github.as_ref().map(|g| g.auth_forward))
    {
        return m;
    }

    cfg.github
        .as_ref()
        .map_or_else(GithubAuthMode::default, |g| g.auth_forward)
}

pub fn build_github_env_layers(
    cfg: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> BTreeMap<String, EnvValue> {
    let ws = cfg.workspaces.get(workspace_key(workspace));
    let layers = [
        cfg.github.as_ref().map(|g| &g.env),
        ws.and_then(|w| w.github.as_ref()).map(|g| &g.env),
        ws.and_then(|w| w.roles.get(role))
            .and_then(|ro| ro.github.as_ref())
            .map(|g| &g.env),
    ];
    let mut merged: BTreeMap<String, EnvValue> = BTreeMap::new();
    for env in layers.into_iter().flatten() {
        for (k, v) in env {
            merged.insert(k.clone(), v.clone());
        }
    }
    merged
}

pub const BUILTIN_ROLES: &[(&str, &str)] = &[
    (
        "agent-smith",
        "https://github.com/jackin-project/jackin-agent-smith.git",
    ),
    (
        "the-architect",
        "https://github.com/jackin-project/jackin-the-architect.git",
    ),
];

impl AppConfig {
    pub fn resolve_role_source(
        &mut self,
        selector: &RoleSelector,
    ) -> crate::ConfigResult<(RoleSource, bool)> {
        if let Some(source) = self.roles.get(&selector.key()) {
            return Ok((source.clone(), false));
        }

        let namespace = selector.namespace.as_ref().ok_or_else(|| {
            anyhow::Error::from(ConfigError::msg(format_args!(
                "unknown selector {}",
                selector.key()
            )))
        })?;

        let repo = if selector.name.starts_with("jackin-") {
            selector.name.clone()
        } else {
            format!("jackin-{}", selector.name)
        };

        let source = RoleSource {
            git: format!("https://github.com/{namespace}/{repo}.git"),
            trusted: false,
            env: BTreeMap::new(),
        };
        self.roles.insert(selector.key(), source.clone());
        Ok((source, true))
    }

    pub fn trust_agent(&mut self, key: &str) -> bool {
        if let Some(source) = self.roles.get_mut(key)
            && !source.trusted
        {
            source.trusted = true;
            return true;
        }
        false
    }

    pub fn untrust_agent(&mut self, key: &str) -> bool {
        if let Some(source) = self.roles.get_mut(key)
            && source.trusted
        {
            source.trusted = false;
            return true;
        }
        false
    }

    pub fn is_builtin_agent(key: &str) -> bool {
        BUILTIN_ROLES.iter().any(|&(name, _)| name == key)
    }

    pub fn sync_builtin_agents(&mut self) -> bool {
        let mut changed = false;
        for &(name, git) in BUILTIN_ROLES {
            let expected = RoleSource {
                git: git.to_owned(),
                trusted: true,
                env: BTreeMap::new(),
            };
            match self.roles.get(name) {
                Some(existing) if existing.git == expected.git && existing.trusted => {}
                _ => {
                    self.roles.insert(name.to_owned(), expected);
                    changed = true;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests;
