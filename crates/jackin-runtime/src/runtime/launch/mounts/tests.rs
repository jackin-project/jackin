// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{Agent, AuthForwardMode, MountIsolation};
use jackin_isolation::materialize::{MaterializedMount, MaterializedWorkspace, WorktreeAuxMounts};

use super::*;
use crate::instance::{
    AgentRuntimeState, GithubProvisionOutcome, ProvisionedAuth, ProvisionedInstanceAuth, RoleState,
};

fn role_state(root: &Path, slots: Vec<(&str, ProvisionedInstanceAuth)>) -> RoleState {
    RoleState {
        root: root.to_owned(),
        gh_config_dir: root.join("gh"),
        gh_provision_outcome: GithubProvisionOutcome::Skipped,
        agent_runtime: AgentRuntimeState {
            agent: Agent::Claude,
            model: None,
        },
        auth: ProvisionedAuth {
            slots: slots
                .into_iter()
                .map(|(key, slot)| (key.to_owned(), slot))
                .collect(),
        },
        auth_outcomes: BTreeMap::new(),
    }
}

fn slot(agent: Agent, store_rel: &str) -> ProvisionedInstanceAuth {
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

#[test]
fn resolve_backend_defaults_to_docker_and_fails_closed_on_typo() {
    let config = AppConfig::default();
    assert_eq!(resolve_backend(&config, None).unwrap(), Backend::Docker);
    assert_eq!(
        resolve_backend(&config, Some("missing")).unwrap(),
        Backend::Docker
    );

    let mut config = AppConfig::default();
    config.runtime.default_backend = Some("apple-container".to_owned());
    assert_eq!(
        resolve_backend(&config, None).unwrap(),
        Backend::AppleContainer
    );

    config.runtime.default_backend = Some("dokcer".to_owned());
    resolve_backend(&config, None).expect_err("typo must not fall through to Docker");
}

#[test]
fn resolve_backend_workspace_override_wins_over_global() {
    let mut config = AppConfig::default();
    config.runtime.default_backend = Some("apple-container".to_owned());
    config.workspaces.insert(
        "work".to_owned(),
        jackin_config::WorkspaceConfig {
            runtime: jackin_config::WorkspaceRuntimeConfig {
                backend: Some("docker".to_owned()),
            },
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    assert_eq!(
        resolve_backend(&config, Some("work")).unwrap(),
        Backend::Docker
    );

    config.workspaces.get_mut("work").unwrap().runtime.backend = Some("bogus".to_owned());
    resolve_backend(&config, Some("work"))
        .expect_err("workspace typo must fail closed, not inherit global");
}

#[test]
fn workspace_mount_strings_order_by_depth_and_harden_gitdir_overrides() {
    let workspace = MaterializedWorkspace {
        workdir: "/work".to_owned(),
        mounts: vec![
            MaterializedMount {
                bind_src: "/host/shallow".to_owned(),
                dst: "/work".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            },
            MaterializedMount {
                bind_src: "/host/deep".to_owned(),
                dst: "/work/deep".to_owned(),
                readonly: true,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            },
            MaterializedMount {
                bind_src: "/host/wt".to_owned(),
                dst: "/wt".to_owned(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(WorktreeAuxMounts {
                    host_git_dir: "/host/repo/.git".to_owned(),
                    host_git_target: "/jackin/host/wt/.git".to_owned(),
                    git_file_override: "/host/overrides/git".to_owned(),
                    git_file_target: "/wt/.git".to_owned(),
                    gitdir_back_override: "/host/overrides/gitdir".to_owned(),
                    gitdir_back_target: "/jackin/host/wt/.git/worktrees/fixture/gitdir".to_owned(),
                }),
            },
        ],
        keep_awake_enabled: false,
    };

    let mounts = build_workspace_mount_strings(&workspace);
    // Ordered by destination length so deeper mounts shadow correctly; the
    // `:ro` placement on the worktree pointer files keeps a misbehaving role
    // from rewriting the gitdir redirect.
    assert_eq!(
        mounts,
        vec![
            "/host/wt:/wt".to_owned(),
            "/host/repo/.git:/jackin/host/wt/.git".to_owned(),
            "/host/overrides/git:/wt/.git:ro".to_owned(),
            "/host/overrides/gitdir:/jackin/host/wt/.git/worktrees/fixture/gitdir:ro".to_owned(),
            "/host/shallow:/work".to_owned(),
            "/host/deep:/work/deep:ro".to_owned(),
        ]
    );
}

#[test]
fn apple_workspace_mounts_reject_worktree_file_overlays() {
    let shared = MaterializedWorkspace {
        workdir: "/work".to_owned(),
        mounts: vec![MaterializedMount {
            bind_src: "/host/src".to_owned(),
            dst: "/work".to_owned(),
            readonly: true,
            isolation: MountIsolation::Shared,
            worktree_aux: None,
        }],
        keep_awake_enabled: false,
    };
    let mounts = build_workspace_mounts(&shared).unwrap();
    assert_eq!(mounts.len(), 1);
    assert!(mounts[0].readonly);

    let worktree = MaterializedWorkspace {
        workdir: "/work".to_owned(),
        mounts: vec![MaterializedMount {
            bind_src: "/host/wt".to_owned(),
            dst: "/wt".to_owned(),
            readonly: false,
            isolation: MountIsolation::Worktree,
            worktree_aux: Some(WorktreeAuxMounts {
                host_git_dir: "/host/repo/.git".to_owned(),
                host_git_target: "/jackin/host/wt/.git".to_owned(),
                git_file_override: "/host/overrides/git".to_owned(),
                git_file_target: "/wt/.git".to_owned(),
                gitdir_back_override: "/host/overrides/gitdir".to_owned(),
                gitdir_back_target: "/jackin/host/wt/.git/worktrees/fixture/gitdir".to_owned(),
            }),
        }],
        keep_awake_enabled: false,
    };
    assert_eq!(
        build_workspace_mounts(&worktree).unwrap_err(),
        AppleContainerMountError::WorktreeFileOverlays {
            destination: "/wt".to_owned(),
        }
    );
}

#[test]
fn agent_mounts_starts_with_state_and_mounts_auth_readonly() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let creds = root.join("auth").join("claude");
    std::fs::create_dir_all(&creds).unwrap();
    let credential = creds.join("credentials.json");
    std::fs::write(&credential, "{}").unwrap();
    let mut claude = slot(Agent::Claude, "claude");
    claude.credential_paths = vec![credential.clone()];
    let state = role_state(&root, vec![("acct@claude", claude)]);

    let mounts = agent_mounts(&state);
    assert_eq!(
        mounts[0],
        format!("{}:/jackin/state", root.join("state").display())
    );
    assert!(
        mounts.contains(&format!(
            "{}:/home/agent/.claude",
            root.join("home").join(".claude").display()
        )),
        "slot home missing: {mounts:?}"
    );
    assert!(
        mounts.contains(&format!(
            "{}:/jackin/claude/credentials.json:ro",
            credential.display()
        )),
        "credential file must be a read-only file mount: {mounts:?}"
    );
}

#[test]
fn agent_mounts_skips_auth_without_forwarding_or_missing_claude_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let mut unforwarded = slot(Agent::Claude, "claude");
    unforwarded.forward_auth = false;
    unforwarded.credential_paths = vec![root.join("auth").join("credentials.json")];
    let mut missing = slot(Agent::Claude, "claude");
    missing.slot_suffix = Some("second".to_owned());
    missing.container_home_rel = ".claude-second".to_owned();
    missing.container_store_rel = "claude-second".to_owned();
    // Guard: a missing Claude credential file (e.g. silently-removed
    // OAuthToken skeleton) must not mount a stale host path.
    missing.credential_paths = vec![root.join("auth").join("absent.json")];
    let state = role_state(
        &root,
        vec![("a@claude", unforwarded), ("b@claude", missing)],
    );

    let mounts = agent_mounts(&state);
    assert!(
        !mounts.iter().any(|mount| mount.contains("/jackin/claude")),
        "no auth mounts expected: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|mount| mount.contains("/home/agent/.claude-second")),
        "secondary slot home missing: {mounts:?}"
    );
}

#[test]
fn apple_agent_mounts_requires_dirs_and_keeps_auth_readonly() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let state = role_state(&root, vec![("acct@claude", slot(Agent::Claude, "claude"))]);
    apple_agent_mounts(&state).expect_err("missing credentials dir must fail");

    std::fs::create_dir_all(root.join("credentials")).unwrap();
    std::fs::create_dir_all(root.join("claude")).unwrap();
    let mounts = apple_agent_mounts(&state).unwrap();
    let store = mounts
        .iter()
        .find(|mount| mount.target.to_string_lossy() == "/jackin/claude")
        .expect("auth store mount missing");
    assert!(store.readonly);
    assert!(
        mounts
            .iter()
            .any(|mount| mount.target.to_string_lossy() == "/jackin/state")
    );
}

#[test]
fn github_config_mount_is_absent_only_when_skipped_and_missing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let state = role_state(&root, vec![]);
    assert_eq!(github_config_mount(&state), None);

    std::fs::create_dir_all(root.join("gh")).unwrap();
    let mounted = github_config_mount(&state).expect("existing dir must mount");
    assert!(mounted.ends_with(":/home/agent/.config/gh"), "{mounted}");
}
