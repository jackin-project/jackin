// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn role_state(root: &Path, slots: Vec<(&str, ProvisionedInstanceAuth)>) -> RoleState {
    std::fs::create_dir_all(root.join("state")).unwrap();
    let slots: BTreeMap<_, _> = slots
        .into_iter()
        .map(|(key, slot)| (key.to_owned(), slot))
        .collect();
    let mut auth_mount_paths = BTreeSet::new();
    for slot in slots.values().filter(|slot| slot.forward_auth) {
        for path in &slot.credential_paths {
            auth_mount_paths.insert(path.clone());
            if !matches!(slot.agent, Agent::Kimi | Agent::Hermes)
                && let Some(parent) = path.parent()
            {
                auth_mount_paths.insert(parent.to_path_buf());
            }
        }
    }
    RoleState {
        root: root.to_owned(),
        gh_config_dir: root.join("gh"),
        gh_provision_outcome: GithubProvisionOutcome::Skipped,
        agent_runtime: AgentRuntimeState {
            agent: Agent::Claude,
            model: None,
        },
        auth: ProvisionedAuth { slots },
        auth_outcomes: BTreeMap::new(),
        auth_mount_paths,
        auth_mount_leases: Vec::new(),
        provider_config_mounts: Vec::new(),
    }
}

pub(super) fn slot(agent: Agent, store_rel: &str) -> ProvisionedInstanceAuth {
    ProvisionedInstanceAuth {
        agent,
        account_id: "fixture".to_owned(),
        mode: AuthForwardMode::Sync,
        home_dir: None,
        credential_paths: Vec::new(),
        forward_auth: true,
        slot_suffix: None,
        container_home_rel: format!(".{store_rel}"),
        container_store_rel: store_rel.to_owned(),
        folder_target: String::new(),
        cache_source_dir: None,
        container_cache_rel: None,
    }
}

pub(super) fn assert_coordinator_source_rejected(state: &RoleState, root: &Path, source: &Path) {
    for readonly in [false, true] {
        for target in ["/home/agent", "/resources", "/archive"] {
            let docker = format!(
                "{}:{target}:{}",
                source.display(),
                if readonly { "ro" } else { "rw" }
            );
            let roots = [root.to_owned()];
            let error = ensure_provider_authority_not_writable(state, &[docker], &roots)
                .expect_err("Docker must never expose coordinator locks")
                .to_string();
            assert!(error.contains("protected host root"), "{error}");
            let apple = AppleContainerMount::new(source.to_owned(), target, readonly);
            let error = ensure_apple_provider_authority_not_exposed(state, &[apple], &roots)
                .expect_err("Apple must never expose coordinator locks")
                .to_string();
            assert!(error.contains("protected host root"), "{error}");
        }
    }
}
