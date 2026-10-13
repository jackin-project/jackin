// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Ignore-mode skip decisions for state preparation.

use super::{
    GithubAuthContext, InstanceAuthBinding, ProvisionedInstanceAuth, agent_slot_dirs, slot_layout,
    slot_store_rel,
};
use anyhow::Context;
use jackin_config::GithubAuthMode;

use std::path::{Path, PathBuf};

pub(crate) fn skipped_ignore_instance_auth(
    root: &Path,
    binding: &InstanceAuthBinding,
    suffix: Option<&str>,
) -> ProvisionedInstanceAuth {
    // No filesystem work ran, so `home_dir` stays `None`; the
    // deterministic credential paths match the real-path shape so
    // path accessors keep working for lazy launches.
    let (store, home_rel) = agent_slot_dirs(binding.agent);
    let layout = slot_layout(binding.agent, store, home_rel, suffix);
    let store_dir = root.join(&layout.store_rel);
    let credential_paths = match binding.agent {
        jackin_core::Agent::Claude => {
            vec![
                store_dir.join("account.json"),
                store_dir.join("credentials.json"),
            ]
        }
        jackin_core::Agent::Kimi | jackin_core::Agent::Hermes => vec![store_dir],
        _ => Vec::new(),
    };
    ProvisionedInstanceAuth::new(binding, None, credential_paths, false, layout)
}

pub(crate) fn agent_ignore_can_skip_state_prepare(
    root: &Path,
    agent: jackin_core::Agent,
    suffix: Option<&str>,
) -> anyhow::Result<bool> {
    let (store, _) = agent_slot_dirs(agent);
    let store_dir = root.join(slot_store_rel(store, suffix));
    let stale_paths: Vec<PathBuf> = match agent {
        jackin_core::Agent::Claude => {
            vec![
                store_dir.join("account.json"),
                store_dir.join("credentials.json"),
            ]
        }
        jackin_core::Agent::Codex => vec![store_dir.join("auth.json")],
        jackin_core::Agent::Amp => vec![store_dir.join("secrets.json")],
        jackin_core::Agent::Kimi => vec![store_dir.clone()],
        jackin_core::Agent::Opencode => vec![store_dir.join("auth.json")],
        jackin_core::Agent::Grok => vec![store_dir.join("auth.json")],
        jackin_core::Agent::Antigravity => vec![store_dir.join("settings.json")],
        jackin_core::Agent::Gemini => vec![store_dir.join("oauth_creds.json")],
        jackin_core::Agent::Cursor => vec![store_dir.join("auth.json")],
        jackin_core::Agent::Muse => vec![store_dir.join("auth.json")],
        jackin_core::Agent::Omp => vec![store_dir.join("agent.db")],
        jackin_core::Agent::Hermes => vec![store_dir.clone()],
    };

    for path in stale_paths {
        match std::fs::symlink_metadata(&path) {
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect {agent} role-state auth path at {}",
                        path.display()
                    )
                });
            }
        }
    }

    Ok(true)
}

pub(crate) fn github_ignore_can_skip_state_prepare(
    github: &GithubAuthContext,
    hosts_yml: &Path,
) -> anyhow::Result<bool> {
    if github.mode != GithubAuthMode::Ignore {
        return Ok(false);
    }
    match std::fs::symlink_metadata(hosts_yml) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to inspect GitHub role-state file at {}",
                hosts_yml.display()
            )
        }),
    }
}
