// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use jackin_core::{Agent, AuthForwardMode, MountIsolation};
use jackin_isolation::materialize::{MaterializedMount, MaterializedWorkspace, WorktreeAuxMounts};

use super::*;
use crate::instance::{
    AgentRuntimeState, GithubProvisionOutcome, ProvisionedAuth, ProvisionedInstanceAuth, RoleState,
};

fn role_state(root: &Path, slots: Vec<(&str, ProvisionedInstanceAuth)>) -> RoleState {
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

fn slot(agent: Agent, store_rel: &str) -> ProvisionedInstanceAuth {
    ProvisionedInstanceAuth {
        profile_material: None,
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

    let mounts = agent_mounts(&state).unwrap();
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

    let mounts = agent_mounts(&state).unwrap();
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
    let mut claude = slot(Agent::Claude, "claude");
    claude.credential_paths = vec![root.join("claude/credentials.json")];
    let state = role_state(&root, vec![("acct@claude", claude)]);
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
fn agent_mounts_emits_provider_config_overlays_readonly_per_slot() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let primary_source = root.join("provider-config/home/.codex/config.toml");
    let secondary_source = root.join("provider-config/home/.codex-second/config.toml");
    std::fs::create_dir_all(primary_source.parent().unwrap()).unwrap();
    std::fs::create_dir_all(secondary_source.parent().unwrap()).unwrap();
    std::fs::write(&primary_source, "model = \"work\"\n").unwrap();
    std::fs::write(&secondary_source, "model = \"personal\"\n").unwrap();

    let mut primary = slot(Agent::Codex, "codex");
    primary.container_home_rel = ".codex".to_owned();
    let mut secondary = slot(Agent::Codex, "codex-second");
    secondary.slot_suffix = Some("second".to_owned());
    secondary.container_home_rel = ".codex-second".to_owned();
    let mut state = role_state(
        &root,
        vec![("work@codex", primary), ("personal@codex", secondary)],
    );
    state.provider_config_mounts = vec![
        (
            primary_source.clone(),
            "/home/agent/.codex/config.toml".to_owned(),
        ),
        (
            secondary_source.clone(),
            "/home/agent/.codex-second/config.toml".to_owned(),
        ),
    ];

    let mounts = agent_mounts(&state).unwrap();
    assert!(mounts.contains(&format!(
        "{}:/home/agent/.codex/config.toml:ro",
        canonical_mount_path(&primary_source).display()
    )));
    assert!(mounts.contains(&format!(
        "{}:/home/agent/.codex-second/config.toml:ro",
        canonical_mount_path(&secondary_source).display()
    )));
}

#[test]
fn agent_mounts_rejects_provider_authority_overlap_from_cache() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let authority = root.join("provider-config");
    let source = authority.join("home/.codex/config.toml");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "model = \"work\"\n").unwrap();

    let mut codex = slot(Agent::Codex, "codex");
    codex.cache_source_dir = Some(authority.clone());
    codex.container_cache_rel = Some(".cache/codex".to_owned());
    let mut state = role_state(&root, vec![("work@codex", codex)]);
    state.provider_config_mounts = vec![(source, "/home/agent/.codex/config.toml".to_owned())];

    let error = agent_mounts(&state).unwrap_err().to_string();
    assert!(
        error.contains("inside writable bind source") && error.contains("provider-config"),
        "cache overlap must fail closed through the source-parent guard: {error}"
    );
}

#[test]
fn apple_agent_mounts_rejects_provider_file_overlays_before_session_setup() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let source = root.join("provider-config/home/.codex/config.toml");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "model = \"work\"\n").unwrap();
    let mut state = role_state(&root, vec![]);
    state.provider_config_mounts = vec![(source, "/home/agent/.codex/config.toml".to_owned())];

    let error = apple_agent_mounts(&state).unwrap_err().to_string();
    assert!(
        error.contains("generated provider config file overlays"),
        "Apple must reject single-file generated overlays: {error}"
    );
}

#[test]
fn provider_authority_audit_rejects_workspace_exposure_and_target_collision() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let source = root.join("provider-config/home/.codex/config.toml");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "model = \"work\"\n").unwrap();
    let mut state = role_state(&root, vec![]);
    state.provider_config_mounts =
        vec![(source.clone(), "/home/agent/.codex/config.toml".to_owned())];
    let overlay = format!("{}:/home/agent/.codex/config.toml:ro", source.display());

    let authority_parent = format!("{}:/workspace:ro", root.join("provider-config").display());
    assert!(
        ensure_provider_authority_not_writable(&state, &[overlay.clone(), authority_parent])
            .is_err()
    );

    let target_collision = "/tmp/user:/home/agent/.codex/config.toml".to_owned();
    assert!(ensure_provider_authority_not_writable(&state, &[overlay, target_collision]).is_err());
}

#[test]
fn provider_authority_audit_rejects_sibling_authority_and_sibling_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("instances/current");
    let source = root.join("provider-config/home/.codex/config.toml");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "model = \"work\"\n").unwrap();
    let mut state = role_state(&root, vec![]);
    state.provider_config_mounts =
        vec![(source.clone(), "/home/agent/.codex/config.toml".to_owned())];
    let overlay = format!("{}:/home/agent/.codex/config.toml:ro", source.display());
    let sibling_root = root.parent().unwrap().join("future-sibling");
    let sibling_authority = sibling_root.join("provider-config");

    let direct = format!("{}:/tmp/sibling-authority:ro", sibling_authority.display());
    assert!(ensure_provider_authority_not_writable(&state, &[overlay.clone(), direct]).is_err());

    let sibling = format!("{}:/tmp/sibling-root:ro", sibling_root.display());
    assert!(ensure_provider_authority_not_writable(&state, &[overlay, sibling]).is_err());
}

#[test]
fn apple_provider_authority_audit_rejects_sibling_sources_without_overlays() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("instances/current");
    let state = role_state(&root, vec![]);
    let sibling_root = root.parent().unwrap().join("future-sibling");
    let sibling_authority = sibling_root.join("provider-config");
    let mounts = vec![AppleContainerMount::new(
        sibling_authority,
        "/tmp/sibling-authority",
        true,
    )];

    assert!(ensure_apple_provider_authority_not_exposed(&state, &mounts).is_err());
}

#[cfg(unix)]
#[test]
fn authority_audits_reject_symlink_parent_traversal_before_canonicalization() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("instances/current");
    let source = root.join("provider-config/home/.codex/config.toml");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "model = \"work\"\n").unwrap();
    let mut docker_state = role_state(&root, vec![]);
    docker_state.provider_config_mounts =
        vec![(source.clone(), "/home/agent/.codex/config.toml".to_owned())];
    let overlay = format!("{}:/home/agent/.codex/config.toml:ro", source.display());

    let alias = temp.path().join("authority-alias");
    symlink(root.join("provider-config"), &alias).unwrap();
    let escaped = alias.join("..").join("provider-config");
    let escaped_bind = format!("{}:/tmp/escaped:ro", escaped.display());
    assert!(
        ensure_provider_authority_not_writable(&docker_state, &[overlay, escaped_bind]).is_err()
    );

    let apple_state = role_state(&root, vec![]);
    let mounts = vec![AppleContainerMount::new(escaped, "/tmp/escaped", true)];
    assert!(ensure_apple_provider_authority_not_exposed(&apple_state, &mounts).is_err());
}

#[cfg(unix)]
#[test]
fn authority_audits_reject_readonly_source_under_writable_alias_parent() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("instances/current");
    let writable_root = root.join("home");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&writable_root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("config.toml"), "model = \"outside\"\n").unwrap();
    let alias = writable_root.join("cache-alias");
    symlink(&outside, &alias).unwrap();
    let readonly_source = alias.join("config.toml");
    let state = role_state(&root, vec![]);

    let docker_mounts = vec![
        format!("{}:/tmp/writable", writable_root.display()),
        format!("{}:/tmp/readonly:ro", readonly_source.display()),
    ];
    assert!(ensure_provider_authority_not_writable(&state, &docker_mounts).is_err());

    let apple_mounts = vec![
        AppleContainerMount::new(writable_root.clone(), "/tmp/writable", false),
        AppleContainerMount::new(readonly_source.clone(), "/tmp/readonly", true),
    ];
    assert!(ensure_apple_provider_authority_not_exposed(&state, &apple_mounts).is_err());
}

#[cfg(unix)]
#[test]
fn authority_audits_reject_nested_rw_cache_alias() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("instances/current");
    let writable_root = root.join("home");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&writable_root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("config.toml"), "model = \"outside\"\n").unwrap();
    let alias = writable_root.join("cache-alias");
    symlink(&outside, &alias).unwrap();
    let cache_source = alias.join("config.toml");
    let state = role_state(&root, vec![]);

    let docker_mounts = vec![
        format!("{}:/tmp/home", writable_root.display()),
        format!("{}:/tmp/cache", cache_source.display()),
    ];
    assert!(ensure_provider_authority_not_writable(&state, &docker_mounts).is_err());

    let apple_mounts = vec![
        AppleContainerMount::new(writable_root, "/tmp/home", false),
        AppleContainerMount::new(cache_source, "/tmp/cache", false),
    ];
    assert!(ensure_apple_provider_authority_not_exposed(&state, &apple_mounts).is_err());
}

#[test]
fn authority_audits_allow_readonly_source_under_readonly_parent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("instances/current");
    let readonly_root = root.join("readonly-parent");
    let readonly_source = readonly_root.join("config.toml");
    std::fs::create_dir_all(&readonly_root).unwrap();
    std::fs::write(&readonly_source, "model = \"outside\"\n").unwrap();
    let state = role_state(&root, vec![]);

    let docker_mounts = vec![
        format!("{}:/tmp/readonly-parent:ro", readonly_root.display()),
        format!("{}:/tmp/readonly:ro", readonly_source.display()),
    ];
    ensure_provider_authority_not_writable(&state, &docker_mounts).unwrap();

    let apple_mounts = vec![
        AppleContainerMount::new(readonly_root.clone(), "/tmp/readonly-parent", true),
        AppleContainerMount::new(readonly_source.clone(), "/tmp/readonly", true),
    ];
    ensure_apple_provider_authority_not_exposed(&state, &apple_mounts).unwrap();
}

#[test]
fn github_config_mount_is_absent_only_when_skipped_and_missing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("role");
    let state = role_state(&root, vec![]);
    assert_eq!(github_config_mount(&state).unwrap(), None);

    std::fs::create_dir_all(root.join("gh")).unwrap();
    let mounted = github_config_mount(&state)
        .unwrap()
        .expect("existing dir must mount");
    assert!(mounted.ends_with(":/home/agent/.config/gh"), "{mounted}");
}
