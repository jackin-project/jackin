// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn workspace(workdir: &str, dsts: &[&str]) -> ResolvedWorkspace {
    ResolvedWorkspace {
        name: "fixture".to_owned(),
        label: "fixture".to_owned(),
        workdir: workdir.to_owned(),
        mounts: dsts
            .iter()
            .map(|dst| MountConfig {
                src: "/host".to_owned(),
                dst: (*dst).to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            })
            .collect(),
        keep_awake_enabled: false,
        default_agent: None,
        git_pull_on_entry: false,
        mount_heal: MountHealReport::default(),
    }
}

pub(super) fn codex_state(root: &std::path::Path) -> RoleState {
    RoleState {
        root: root.to_owned(),
        gh_config_dir: root.join("gh"),
        gh_provision_outcome: GithubProvisionOutcome::Skipped,
        agent_runtime: AgentRuntimeState {
            agent: Agent::Codex,
            model: None,
        },
        auth: ProvisionedAuth {
            slots: BTreeMap::from([(
                "acct@codex".to_owned(),
                ProvisionedInstanceAuth {
                    agent: Agent::Codex,
                    account_id: "fixture".to_owned(),
                    mode: AuthForwardMode::Sync,
                    home_dir: None,
                    credential_paths: Vec::new(),
                    forward_auth: false,
                    slot_suffix: None,
                    container_home_rel: ".codex".to_owned(),
                    container_store_rel: "codex".to_owned(),
                    folder_target: String::new(),
                    cache_source_dir: None,
                    container_cache_rel: None,
                },
            )]),
        },
        auth_outcomes: BTreeMap::new(),
        auth_mount_paths: BTreeSet::new(),
        auth_mount_leases: Vec::new(),
        provider_config_mounts: Vec::new(),
    }
}
