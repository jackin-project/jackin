pub mod mounts;
pub mod paths;
pub(crate) mod planner;
pub mod resolve;
pub mod sensitive;

pub use mounts::{
    parse_mount_spec, parse_mount_spec_resolved, validate_mount_paths, validate_mount_specs,
    validate_mounts,
};
pub use paths::{expand_tilde, resolve_path};
pub use planner::{CollapseError, CollapsePlan, Removal, plan_collapse};
pub use resolve::{
    LoadWorkspaceInput, ResolvedWorkspace, current_dir_workspace, resolve_load_workspace,
    saved_workspace_match_depth,
};
pub use sensitive::{SensitiveMount, confirm_sensitive_mounts, find_sensitive_mounts};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
    /// Old configs without this field deserialize to `MountIsolation::Shared`
    /// (the enum default). On save we always write the field — even when it's
    /// the default — so the stored TOML is explicit and old configs migrate to
    /// the new shape on first save instead of silently retaining their
    /// pre-isolation form.
    #[serde(default)]
    pub isolation: crate::isolation::MountIsolation,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConfig {
    pub workdir: String,
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
    #[serde(default)]
    pub allowed_agents: Vec<String>,
    #[serde(default)]
    pub default_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent: Option<String>,
    /// Workspace-level operator env map. Keys are env var names;
    /// values use the `operator_env` dispatch syntax
    /// (`op://...` | `$NAME` | `${NAME}` | literal).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    /// Per-(workspace × agent) env overrides, keyed by the agent
    /// selector (e.g. `"agent-smith"` or `"chainargos/agent-brown"`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub agents: std::collections::BTreeMap<String, WorkspaceAgentOverride>,
    #[serde(default, skip_serializing_if = "KeepAwakeConfig::is_default")]
    pub keep_awake: KeepAwakeConfig,
}

/// Per-workspace power-management opt-in.
///
/// macOS-only today: when `enabled = true`, jackin spawns
/// `caffeinate -imsu` while at least one agent is running in any
/// workspace with this flag set, so the host stays awake while
/// agents work in the background. The reconciler runs at every
/// jackin command boundary; one caffeinate process covers all
/// active keep-awake workspaces.
///
/// Linux/Windows support (e.g. `systemd-inhibit`) is intentionally
/// out of scope until a non-mac user reports needing it. Adding
/// fields here is a schema change because the struct is
/// `deny_unknown_fields` — extend it intentionally rather than
/// silently accepting new keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeepAwakeConfig {
    #[serde(default)]
    pub enabled: bool,
}

impl KeepAwakeConfig {
    const fn is_default(&self) -> bool {
        !self.enabled
    }
}

/// Per-(workspace × agent) operator overrides.
///
/// Currently only `env` is supported; the struct exists as a named type
/// so future overrides (e.g. `auth_forward`) can be added without a
/// TOML schema break.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceAgentOverride {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    pub workdir: Option<String>,
    pub upsert_mounts: Vec<MountConfig>,
    pub remove_destinations: Vec<String>,
    pub no_workdir_mount: bool,
    pub allowed_agents_to_add: Vec<String>,
    pub allowed_agents_to_remove: Vec<String>,
    pub default_agent: Option<Option<String>>,
    pub mount_isolation_overrides: Vec<(String, crate::isolation::MountIsolation)>,
    /// Toggle for the macOS keep-awake reconciler. `None` = no change,
    /// `Some(true)` = opt in, `Some(false)` = opt out. The CLI's paired
    /// `--keep-awake` / `--no-keep-awake` flags map onto this; the TUI
    /// derives it by diffing `pending.keep_awake` vs `original`.
    pub keep_awake_enabled: Option<bool>,
}

/// Validate the isolation layout for a workspace's mounts. Two rules
/// today, both worktree-specific:
///
/// 1. **No nested isolated mounts.** Two `Worktree` mounts where one's
///    `dst` is a strict ancestor of the other's are rejected. The
///    inner worktree's `.git` would land inside the outer worktree's
///    tree, which is unsafe regardless of mode.
///
/// 2. **No same-host-repo isolated siblings.** Two `Worktree` mounts
///    that resolve to the same host repository are rejected. Each
///    isolated worktree creates an admin entry under
///    `<host_repo>/.git/worktrees/<n>/`, and our naming uses the
///    container name as `<n>` so admin entries are globally unique
///    per (`host_repo`, container). Allowing two isolated mounts on the
///    same host repo in one container would force `<container>` to
///    appear twice in that namespace; that case is rare/unmotivated
///    in practice (no operator workflow has surfaced for it). Reject
///    upfront. Revisit if a real use case shows up.
pub fn validate_isolation_layout(mounts: &[MountConfig]) -> anyhow::Result<()> {
    use crate::isolation::MountIsolation;

    let isolated: Vec<(usize, &MountConfig, &str)> = mounts
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m.isolation, MountIsolation::Worktree))
        .map(|(i, m)| (i, m, m.dst.trim_end_matches('/')))
        .collect();

    for (i, (_, ma, a)) in isolated.iter().enumerate() {
        for (_, mb, b) in &isolated[i + 1..] {
            // Rule 1: nested dst paths.
            if is_strict_ancestor(a, b) || is_strict_ancestor(b, a) {
                anyhow::bail!(
                    "isolated mount `{b}` cannot be nested inside isolated mount `{a}`; \
                     either make the inner mount `shared` or move the inner mount outside \
                     the parent's path",
                    a = if is_strict_ancestor(a, b) { a } else { b },
                    b = if is_strict_ancestor(a, b) { b } else { a },
                );
            }
            // Rule 2: same host repo (same `src` after canonicalization
            // best-effort; falls back to literal string equality if a
            // path can't be canonicalized — e.g., `src` doesn't exist
            // yet on disk).
            if same_host_repo(&ma.src, &mb.src) {
                anyhow::bail!(
                    "isolated mounts `{}` and `{}` cannot share the same host repository `{}`; \
                     remove one of them or change one to `shared` (V1 limitation — see roadmap)",
                    ma.dst,
                    mb.dst,
                    ma.src,
                );
            }
        }
    }
    Ok(())
}

fn same_host_repo(a: &str, b: &str) -> bool {
    let ca = std::fs::canonicalize(a).ok();
    let cb = std::fs::canonicalize(b).ok();
    match (ca, cb) {
        (Some(x), Some(y)) => x == y,
        // If either path can't be canonicalized (e.g. doesn't exist
        // on disk yet during planning), fall back to literal string
        // equality. Stricter checks happen later at materialize time.
        _ => a == b,
    }
}

fn is_strict_ancestor(parent: &str, child: &str) -> bool {
    let parent = parent.trim_end_matches('/');
    let child = child.trim_end_matches('/');
    if parent == child {
        return false;
    }
    let prefix = format!("{parent}/");
    child.starts_with(&prefix)
}

pub fn validate_workspace_config(name: &str, workspace: &WorkspaceConfig) -> anyhow::Result<()> {
    if workspace.workdir.is_empty() {
        anyhow::bail!("workspace {name:?} must define workdir");
    }
    if !workspace.workdir.starts_with('/') {
        anyhow::bail!("workspace {name:?} workdir must be an absolute container path");
    }
    if workspace.mounts.is_empty() {
        anyhow::bail!("workspace {name:?} must define at least one mount");
    }

    validate_mount_specs(&workspace.mounts)?;
    validate_isolation_layout(&workspace.mounts)?;

    let covers_workdir = workspace.mounts.iter().any(|mount| {
        let dst = mount.dst.trim_end_matches('/');
        workspace.workdir == dst
            || workspace.workdir.starts_with(&format!("{dst}/"))
            || dst.starts_with(&format!("{}/", workspace.workdir.trim_end_matches('/')))
    });
    anyhow::ensure!(
        covers_workdir,
        "workspace {name:?} workdir must be equal to, inside, or a parent of one of the workspace mount destinations"
    );

    if let Some(default_agent) = &workspace.default_agent
        && !workspace.allowed_agents.is_empty()
        && !workspace
            .allowed_agents
            .iter()
            .any(|agent| agent == default_agent)
    {
        anyhow::bail!(
            "workspace {name:?} default_agent must be a member of allowed_agents when allowed_agents is set"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- validate_workspace_config: workdir vs mount destination coverage ------

    fn workspace_with_workdir_and_dst(workdir: &str, dst: &str) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: workdir.to_string(),
            mounts: vec![MountConfig {
                src: "/tmp/src".to_string(),
                dst: dst.to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..Default::default()
        }
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
        let ws =
            workspace_with_workdir_and_dst("/workspace/project/src/main", "/workspace/project");
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
            workdir: "/workspace".to_string(),
            mounts: vec![
                MountConfig {
                    src: "/tmp/a".to_string(),
                    dst: "/other/path".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
                MountConfig {
                    src: "/tmp/b".to_string(),
                    dst: "/workspace/project".to_string(),
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
        // The common multi-mount case: agent works on two different
        // host repos, each isolated. Distinct `src` paths → no
        // collision in host's `.git/worktrees/` namespace.
        let mounts = vec![
            worktree_mount("/host/jackin", "/workspace/jackin"),
            worktree_mount("/host/jackin-docs", "/workspace/jackin-docs"),
        ];
        validate_isolation_layout(&mounts).unwrap();
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
            workdir: "/workspace/proj".into(),
            mounts: vec![
                worktree_mount("/tmp/a", "/workspace/proj"),
                worktree_mount("/tmp/b", "/workspace/proj/sub"),
            ],
            allowed_agents: Vec::new(),
            default_agent: None,
            last_agent: None,
            env: BTreeMap::new(),
            agents: BTreeMap::new(),
            keep_awake: KeepAwakeConfig::default(),
        };
        let err = validate_workspace_config("ws", &workspace).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("nested inside"),
            "validate_workspace_config must surface the nested-worktrees error from validate_isolation_layout; got: {msg}",
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
}
