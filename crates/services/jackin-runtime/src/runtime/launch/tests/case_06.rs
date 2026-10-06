// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn resolve_backend_defaults_docker_and_workspace_overrides_config() {
    let mut config = jackin_config::AppConfig::default();
    // No selection anywhere → Docker.
    assert_eq!(resolve_backend(&config, None).unwrap(), Backend::Docker);
    // Host-wide default applies.
    config.runtime.default_backend = Some("apple-container".to_owned());
    assert_eq!(
        resolve_backend(&config, None).unwrap(),
        Backend::AppleContainer
    );
    // Per-workspace backend overrides the host-wide default.
    let mut ws = jackin_config::WorkspaceConfig::default();
    ws.runtime.backend = Some("docker".to_owned());
    config.workspaces.insert("prod".to_owned(), ws);
    assert_eq!(
        resolve_backend(&config, Some("prod")).unwrap(),
        Backend::Docker
    );
    // A workspace without an override falls back to the host-wide default.
    assert_eq!(
        resolve_backend(&config, Some("absent")).unwrap(),
        Backend::AppleContainer
    );
    // An unrecognised backend fails closed instead of silently launching Docker.
    config.runtime.default_backend = Some("aple-container".to_owned());
    resolve_backend(&config, None).unwrap_err();
}

#[tokio::test]
async fn agent_mounts_derive_opencode_data_and_config_roots() {
    let mounts = home_mounts_for("opencode", jackin_core::Agent::Opencode);
    assert!(
        mounts
            .iter()
            .any(|m| m.ends_with(":/home/agent/.local/share/opencode")),
        "opencode data root mount missing: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.ends_with(":/home/agent/.config/opencode")),
        "opencode paired config root mount missing: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_derive_amp_paired_config_root() {
    let mounts = home_mounts_for("amp", jackin_core::Agent::Amp);
    assert!(
        mounts
            .iter()
            .any(|m| m.ends_with(":/home/agent/.config/amp")),
        "amp paired config root mount missing: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_derive_grok_home_root() {
    let mounts = home_mounts_for("grok", jackin_core::Agent::Grok);
    assert!(
        mounts.iter().any(|m| m.ends_with(":/home/agent/.grok")),
        "grok home root mount missing: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_derive_kimi_home_root() {
    let mounts = home_mounts_for("kimi", jackin_core::Agent::Kimi);
    assert!(
        mounts
            .iter()
            .any(|m| m.ends_with(":/home/agent/.kimi-code")),
        "kimi home root mount missing: {mounts:?}"
    );
}

#[tokio::test]
async fn build_workspace_mount_strings_marks_overrides_readonly() {
    // One worktree-mode mount with all four bind sources populated.
    // Host `.git/` mount MUST stay rw (git writes refs/objects/
    // HEAD/index/logs all under it on every commit/branch/fetch).
    // Both override files MUST be `:ro`-suppressed.
    let mat = MaterializedWorkspace {
            workdir: "/workspace/jackin".into(),
            mounts: vec![MaterializedMount {
                bind_src:
                    "/data/jk-the-architect/git/worktree/repo/Users/donbeave/Projects/jackin-project/jackin/jk-the-architect"
                        .into(),
                dst: "/Users/donbeave/Projects/jackin-project/jackin".into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(WorktreeAuxMounts {
                    host_git_dir: "/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    host_git_target:
                        "/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    git_file_override:
                        "/data/jk-the-architect/git/overrides/Users/donbeave/Projects/jackin-project/jackin/.git"
                            .into(),
                    git_file_target: "/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    gitdir_back_override:
                        "/data/jk-the-architect/git/overrides/Users/donbeave/Projects/jackin-project/jackin/gitdir"
                            .into(),
                    gitdir_back_target:
                        "/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git/worktrees/jk-the-architect/gitdir"
                            .into(),
                }),
            }],
            keep_awake_enabled: false,
        };

    let strings = build_workspace_mount_strings(&mat);
    assert_eq!(strings.len(), 4, "one worktree mount → four bind specs");

    // 1: worktree at <dst>, no :ro (writable).
    assert_eq!(
        strings[0],
        "/data/jk-the-architect/git/worktree/repo/Users/donbeave/Projects/jackin-project/jackin/jk-the-architect:/Users/donbeave/Projects/jackin-project/jackin"
    );
    assert!(!strings[0].ends_with(":ro"));

    // 2: host .git/, MUST stay rw — refs/objects/HEAD/index/logs
    // are all written under it. Both ends terminate in `.git`.
    assert_eq!(
        strings[1],
        "/Users/donbeave/Projects/jackin-project/jackin/.git:/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git"
    );
    assert!(
        !strings[1].ends_with(":ro"),
        "host .git mount must remain rw",
    );

    // 3: .git pointer override at <dst>/.git. :ro hardening.
    assert!(
        strings[2].ends_with(":ro"),
        "git-file override must be ro; got {}",
        strings[2],
    );
    assert!(
        strings[2].contains("/git/overrides/Users/donbeave/Projects/jackin-project/jackin/.git")
    );
    assert!(strings[2].contains(":/Users/donbeave/Projects/jackin-project/jackin/.git:ro"));

    // 4: gitdir back-pointer override at
    // `/jackin/host/<dst-tree>/.git/worktrees/<container>/gitdir`.
    // File-level overlay on top of the host `.git/` mount destination.
    // :ro hardening.
    assert!(
        strings[3].ends_with(":ro"),
        "gitdir-back override must be ro; got {}",
        strings[3],
    );
    assert!(
        strings[3].contains("/git/overrides/Users/donbeave/Projects/jackin-project/jackin/gitdir")
    );
    assert!(
            strings[3].contains(
                ":/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git/worktrees/jk-the-architect/gitdir:ro"
            )
        );
}

#[tokio::test]
async fn build_workspace_mount_strings_passthrough_for_shared_mounts() {
    // Shared mounts produce exactly one bind spec, no aux entries.
    let mat = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![MaterializedMount {
            bind_src: "/host/shared".into(),
            dst: "/workspace/shared".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
            worktree_aux: None,
        }],
        keep_awake_enabled: false,
    };

    let strings = build_workspace_mount_strings(&mat);
    assert_eq!(strings, vec!["/host/shared:/workspace/shared".to_owned()]);
}

#[tokio::test]
async fn build_workspace_mount_strings_two_isolated_mounts_emits_eight_distinct_strings() {
    // A workspace with two isolated mounts on different host repos
    // (allowed by validate_isolation_layout) must emit a clean
    // 4-bind grouping per mount with no path collisions. This is
    // the production multi-mount path; finalize.rs's prompt loop
    // also handles this case (see multi_mount_force_delete_on_each_*).
    let mat = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![
            MaterializedMount {
                bind_src: "/data/jackin-x/git/worktree/repo/workspace/a/jackin-x".into(),
                dst: "/workspace/a".into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(WorktreeAuxMounts {
                    host_git_dir: "/host/repo-a/.git".into(),
                    host_git_target: "/jackin/host/workspace/a/.git".into(),
                    git_file_override: "/data/jackin-x/git/overrides/workspace/a/.git".into(),
                    git_file_target: "/workspace/a/.git".into(),
                    gitdir_back_override: "/data/jackin-x/git/overrides/workspace/a/gitdir".into(),
                    gitdir_back_target: "/jackin/host/workspace/a/.git/worktrees/jackin-x/gitdir"
                        .into(),
                }),
            },
            MaterializedMount {
                bind_src: "/data/jackin-x/git/worktree/repo/workspace/b/jackin-x".into(),
                dst: "/workspace/b".into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(WorktreeAuxMounts {
                    host_git_dir: "/host/repo-b/.git".into(),
                    host_git_target: "/jackin/host/workspace/b/.git".into(),
                    git_file_override: "/data/jackin-x/git/overrides/workspace/b/.git".into(),
                    git_file_target: "/workspace/b/.git".into(),
                    gitdir_back_override: "/data/jackin-x/git/overrides/workspace/b/gitdir".into(),
                    gitdir_back_target: "/jackin/host/workspace/b/.git/worktrees/jackin-x/gitdir"
                        .into(),
                }),
            },
        ],
        keep_awake_enabled: false,
    };

    let strings = build_workspace_mount_strings(&mat);
    assert_eq!(
        strings.len(),
        8,
        "two isolated mounts → eight bind specs (4 per mount); got {strings:?}"
    );

    // No two emitted strings may be identical — distinct dsts
    // throughout, which is the disambiguation guarantee under
    // /jackin/host/<dst-tree>/.
    let mut sorted = strings.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        strings.len(),
        "no duplicate bind specs across mounts; got {strings:?}"
    );

    // Each mount's 4 bind specs reference its own dst tree.
    let first_mount_count = strings
        .iter()
        .filter(|s| s.contains("/workspace/a") || s.contains("/jackin/host/workspace/a/"))
        .count();
    let second_mount_count = strings
        .iter()
        .filter(|s| s.contains("/workspace/b") || s.contains("/jackin/host/workspace/b/"))
        .count();
    assert_eq!(first_mount_count, 4, "mount A should have 4 bind specs");
    assert_eq!(second_mount_count, 4, "mount B should have 4 bind specs");

    // Both override files for both mounts must remain :ro.
    let ro_count = strings.iter().filter(|s| s.ends_with(":ro")).count();
    assert_eq!(
        ro_count, 4,
        ":ro hardening must apply to both override files of both mounts; got {strings:?}"
    );
}

#[tokio::test]
async fn build_workspace_mount_strings_preserves_readonly_on_user_facing_mount() {
    // A user-configured `readonly = true` mount still gets `:ro` on
    // the user-facing dst — this is independent of the override
    // hardening.
    let mat = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![MaterializedMount {
            bind_src: "/host/cache".into(),
            dst: "/workspace/cache".into(),
            readonly: true,
            isolation: MountIsolation::Shared,
            worktree_aux: None,
        }],
        keep_awake_enabled: false,
    };

    let strings = build_workspace_mount_strings(&mat);
    assert_eq!(strings, vec!["/host/cache:/workspace/cache:ro".to_owned()]);
}

#[test]
fn build_workspace_mounts_preserves_plain_read_write_mount() {
    let workspace = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![MaterializedMount {
            bind_src: "/host/source".into(),
            dst: "/workspace/source".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
            worktree_aux: None,
        }],
        keep_awake_enabled: false,
    };

    let mounts = build_workspace_mounts(&workspace).unwrap();

    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].source, Path::new("/host/source"));
    assert_eq!(mounts[0].target, Path::new("/workspace/source"));
    assert!(!mounts[0].readonly);
}

#[test]
fn build_workspace_mounts_preserves_operator_readonly_mount() {
    let workspace = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![MaterializedMount {
            bind_src: "/host/cache".into(),
            dst: "/workspace/cache".into(),
            readonly: true,
            isolation: MountIsolation::Shared,
            worktree_aux: None,
        }],
        keep_awake_enabled: false,
    };

    let mounts = build_workspace_mounts(&workspace).unwrap();

    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].target, Path::new("/workspace/cache"));
    assert!(mounts[0].readonly);
}

#[test]
fn build_workspace_mounts_rejects_worktree_file_overlays() {
    let workspace = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![MaterializedMount {
            bind_src: "/state/worktree".into(),
            dst: "/workspace/repo".into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
            worktree_aux: Some(WorktreeAuxMounts {
                host_git_dir: "/host/repo/.git".into(),
                host_git_target: "/jackin/host/workspace/repo/.git".into(),
                git_file_override: "/state/overrides/.git".into(),
                git_file_target: "/workspace/repo/.git".into(),
                gitdir_back_override: "/state/overrides/gitdir".into(),
                gitdir_back_target: "/jackin/host/workspace/repo/.git/worktrees/role/gitdir".into(),
            }),
        }],
        keep_awake_enabled: false,
    };

    let error = build_workspace_mounts(&workspace).unwrap_err();

    assert_eq!(
        error,
        AppleContainerMountError::WorktreeFileOverlays {
            destination: "/workspace/repo".into()
        }
    );
    let message = error.to_string();
    assert!(message.contains("/workspace/repo"));
    assert!(message.contains("single-file bind mounts"));
    assert!(message.contains("docker backend"));
}
