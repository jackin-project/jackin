// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn isolation_layout_rejects_nested_clone_mounts() {
    let mounts = vec![
        clone_mount("/tmp/proj", "/workspace/proj"),
        clone_mount("/tmp/sub", "/workspace/proj/sub"),
    ];
    let err = validate_isolation_layout(&mounts).unwrap_err().to_string();
    assert!(err.contains("nested inside"), "got: {err}");
}

#[test]
fn isolation_layout_ignores_trailing_slashes() {
    let mounts = vec![
        worktree_mount("/tmp/a", "/workspace/proj/"),
        worktree_mount("/tmp/b", "/workspace/proj/sub/"),
    ];
    let err = validate_isolation_layout(&mounts).unwrap_err().to_string();
    assert!(err.contains("/workspace/proj"));
}

#[test]
fn validate_workspace_config_surfaces_isolation_layout_errors() {
    use std::collections::BTreeMap;
    let workspace = WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/proj".into(),
        mounts: vec![
            worktree_mount("/tmp/a", "/workspace/proj"),
            worktree_mount("/tmp/b", "/workspace/proj/sub"),
        ],
        allowed_roles: Vec::new(),
        default_role: None,
        default_agent: None,
        last_role: None,
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
        keep_awake: KeepAwakeConfig::default(),
        accounts: Vec::new(),
        account_bindings: BTreeMap::new(),
        github: None,
        git_pull_on_entry: false,
        runtime: jackin_config::WorkspaceRuntimeConfig::default(),
        dirty_exit_policy: None,
        docker: None,
        default_launch: None,
    };
    let err =
        validate_workspace_config(&WorkspaceName::parse("ws").unwrap(), &workspace).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("nested inside"),
        "validate_workspace_config must surface the nested-worktrees error from validate_isolation_layout; got: {msg}",
    );
}

#[test]
fn legacy_bare_op_uri_in_workspace_loads_as_plain_no_error() {
    let toml_input = r#"
workdir = "/workspace/proj"

[[mounts]]
src = "/tmp/proj"
dst = "/workspace/proj"

[env]
OLD = "op://Vault/Item/Field"
"#;
    let ws: WorkspaceConfig = toml::from_str(toml_input).expect("must parse");
    assert_eq!(
        ws.env.get("OLD").expect("OLD env var present"),
        &jackin_core::EnvValue::Plain("op://Vault/Item/Field".into()),
        "bare op:// scalar must deserialize as Plain, not OpRef",
    );
}

#[test]
fn workspace_op_ref_round_trips_account() {
    use jackin_core::{EnvValue, OpRef};
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "TOKEN".to_owned(),
        EnvValue::OpRef(OpRef {
            op: "op://v/i/f".into(),
            path: "Vault/Item/Field".into(),
            account: Some("ACCT123".into()),
            on_demand: false,
        }),
    );
    let original = WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/x".into(),
        env,
        ..Default::default()
    };
    let serialized = toml::to_string(&original).expect("serialize");
    assert!(
        serialized.contains(r#"account = "ACCT123""#),
        "serialized op ref must carry account, got:\n{serialized}"
    );
    let parsed: WorkspaceConfig = toml::from_str(&serialized).expect("re-deserialize");
    let EnvValue::OpRef(r) = parsed.env.get("TOKEN").expect("TOKEN present") else {
        panic!("TOKEN must round-trip as an OpRef");
    };
    assert_eq!(r.account, Some("ACCT123".into()));
}

#[test]
fn workspace_op_ref_omits_account_when_none() {
    use jackin_core::{EnvValue, OpRef};
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "TOKEN".to_owned(),
        EnvValue::OpRef(OpRef {
            op: "op://v/i/f".into(),
            path: "Vault/Item/Field".into(),
            account: None,
            on_demand: false,
        }),
    );
    let cfg = WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/x".into(),
        env,
        ..Default::default()
    };
    let s = toml::to_string(&cfg).unwrap();
    assert!(
        !s.contains("account"),
        "op ref with no account must not serialize an account key, got:\n{s}"
    );
}

#[test]
fn workspace_account_bindings_round_trip() {
    let value = r#"workdir = "/work"
accounts = ["work"]
[account_bindings]
codex = "work"
[roles.smith.account_bindings]
codex = "work"
"#;
    let workspace: WorkspaceConfig = toml::from_str(value).unwrap();
    assert_eq!(workspace.accounts, ["work"]);
    assert_eq!(
        workspace.account_bindings[&jackin_core::Agent::Codex],
        "work"
    );
    assert_eq!(
        workspace.roles["smith"].account_bindings[&jackin_core::Agent::Codex],
        "work"
    );
    let round_trip: WorkspaceConfig =
        toml::from_str(&toml::to_string(&workspace).unwrap()).unwrap();
    assert_eq!(workspace, round_trip);
}
