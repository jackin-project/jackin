// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn fake_with_outputs(outputs: &[&str]) -> FakeRunner {
    FakeRunner {
        capture_queue: VecDeque::from(outputs.iter().map(ToString::to_string).collect::<Vec<_>>()),
        ..Default::default()
    }
}

pub(super) fn ctx() -> PreflightContext {
    PreflightContext {
        workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
        force: false,
        interactive: false,
    }
}

pub(super) fn worktree_mount(dst: &str, src: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: MountIsolation::Worktree,
    }
}

pub(super) fn dirty_porcelain() -> &'static str {
    " M src/foo.rs\n?? new.rs\n"
}

pub(super) fn ignored_only_porcelain() -> &'static str {
    ""
}

pub(super) fn make_repo_root() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    dir
}

pub(super) fn fake_with_repo_and_status(repo: &Path, status: &str) -> FakeRunner {
    // Capture queue order: rev-parse --show-toplevel, status --porcelain
    fake_with_outputs(&[&repo.to_string_lossy(), status])
}

pub(super) fn resolved_with_one_isolated(repo: &Path, dst: &str) -> ResolvedWorkspace {
    ResolvedWorkspace {
        name: String::new(),
        label: "jackin".into(),
        workdir: dst.into(),
        mounts: vec![MountConfig {
            src: repo.to_string_lossy().into(),
            dst: dst.into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: MountHealReport::default(),
    }
}

pub(super) fn resolved_with_one_clone(repo: &Path, dst: &str) -> ResolvedWorkspace {
    ResolvedWorkspace {
        name: String::new(),
        label: "jackin".into(),
        workdir: dst.into(),
        mounts: vec![MountConfig {
            src: repo.to_string_lossy().into(),
            dst: dst.into(),
            readonly: false,
            isolation: MountIsolation::Clone,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: MountHealReport::default(),
    }
}

pub(super) fn write_loose_branch(repo: &Path, branch: &str, content: &str) {
    let mut p = repo.join(".git").join("refs").join("heads");
    for seg in branch.split('/') {
        p = p.join(seg);
    }
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, content).unwrap();
}

pub(super) fn write_packed_refs(repo: &Path, contents: &str) {
    std::fs::write(repo.join(".git").join("packed-refs"), contents).unwrap();
}
