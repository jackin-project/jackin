// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
