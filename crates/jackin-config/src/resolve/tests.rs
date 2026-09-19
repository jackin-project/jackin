// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `resolve`.
use super::*;
use jackin_core::{Agent, MountIsolation, WorkspaceName};
use tempfile::tempdir;

use crate::AppConfig;
use crate::schema::{MountHealReport, RoleSource};

#[test]
fn current_dir_workspace_uses_same_host_and_container_path() {
    let dir = tempdir().unwrap();
    let workspace = current_dir_workspace(dir.path()).unwrap();

    assert_eq!(
        workspace.workdir,
        dir.path().canonicalize().unwrap().display().to_string()
    );
    assert_eq!(workspace.mounts.len(), 1);
    assert_eq!(workspace.mounts[0].src, workspace.mounts[0].dst);
}

#[test]
fn saved_workspace_resolution_preserves_agent() {
    let temp = tempdir().unwrap();
    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();
    let canonical = workspace_root.canonicalize().unwrap();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "codex-workspace".to_owned(),
        WorkspaceConfig {
            workdir: "/workspace/project".to_owned(),
            mounts: vec![MountConfig {
                src: canonical.display().to_string(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            default_agent: Some(Agent::Codex),
            ..Default::default()
        },
    );

    let resolved = resolve_load_workspace(
        &config,
        &RoleSelector::new(None, "agent-smith"),
        &canonical,
        LoadWorkspaceInput::Saved("codex-workspace".to_owned()),
        &[],
    )
    .unwrap();

    assert_eq!(resolved.default_agent, Some(Agent::Codex));
}

#[test]
fn saved_workspace_match_depth_matches_host_workdir_parent_of_mounts() {
    let temp = tempdir().unwrap();
    let workspace_root = temp.path().join("monorepo");
    let repo_a = workspace_root.join("jackin");
    let repo_b = workspace_root.join("jackin-dev");
    std::fs::create_dir_all(&repo_a).unwrap();
    std::fs::create_dir_all(&repo_b).unwrap();

    let canonical_root = workspace_root.canonicalize().unwrap();
    let workspace = WorkspaceConfig {
        workdir: canonical_root.display().to_string(),
        mounts: vec![
            MountConfig {
                src: repo_a.canonicalize().unwrap().display().to_string(),
                dst: "/workspace/jackin".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            MountConfig {
                src: repo_b.canonicalize().unwrap().display().to_string(),
                dst: "/workspace/jackin-dev".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        ..Default::default()
    };

    assert_eq!(
        saved_workspace_match_depth(&workspace, &canonical_root),
        Some(canonical_root.components().count())
    );
}

#[test]
fn saved_workspace_match_depth_still_matches_nested_path_under_mount_root() {
    let temp = tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let nested_dir = project_dir.join("src/bin");
    std::fs::create_dir_all(&nested_dir).unwrap();

    let workspace = WorkspaceConfig {
        workdir: "/workspace".to_owned(),
        mounts: vec![MountConfig {
            src: project_dir.canonicalize().unwrap().display().to_string(),
            dst: "/workspace".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        ..Default::default()
    };

    assert_eq!(
        saved_workspace_match_depth(&workspace, &nested_dir),
        Some(project_dir.canonicalize().unwrap().components().count())
    );
}

#[test]
fn saved_workspace_match_depth_rejects_workdir_prefix_only_match() {
    // Broad workdir that is a parent of cwd but not equal to it.
    // The mount source exists and is canonicalized so it is a real
    // candidate — the test confirms the exact-workdir rule rejects the
    // match rather than a silent canonicalize failure on a missing path.
    let temp = tempdir().unwrap();
    let broad_workdir = temp.path().join("Projects");
    let agent_repo = broad_workdir.join("role-repo");
    let unrelated_cwd = broad_workdir.join("jackin4");
    std::fs::create_dir_all(&agent_repo).unwrap();
    std::fs::create_dir_all(&unrelated_cwd).unwrap();

    let workspace = WorkspaceConfig {
        workdir: broad_workdir.canonicalize().unwrap().display().to_string(),
        mounts: vec![MountConfig {
            src: agent_repo.canonicalize().unwrap().display().to_string(),
            dst: "/workspace/role-repo".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        ..Default::default()
    };

    assert_eq!(
        saved_workspace_match_depth(&workspace, &unrelated_cwd),
        None,
        "workdir parent must not match when cwd is an unrelated subdirectory"
    );
}

#[test]
fn saved_workspace_match_depth_matches_exact_workdir() {
    let temp = tempdir().unwrap();
    let workdir = temp.path().join("Projects");
    std::fs::create_dir_all(&workdir).unwrap();
    let canonical = workdir.canonicalize().unwrap();

    let workspace = WorkspaceConfig {
        workdir: canonical.display().to_string(),
        mounts: vec![MountConfig {
            src: canonical.join("repo").display().to_string(),
            dst: "/workspace/repo".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        ..Default::default()
    };

    assert_eq!(
        saved_workspace_match_depth(&workspace, &canonical),
        Some(canonical.components().count()),
    );
}

#[test]
fn saved_workspace_match_depth_matches_nested_path_under_mount_src() {
    let temp = tempdir().unwrap();
    let mount_src = temp.path().join("role-repo");
    let nested = mount_src.join("src");
    std::fs::create_dir_all(&nested).unwrap();

    let workspace = WorkspaceConfig {
        workdir: "/Users/me/Projects".to_owned(),
        mounts: vec![MountConfig {
            src: mount_src.canonicalize().unwrap().display().to_string(),
            dst: "/workspace/role-repo".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        ..Default::default()
    };

    assert_eq!(
        saved_workspace_match_depth(&workspace, &nested),
        Some(mount_src.canonicalize().unwrap().components().count()),
    );
}

#[test]
fn resolves_saved_workspace_and_rejects_disallowed_agent() {
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "big-monorepo".to_owned(),
        WorkspaceConfig {
            workdir: "/workspace/project".to_owned(),
            mounts: vec![MountConfig {
                src: "/tmp/project".to_owned(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            allowed_roles: vec!["agent-smith".to_owned()],
            default_role: Some("agent-smith".to_owned()),
            ..Default::default()
        },
    );

    let cwd = std::env::temp_dir();
    let error = resolve_load_workspace(
        &config,
        &RoleSelector::new(None, "neo"),
        &cwd,
        LoadWorkspaceInput::Saved("big-monorepo".to_owned()),
        &[],
    )
    .unwrap_err();

    assert!(error.to_string().contains("is not allowed by workspace"));
}

#[test]
fn saved_workspace_label_uses_workspace_name() {
    let temp = tempdir().unwrap();
    let mount_src = temp.path().join("project");
    std::fs::create_dir_all(&mount_src).unwrap();

    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "big-monorepo".to_owned(),
        WorkspaceConfig {
            workdir: "/workspace/project".to_owned(),
            mounts: vec![MountConfig {
                src: mount_src.display().to_string(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );

    let cwd = std::env::temp_dir();
    let resolved = resolve_load_workspace(
        &config,
        &RoleSelector::new(None, "agent-smith"),
        &cwd,
        LoadWorkspaceInput::Saved("big-monorepo".to_owned()),
        &[],
    )
    .unwrap();

    assert_eq!(resolved.label, "big-monorepo");
    assert_eq!(resolved.workdir, "/workspace/project");
}

#[test]
fn resolves_same_path_relative_target_to_absolute_workdir() {
    let temp = tempdir().unwrap();
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();

    // The cwd parameter is used to resolve relative paths — no need
    // to mutate the global process CWD.
    let resolved = resolve_load_workspace(
        &AppConfig::default(),
        &RoleSelector::new(None, "agent-smith"),
        temp.path(),
        LoadWorkspaceInput::Path {
            src: "./project".to_owned(),
            dst: "./project".to_owned(),
        },
        &[],
    )
    .unwrap();

    let expected = project_dir.canonicalize().unwrap().display().to_string();
    assert_eq!(resolved.workdir, expected);
    assert_eq!(resolved.mounts[0].dst, expected);
}

#[test]
fn resolves_global_mounts_with_tilde_sources() {
    let home = std::env::var("HOME").unwrap();
    let cwd = tempdir().unwrap();
    let mut config = AppConfig::default();
    config.add_mount(
        "home",
        MountConfig {
            src: "~".to_owned(),
            dst: "/home/agent/home".to_owned(),
            readonly: true,
            isolation: MountIsolation::Shared,
        },
        None,
    );

    let resolved = resolve_load_workspace(
        &config,
        &RoleSelector::new(None, "agent-smith"),
        cwd.path(),
        LoadWorkspaceInput::CurrentDir,
        &[],
    )
    .unwrap();

    assert!(
        resolved
            .mounts
            .iter()
            .any(|mount| mount.dst == "/home/agent/home" && mount.src == home && mount.readonly)
    );
}

#[test]
fn resolve_with_ad_hoc_mounts_merges_correctly() {
    let temp = tempdir().unwrap();
    let mount_src = temp.path().join("project");
    let extra_src = temp.path().join("extra");
    std::fs::create_dir_all(&mount_src).unwrap();
    std::fs::create_dir_all(&extra_src).unwrap();

    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "my-ws".to_owned(),
        WorkspaceConfig {
            workdir: "/workspace/project".to_owned(),
            mounts: vec![MountConfig {
                src: mount_src.display().to_string(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );

    let cwd = std::env::temp_dir();
    let resolved = resolve_load_workspace(
        &config,
        &RoleSelector::new(None, "agent-smith"),
        &cwd,
        LoadWorkspaceInput::Saved("my-ws".to_owned()),
        &[MountConfig {
            src: extra_src.display().to_string(),
            dst: "/extra".to_owned(),
            readonly: true,
            isolation: MountIsolation::Shared,
        }],
    )
    .unwrap();

    assert_eq!(resolved.mounts.len(), 2);
    assert!(
        resolved
            .mounts
            .iter()
            .any(|m| m.dst == "/extra" && m.readonly)
    );
}

#[test]
fn resolve_with_ad_hoc_mount_dst_conflict_errors() {
    let temp = tempdir().unwrap();
    let mount_src = temp.path().join("project");
    std::fs::create_dir_all(&mount_src).unwrap();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "my-ws".to_owned(),
        WorkspaceConfig {
            workdir: "/workspace/project".to_owned(),
            mounts: vec![MountConfig {
                src: mount_src.display().to_string(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );

    let cwd = std::env::temp_dir();
    let error = resolve_load_workspace(
        &config,
        &RoleSelector::new(None, "agent-smith"),
        &cwd,
        LoadWorkspaceInput::Saved("my-ws".to_owned()),
        &[MountConfig {
            src: mount_src.display().to_string(),
            dst: "/workspace/project".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("ad-hoc mount destination conflicts")
    );
}

#[test]
fn resolve_rejects_duplicate_effective_global_workspace_destination() {
    let temp = tempdir().unwrap();
    let workspace_src = temp.path().join("project");
    let global_src = temp.path().join("cache");
    std::fs::create_dir_all(&workspace_src).unwrap();
    std::fs::create_dir_all(&global_src).unwrap();

    let mut config = AppConfig::default();
    config.add_mount(
        "cache",
        MountConfig {
            src: global_src.display().to_string(),
            dst: "/workspace/project".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
        None,
    );
    config.workspaces.insert(
        "my-ws".to_owned(),
        WorkspaceConfig {
            workdir: "/workspace/project".to_owned(),
            mounts: vec![MountConfig {
                src: workspace_src.display().to_string(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );

    let error = resolve_load_workspace(
        &config,
        &RoleSelector::new(None, "agent-smith"),
        &std::env::temp_dir(),
        LoadWorkspaceInput::Saved("my-ws".to_owned()),
        &[],
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("global mount destination conflicts")
    );
}

#[test]
fn resolved_workspace_as_workspace_label_accepts_path_and_stem() {
    let stem = ResolvedWorkspace {
        name: "chainargos".into(),
        label: "chainargos".into(),
        workdir: "/workspace".into(),
        mounts: vec![],
        keep_awake_enabled: false,
        default_agent: None,
        git_pull_on_entry: false,
        mount_heal: MountHealReport::default(),
    };
    let label = stem.as_workspace_label().unwrap();
    assert_eq!(label.as_str(), "chainargos");
    WorkspaceName::parse(label.as_str()).unwrap();

    let path_label = ResolvedWorkspace {
        name: "/home/op/proj".into(),
        label: "/home/op/proj".into(),
        workdir: "/workspace".into(),
        mounts: vec![],
        keep_awake_enabled: false,
        default_agent: None,
        git_pull_on_entry: false,
        mount_heal: MountHealReport::default(),
    };
    let label = path_label.as_workspace_label().unwrap();
    assert_eq!(label.as_str(), "/home/op/proj");
    let error = WorkspaceName::parse(label.as_str()).unwrap_err();
    assert!(error.to_string().contains("cannot contain path separators"));
}

#[test]
fn resolve_heals_wiped_cache_mounts_end_to_end() {
    // Regression test for a wiped `~/.cache/jackin` bricking every launch:
    // cache directories are recreated, cache files are skipped with a
    // report, and resolution succeeds.
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let cache_sandbox = PathBuf::from(std::env::var("HOME").unwrap())
        .join(".cache")
        .join(format!(
            "jackin-config-heal-test-{}-{unique}",
            std::process::id()
        ));
    // Start clean (stale junk from a previously aborted run heals the
    // same way, but asserting recreation needs a missing dir).
    let _unused = std::fs::remove_dir_all(&cache_sandbox);
    let cargo_git = cache_sandbox.join("global/cargo/git");
    let gradle_properties = cache_sandbox.join("global/gradle/gradle.properties");

    let temp = tempdir().unwrap();
    let mut config = AppConfig::default();
    config.add_mount(
        "cargo-git",
        MountConfig {
            src: cargo_git.display().to_string(),
            dst: "/home/agent/.cargo/git".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
        None,
    );
    config.add_mount(
        "gradle-properties",
        MountConfig {
            src: gradle_properties.display().to_string(),
            dst: "/home/agent/.gradle/gradle.properties".to_owned(),
            readonly: true,
            isolation: MountIsolation::Shared,
        },
        None,
    );

    let resolved = resolve_load_workspace(
        &config,
        &RoleSelector::new(None, "agent-smith"),
        temp.path(),
        LoadWorkspaceInput::CurrentDir,
        &[],
    )
    .unwrap();

    assert!(
        cargo_git.is_dir(),
        "wiped cache dir must be recreated during resolve"
    );
    assert!(
        resolved
            .mounts
            .iter()
            .any(|m| m.dst == "/home/agent/.cargo/git"),
        "recreated cache mount must stay in the effective mounts"
    );
    assert!(
        resolved
            .mounts
            .iter()
            .all(|m| m.dst != "/home/agent/.gradle/gradle.properties"),
        "missing cache file must be skipped, not mounted"
    );
    assert_eq!(resolved.mount_heal.recreated.len(), 1);
    assert_eq!(
        resolved.mount_heal.recreated[0].name.as_deref(),
        Some("cargo-git")
    );
    assert_eq!(resolved.mount_heal.skipped.len(), 1);
    assert_eq!(
        resolved.mount_heal.skipped[0].name.as_deref(),
        Some("gradle-properties")
    );

    std::fs::remove_dir_all(&cache_sandbox).unwrap();
}

#[test]
fn resolve_still_rejects_missing_non_cache_mount_sources() {
    // Healing is confined to cache roots: a missing project checkout
    // must stay a hard error, never an auto-created empty directory.
    let temp = tempdir().unwrap();
    let missing_project = temp.path().join("projects/gone");
    let mut config = AppConfig::default();
    config.workspaces.insert(
        "my-ws".to_owned(),
        WorkspaceConfig {
            workdir: "/workspace/project".to_owned(),
            mounts: vec![MountConfig {
                src: missing_project.display().to_string(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );

    let error = resolve_load_workspace(
        &config,
        &RoleSelector::new(None, "agent-smith"),
        temp.path(),
        LoadWorkspaceInput::Saved("my-ws".to_owned()),
        &[],
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("mount source does not exist"),
        "{error}"
    );
    assert!(
        !missing_project.exists(),
        "non-cache sources must never be auto-created"
    );
}

fn launch_config() -> (AppConfig, WorkspaceName) {
    use crate::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider};
    use jackin_core::EnvValue;
    let mut config = AppConfig::default();
    for (id, display) in [("a-claude", "A"), ("z-claude", "Z")] {
        config.accounts.insert(
            id.to_owned(),
            AccountConfig {
                enabled: true,
                name: display.into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::ApiKey {
                    value: EnvValue::Plain("test-key".into()),
                    base_url: None,
                    model: None,
                },
            },
        );
    }
    for (id, account) in [("claude-a", "a-claude"), ("claude-z", "z-claude")] {
        config.agent_configurations.insert(
            id.to_owned(),
            AgentConfiguration {
                agent: Agent::Claude,
                account: account.into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
    }
    let ws = WorkspaceName::parse("demo").unwrap();
    config.workspaces.insert(
        ws.as_str().to_owned(),
        WorkspaceConfig {
            workdir: "/demo".into(),
            accounts: vec!["a-claude".into(), "z-claude".into()],
            ..Default::default()
        },
    );
    (config, ws)
}

fn effective_ids(
    config: &AppConfig,
    ws: Option<&WorkspaceName>,
    role: &str,
) -> Option<Vec<String>> {
    config
        .effective_default_launch(ws, role)
        .map(<[String]>::to_vec)
}

#[test]
fn effective_default_launch_follows_scope_precedence() {
    let (mut config, ws) = launch_config();
    assert_eq!(effective_ids(&config, Some(&ws), "smith"), None);
    assert_eq!(effective_ids(&config, None, "smith"), None);

    config.default_launch = Some(vec!["claude-a".into()]);
    assert_eq!(
        effective_ids(&config, Some(&ws), "smith"),
        Some(vec!["claude-a".to_owned()])
    );
    assert_eq!(
        effective_ids(&config, None, "smith"),
        Some(vec!["claude-a".to_owned()])
    );

    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .default_launch = Some(vec!["claude-z".into()]);
    assert_eq!(
        effective_ids(&config, Some(&ws), "smith"),
        Some(vec!["claude-z".to_owned()])
    );
    // Ad-hoc launches have no workspace scope: the global list applies.
    assert_eq!(
        effective_ids(&config, None, "smith"),
        Some(vec!["claude-a".to_owned()])
    );

    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .roles
        .entry("smith".into())
        .or_default()
        .default_launch = Some(vec!["claude-a".into()]);
    assert_eq!(
        effective_ids(&config, Some(&ws), "smith"),
        Some(vec!["claude-a".to_owned()])
    );
    // A role default binds its own role only.
    assert_eq!(
        effective_ids(&config, Some(&ws), "other"),
        Some(vec!["claude-z".to_owned()])
    );

    // An explicit empty list is still a configured default (shell-only).
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .default_launch = Some(Vec::new());
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .roles
        .clear();
    assert_eq!(effective_ids(&config, Some(&ws), "smith"), Some(Vec::new()));

    // Unknown workspaces defer to the legacy unknown-workspace error path.
    let ghost = WorkspaceName::parse("ghost").unwrap();
    assert_eq!(effective_ids(&config, Some(&ghost), "smith"), None);
}

#[test]
fn effective_default_launch_agrees_with_resolve_launch() {
    // None ⟺ sole-eligible fallback: two eligible accounts with no
    // defaults fail with ambiguity, never resolve.
    let (mut config, ws) = launch_config();
    assert!(
        config
            .effective_default_launch(Some(&ws), "smith")
            .is_none()
    );
    let error = crate::resolve_launch(&config, Some(&ws), "smith", None, None).unwrap_err();
    assert!(
        error.to_string().contains("multiple accounts are eligible"),
        "{error:?}"
    );

    // Some ⟺ `resolve_launch` consults defaults: the same ambiguous
    // accounts now fail on the invalid explicit default instead of the
    // fallback — the error names the bad configuration, not ambiguity.
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .roles
        .entry("smith".into())
        .or_default()
        .default_launch = Some(vec!["ghost".into()]);
    assert!(
        config
            .effective_default_launch(Some(&ws), "smith")
            .is_some()
    );
    let error = crate::resolve_launch(&config, Some(&ws), "smith", None, None).unwrap_err();
    assert!(
        error.to_string().contains("unknown agent configuration"),
        "{error:?}"
    );
}
