// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::RoleState;
use jackin_core::Agent;
use jackin_instance_agents::{AgentRuntimeState, ProvisionedAuth};
use jackin_instance_credentials::GithubProvisionOutcome;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn state_for(agent: Agent, model: Option<&str>) -> RoleState {
    RoleState {
        root: PathBuf::from("test-root"),
        gh_config_dir: PathBuf::from("test-gh"),
        gh_provision_outcome: GithubProvisionOutcome::Skipped,
        agent_runtime: AgentRuntimeState {
            agent,
            model: model.map(str::to_owned),
        },
        auth: ProvisionedAuth {
            slots: BTreeMap::new(),
        },
        auth_outcomes: BTreeMap::new(),
        auth_mount_paths: BTreeSet::new(),
        auth_mount_leases: Vec::new(),
        provider_config_mounts: Vec::new(),
    }
}

#[test]
fn model_accessors_follow_selected_agent() {
    let claude = state_for(Agent::Claude, Some("sonnet"));
    assert_eq!(claude.claude_model(), Some("sonnet"));
    assert_eq!(claude.codex_model(), None);
    assert_eq!(claude.kimi_model(), None);

    let codex = state_for(Agent::Codex, Some("gpt-5"));
    assert_eq!(codex.codex_model(), Some("gpt-5"));
    assert_eq!(codex.claude_model(), None);
}

#[test]
fn empty_auth_denies_forwarding_and_unknown_mounts() {
    let state = state_for(Agent::Claude, None);
    assert!(!state.claude_forwards_auth());
    assert!(
        !state
            .auth_mount_file_allowed(Path::new("test-absent-mount"))
            .expect("absence check is infallible"),
        "empty auth must deny unknown mounts"
    );
}
