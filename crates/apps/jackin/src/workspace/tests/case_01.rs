// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn allows_all_roles_when_allowed_list_is_empty() {
    assert!(jackin_console::workspace::allows_all_agents(
        &ws_with_allowed(vec![])
    ));
    assert!(!jackin_console::workspace::allows_all_agents(
        &ws_with_allowed(vec!["alpha".into()])
    ));
}

#[test]
fn role_access_accepts_empty_shorthand_or_explicit_membership() {
    let all = ws_with_allowed(vec![]);
    assert!(jackin_console::workspace::agent_is_effectively_allowed(
        &all, "alpha"
    ));
    assert!(jackin_console::workspace::agent_is_effectively_allowed(
        &all, "beta"
    ));

    let custom = ws_with_allowed(vec!["alpha".into(), "gamma".into()]);
    assert!(jackin_console::workspace::agent_is_effectively_allowed(
        &custom, "alpha"
    ));
    assert!(!jackin_console::workspace::agent_is_effectively_allowed(
        &custom, "beta"
    ));
    assert!(jackin_console::workspace::agent_is_effectively_allowed(
        &custom, "gamma"
    ));
}

#[test]
fn workspace_serializes_default_agent_when_set() {
    let ws = WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/tmp/x".to_owned(),
        default_agent: Some(jackin_core::Agent::Codex),
        ..Default::default()
    };

    let toml_str = toml::to_string(&ws).unwrap();
    assert!(toml_str.contains("default_agent = \"codex\""));
}

#[test]
fn workspace_omits_default_agent_field_when_unset() {
    let ws = WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/tmp/x".to_owned(),
        ..Default::default()
    };

    let toml_str = toml::to_string(&ws).unwrap();
    assert!(!toml_str.contains("default_agent"));
}

#[test]
fn workspace_resolves_to_claude_when_unset() {
    let ws = WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/tmp/x".to_owned(),
        ..Default::default()
    };
    assert_eq!(ws.resolved_agent(), jackin_core::Agent::Claude);
}

#[test]
fn workspace_resolves_to_codex_when_set() {
    let ws = WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/tmp/x".to_owned(),
        default_agent: Some(jackin_core::Agent::Codex),
        ..Default::default()
    };
    assert_eq!(ws.resolved_agent(), jackin_core::Agent::Codex);
}

#[test]
fn keep_awake_defaults_to_disabled_when_section_omitted() {
    let toml_str = r#"
workdir = "/workspace/project"

[[mounts]]
src = "/tmp/project"
dst = "/workspace/project"
"#;
    let ws: WorkspaceConfig = toml::from_str(toml_str).unwrap();
    assert!(!ws.keep_awake.enabled);
}

#[test]
fn keep_awake_enabled_round_trips_through_toml() {
    let toml_str = r#"
workdir = "/workspace/project"

[[mounts]]
src = "/tmp/project"
dst = "/workspace/project"

[keep_awake]
enabled = true
"#;
    let ws: WorkspaceConfig = toml::from_str(toml_str).unwrap();
    assert!(ws.keep_awake.enabled);

    let serialized = toml::to_string(&ws).unwrap();
    assert!(
        serialized.contains("[keep_awake]") && serialized.contains("enabled = true"),
        "expected serialized form to contain [keep_awake] enabled = true, got:\n{serialized}"
    );

    // Default (disabled) variant must round-trip back to "no section emitted"
    // so existing configs don't grow noise after a load/save cycle.
    let mut default_ws = ws;
    default_ws.keep_awake.enabled = false;
    let serialized_default = toml::to_string(&default_ws).unwrap();
    assert!(
        !serialized_default.contains("keep_awake"),
        "disabled keep_awake should be skipped during serialization, got:\n{serialized_default}"
    );
}

#[test]
fn keep_awake_rejects_unknown_fields_under_section() {
    let toml_str = r#"
workdir = "/workspace/project"

[[mounts]]
src = "/tmp/project"
dst = "/workspace/project"

[keep_awake]
enabled = true
mystery_field = 7
"#;
    let err = toml::from_str::<WorkspaceConfig>(toml_str).unwrap_err();
    assert!(
        err.to_string().contains("mystery_field"),
        "expected error to name the unknown field, got: {err}"
    );
}

#[test]
fn validate_workdir_equal_to_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace/project", "/workspace/project");
    validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap();
}

#[test]
fn validate_workdir_inside_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace/project/src", "/workspace/project");
    validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap();
}

#[test]
fn validate_workdir_deeply_nested_inside_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace/project/src/main", "/workspace/project");
    validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap();
}

#[test]
fn validate_workdir_parent_of_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace", "/workspace/project");
    validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap();
}

#[test]
fn validate_workdir_grandparent_of_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace", "/workspace/project/src");
    validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap();
}

#[test]
fn validate_workdir_parent_with_trailing_slash_on_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace", "/workspace/project/");
    validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap();
}

#[test]
fn validate_rejects_workdir_sibling_of_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace/other", "/workspace/project");
    let err = validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap_err();
    assert!(err.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn validate_rejects_workdir_with_prefix_overlap_but_not_parent() {
    // /workspace/project-v2 is NOT inside /workspace/project
    let ws = workspace_with_workdir_and_dst("/workspace/project-v2", "/workspace/project");
    let err = validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap_err();
    assert!(err.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn validate_rejects_mount_dst_with_prefix_overlap_but_not_child() {
    // /workspace/project is NOT a parent of /workspace/project-v2
    let ws = workspace_with_workdir_and_dst("/workspace/project", "/workspace/project-v2");
    let err = validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap_err();
    assert!(err.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn validate_rejects_completely_unrelated_workdir() {
    let ws = workspace_with_workdir_and_dst("/home/user", "/workspace/project");
    let err = validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap_err();
    assert!(err.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn validate_workdir_parent_of_any_mount_dst() {
    let ws = WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![
            MountConfig {
                src: "/tmp/a".to_owned(),
                dst: "/other/path".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            MountConfig {
                src: "/tmp/b".to_owned(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        ..Default::default()
    };
    validate_workspace_config(&WorkspaceName::parse("test").unwrap(), &ws).unwrap();
}

#[test]
fn mount_config_defaults_isolation_to_shared() {
    let toml = r#"src = "/tmp/src"
dst = "/workspace/x"
"#;
    let mount: MountConfig = toml::from_str(toml).unwrap();
    assert_eq!(mount.isolation, MountIsolation::Shared);
}

#[test]
fn mount_config_parses_worktree_isolation() {
    let toml = r#"src = "/tmp/src"
dst = "/workspace/x"
isolation = "worktree"
"#;
    let mount: MountConfig = toml::from_str(toml).unwrap();
    assert_eq!(mount.isolation, MountIsolation::Worktree);
}

#[test]
fn mount_config_parses_clone_isolation() {
    let toml = r#"src = "/tmp/src"
dst = "/workspace/x"
isolation = "clone"
"#;
    let mount: MountConfig = toml::from_str(toml).unwrap();
    assert_eq!(mount.isolation, MountIsolation::Clone);
}

#[test]
fn mount_config_writes_isolation_field_even_when_shared_on_serialize() {
    // Old configs without `isolation` deserialize to Shared (the default);
    // on save we re-emit the field explicitly so the stored TOML always
    // names the isolation level. No surprises for operators reading the
    // config — every mount shows what it is.
    let mount = MountConfig {
        src: "/tmp/src".into(),
        dst: "/workspace/x".into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    };
    let serialized = toml::to_string(&mount).unwrap();
    assert!(
        serialized.contains(r#"isolation = "shared""#),
        "serialized = {serialized:?}"
    );
}

#[test]
fn mount_config_emits_isolation_field_when_non_shared_on_serialize() {
    let mount = MountConfig {
        src: "/tmp/src".into(),
        dst: "/workspace/x".into(),
        readonly: false,
        isolation: MountIsolation::Worktree,
    };
    let serialized = toml::to_string(&mount).unwrap();
    assert!(serialized.contains(r#"isolation = "worktree""#));
}

#[test]
fn isolation_layout_allows_one_worktree_plus_n_shared() {
    let mounts = vec![
        worktree_mount("/tmp/a", "/workspace/a"),
        shared_mount("/tmp/cache", "/workspace/cache"),
    ];
    validate_isolation_layout(&mounts).unwrap();
}

#[test]
fn isolation_layout_allows_sibling_worktrees() {
    let mounts = vec![
        worktree_mount("/tmp/a", "/workspace/a"),
        worktree_mount("/tmp/b", "/workspace/b"),
    ];
    validate_isolation_layout(&mounts).unwrap();
}

#[test]
fn isolation_layout_allows_isolated_parent_with_shared_child() {
    let mounts = vec![
        worktree_mount("/tmp/proj", "/workspace/proj"),
        shared_mount("/tmp/proj-target", "/workspace/proj/target"),
    ];
    validate_isolation_layout(&mounts).unwrap();
}

#[test]
fn isolation_layout_rejects_nested_worktrees_parent_child() {
    let mounts = vec![
        worktree_mount("/tmp/proj", "/workspace/proj"),
        worktree_mount("/tmp/sub", "/workspace/proj/sub"),
    ];
    let err = validate_isolation_layout(&mounts).unwrap_err().to_string();
    assert!(err.contains("/workspace/proj"), "missing parent dst: {err}");
    assert!(
        err.contains("/workspace/proj/sub"),
        "missing child dst: {err}"
    );
}

#[test]
fn isolation_layout_rejects_nested_worktrees_grandparent() {
    let mounts = vec![
        worktree_mount("/tmp/a", "/workspace"),
        worktree_mount("/tmp/b", "/workspace/proj/sub"),
    ];
    let err = validate_isolation_layout(&mounts).unwrap_err().to_string();
    assert!(err.contains("/workspace") && err.contains("/workspace/proj/sub"));
}

#[test]
fn isolation_layout_rejects_two_worktree_mounts_on_same_repo() {
    // V1 limitation: two isolated mounts in one workspace cannot
    // share the same host repository (literal `src` equality is
    // sufficient when the path can't be canonicalized — the case
    // exercised by this test).
    let mounts = vec![
        worktree_mount("/host/jackin", "/workspace/jackin"),
        worktree_mount("/host/jackin", "/workspace/jackin-copy"),
    ];
    let err = validate_isolation_layout(&mounts).unwrap_err().to_string();
    assert!(
        err.contains("same host repository"),
        "expected same-host-repo error; got: {err}"
    );
    assert!(err.contains("/workspace/jackin"));
    assert!(err.contains("/workspace/jackin-copy"));
    assert!(err.contains("/host/jackin"));
}

#[test]
fn isolation_layout_allows_different_host_repos_in_one_workspace() {
    // The common multi-mount case: role works on two different
    // host repos, each isolated. Distinct `src` paths → no
    // collision in host's `.git/worktrees/` namespace.
    let mounts = vec![
        worktree_mount("/host/jackin", "/workspace/jackin"),
        worktree_mount("/host/jackin-docs", "/workspace/jackin-docs"),
    ];
    validate_isolation_layout(&mounts).unwrap();
}

#[test]
fn isolation_layout_allows_two_clone_mounts_on_same_repo() {
    let mounts = vec![
        clone_mount("/host/jackin", "/workspace/jackin"),
        clone_mount("/host/jackin", "/workspace/jackin-copy"),
    ];
    validate_isolation_layout(&mounts).unwrap();
}
