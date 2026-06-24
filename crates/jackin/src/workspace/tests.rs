//! Tests for `workspace`.
use super::*;

fn ws_with_allowed(allowed: Vec<String>) -> WorkspaceConfig {
    WorkspaceConfig {
        allowed_roles: allowed,
        ..WorkspaceConfig::default()
    }
}

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

// -- validate_workspace_config: workdir vs mount destination coverage ------

fn workspace_with_workdir_and_dst(workdir: &str, dst: &str) -> WorkspaceConfig {
    WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: workdir.to_owned(),
        mounts: vec![MountConfig {
            src: "/tmp/src".to_owned(),
            dst: dst.to_owned(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }],
        ..Default::default()
    }
}

#[test]
fn workspace_serializes_default_agent_when_set() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/tmp/x".to_owned(),
        default_agent: Some(crate::agent::Agent::Codex),
        ..Default::default()
    };

    let toml_str = toml::to_string(&ws).unwrap();
    assert!(toml_str.contains("default_agent = \"codex\""));
}

#[test]
fn workspace_omits_default_agent_field_when_unset() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/tmp/x".to_owned(),
        ..Default::default()
    };

    let toml_str = toml::to_string(&ws).unwrap();
    assert!(!toml_str.contains("default_agent"));
}

#[test]
fn workspace_resolves_to_claude_when_unset() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/tmp/x".to_owned(),
        ..Default::default()
    };
    assert_eq!(ws.resolved_agent(), crate::agent::Agent::Claude);
}

#[test]
fn workspace_resolves_to_codex_when_set() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/tmp/x".to_owned(),
        default_agent: Some(crate::agent::Agent::Codex),
        ..Default::default()
    };
    assert_eq!(ws.resolved_agent(), crate::agent::Agent::Codex);
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
    validate_workspace_config("test", &ws).unwrap();
}

#[test]
fn validate_workdir_inside_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace/project/src", "/workspace/project");
    validate_workspace_config("test", &ws).unwrap();
}

#[test]
fn validate_workdir_deeply_nested_inside_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace/project/src/main", "/workspace/project");
    validate_workspace_config("test", &ws).unwrap();
}

#[test]
fn validate_workdir_parent_of_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace", "/workspace/project");
    validate_workspace_config("test", &ws).unwrap();
}

#[test]
fn validate_workdir_grandparent_of_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace", "/workspace/project/src");
    validate_workspace_config("test", &ws).unwrap();
}

#[test]
fn validate_workdir_parent_with_trailing_slash_on_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace", "/workspace/project/");
    validate_workspace_config("test", &ws).unwrap();
}

#[test]
fn validate_rejects_workdir_sibling_of_mount_dst() {
    let ws = workspace_with_workdir_and_dst("/workspace/other", "/workspace/project");
    let err = validate_workspace_config("test", &ws).unwrap_err();
    assert!(err.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn validate_rejects_workdir_with_prefix_overlap_but_not_parent() {
    // /workspace/project-v2 is NOT inside /workspace/project
    let ws = workspace_with_workdir_and_dst("/workspace/project-v2", "/workspace/project");
    let err = validate_workspace_config("test", &ws).unwrap_err();
    assert!(err.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn validate_rejects_mount_dst_with_prefix_overlap_but_not_child() {
    // /workspace/project is NOT a parent of /workspace/project-v2
    let ws = workspace_with_workdir_and_dst("/workspace/project", "/workspace/project-v2");
    let err = validate_workspace_config("test", &ws).unwrap_err();
    assert!(err.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn validate_rejects_completely_unrelated_workdir() {
    let ws = workspace_with_workdir_and_dst("/home/user", "/workspace/project");
    let err = validate_workspace_config("test", &ws).unwrap_err();
    assert!(err.to_string().contains(
        "must be equal to, inside, or a parent of one of the workspace mount destinations"
    ));
}

#[test]
fn validate_workdir_parent_of_any_mount_dst() {
    let ws = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![
            MountConfig {
                src: "/tmp/a".to_owned(),
                dst: "/other/path".to_owned(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
            MountConfig {
                src: "/tmp/b".to_owned(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        ],
        ..Default::default()
    };
    validate_workspace_config("test", &ws).unwrap();
}

use crate::isolation::MountIsolation;

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

fn worktree_mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: MountIsolation::Worktree,
    }
}

fn clone_mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: MountIsolation::Clone,
    }
}

fn shared_mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    }
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

/// Pin the wiring: `validate_workspace_config` must call
/// `validate_isolation_layout` so isolation rejections actually
/// propagate through the public validation entrypoint. If the call
/// site at L174 is ever refactored away, every isolation rejection
/// would silently become a no-op (only catchable at materialize
/// time, after the operator has already saved a broken config).
#[test]
fn validate_workspace_config_surfaces_isolation_layout_errors() {
    use std::collections::BTreeMap;
    let workspace = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
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
        claude: None,
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        github: None,
        git_pull_on_entry: false,
        dirty_exit_policy: None,
    };
    let err = validate_workspace_config("ws", &workspace).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("nested inside"),
        "validate_workspace_config must surface the nested-worktrees error from validate_isolation_layout; got: {msg}",
    );
}

#[test]
fn parse_workspace_with_agent_auth_blocks() {
    let toml = r#"
workdir = "/tmp/proj"
allowed_roles = ["smith"]

[claude]
auth_forward = "api_key"

[codex]
auth_forward = "sync"

[amp]
auth_forward = "api_key"
"#;
    let cfg: WorkspaceConfig = toml::from_str(toml).unwrap();
    assert_eq!(
        cfg.claude.as_ref().unwrap().auth_forward,
        crate::config::AuthForwardMode::ApiKey,
    );
    assert_eq!(
        cfg.codex.as_ref().unwrap().auth_forward,
        crate::config::AuthForwardMode::Sync,
    );
    assert_eq!(
        cfg.amp.as_ref().unwrap().auth_forward,
        crate::config::AuthForwardMode::ApiKey,
    );
}

// ── Legacy bare op:// migration regression ───────────────────────────────

/// Pre-Task-3 workspaces may contain bare `op://Vault/Item/Field`
/// strings written as scalar TOML values (not the inline-table
/// `{ op = "...", path = "..." }` shape produced by the picker).
/// They must deserialize without error as `EnvValue::Plain` so the
/// user's config remains loadable; at the operator's pace they can
/// re-pick via the TUI to get the pinned-UUID form.
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
        &crate::operator_env::EnvValue::Plain("op://Vault/Item/Field".into()),
        "bare op:// scalar must deserialize as Plain, not OpRef",
    );
}

#[test]
fn parse_workspace_role_override_with_agent_auth() {
    let toml = r#"
workdir = "/tmp/proj"
allowed_roles = ["smith"]

[roles.smith]
[roles.smith.claude]
auth_forward = "oauth_token"
[roles.smith.codex]
auth_forward = "ignore"
[roles.smith.amp]
auth_forward = "api_key"
"#;
    let cfg: WorkspaceConfig = toml::from_str(toml).unwrap();
    let smith = cfg.roles.get("smith").expect("smith role must be present");
    assert_eq!(
        smith.claude.as_ref().unwrap().auth_forward,
        crate::config::AuthForwardMode::OAuthToken,
    );
    assert_eq!(
        smith.codex.as_ref().unwrap().auth_forward,
        crate::config::AuthForwardMode::Ignore,
    );
    assert_eq!(
        smith.amp.as_ref().unwrap().auth_forward,
        crate::config::AuthForwardMode::ApiKey,
    );
}

#[test]
fn parse_workspace_without_agent_auth_blocks() {
    let toml = r#"
workdir = "/tmp/proj"
allowed_roles = ["smith"]
"#;
    let cfg: WorkspaceConfig = toml::from_str(toml).unwrap();
    assert!(
        cfg.claude.is_none(),
        "WorkspaceConfig.claude must default to None"
    );
    assert!(
        cfg.codex.is_none(),
        "WorkspaceConfig.codex must default to None"
    );
    assert!(
        cfg.amp.is_none(),
        "WorkspaceConfig.amp must default to None"
    );
}

#[test]
fn reject_codex_oauth_token_in_workspace() {
    // Phase 3: post-parse validation replaces the serde newtype check.
    let toml = r#"
workdir = "/tmp/proj"
allowed_roles = ["smith"]

[codex]
auth_forward = "oauth_token"
"#;
    let cfg = toml::from_str::<WorkspaceConfig>(toml).expect("parse should succeed");
    let err = cfg.validate_auth_modes().expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("not supported for codex"),
        "expected codex-rejection message, got: {msg}"
    );
}

#[test]
fn reject_codex_oauth_token_in_workspace_role_override() {
    // Phase 3: post-parse validation replaces the serde newtype check.
    let toml = r#"
workdir = "/tmp/proj"
allowed_roles = ["smith"]

[roles.smith]
[roles.smith.codex]
auth_forward = "oauth_token"
"#;
    let cfg = toml::from_str::<WorkspaceConfig>(toml).expect("parse should succeed");
    let err = cfg.validate_auth_modes().expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("not supported for codex"),
        "expected codex-rejection message, got: {msg}"
    );
}

#[test]
fn reject_amp_oauth_token_in_workspace() {
    // Phase 3: post-parse validation replaces the serde newtype check.
    let toml = r#"
workdir = "/tmp/proj"
allowed_roles = ["smith"]

[amp]
auth_forward = "oauth_token"
"#;
    let cfg = toml::from_str::<WorkspaceConfig>(toml).expect("parse should succeed");
    let err = cfg.validate_auth_modes().expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("not supported for amp"),
        "expected amp-rejection message, got: {msg}"
    );
}

#[test]
fn reject_amp_oauth_token_in_workspace_role_override() {
    // Phase 3: post-parse validation replaces the serde newtype check.
    let toml = r#"
workdir = "/tmp/proj"
allowed_roles = ["smith"]

[roles.smith]
[roles.smith.amp]
auth_forward = "oauth_token"
"#;
    let cfg = toml::from_str::<WorkspaceConfig>(toml).expect("parse should succeed");
    let err = cfg.validate_auth_modes().expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("not supported for amp"),
        "expected amp-rejection message, got: {msg}"
    );
}

#[test]
fn parse_workspace_role_override_without_agent_auth() {
    let toml = r#"
workdir = "/tmp/proj"
allowed_roles = ["smith"]

[roles.smith]
"#;
    let cfg: WorkspaceConfig = toml::from_str(toml).unwrap();
    let smith = cfg.roles.get("smith").expect("smith role must be present");
    assert!(
        smith.claude.is_none(),
        "role override claude must default to None"
    );
    assert!(
        smith.codex.is_none(),
        "role override codex must default to None"
    );
    assert!(
        smith.amp.is_none(),
        "role override amp must default to None"
    );
}

/// An `op://` env value round-trips its per-ref `account`.
#[test]
fn workspace_op_ref_round_trips_account() {
    use crate::operator_env::{EnvValue, OpRef};
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "TOKEN".to_owned(),
        EnvValue::OpRef(OpRef {
            op: "op://v/i/f".into(),
            path: "Vault/Item/Field".into(),
            account: Some("ACCT123".into()),
        }),
    );
    let original = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
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

/// An `op://` env value with no account omits the `account` key.
#[test]
fn workspace_op_ref_omits_account_when_none() {
    use crate::operator_env::{EnvValue, OpRef};
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "TOKEN".to_owned(),
        EnvValue::OpRef(OpRef {
            op: "op://v/i/f".into(),
            path: "Vault/Item/Field".into(),
            account: None,
        }),
    );
    let cfg = WorkspaceConfig {
        version: crate::config::CURRENT_WORKSPACE_VERSION.to_owned(),
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
