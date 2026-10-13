// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) struct NoPrompt;

impl FinalizerPrompt for NoPrompt {
    fn ask_exit_dialog(
        &mut self,
        _c: &str,
        _records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice> {
        panic!("prompt should not be called in this test");
    }
}

pub(super) fn cleanup_fallback_cycles() -> Vec<String> {
    ["", "files", "sha1", "", "", "", ""]
        .into_iter()
        .cycle()
        .take(21)
        .map(str::to_owned)
        .collect()
}

pub(super) fn fake_with_outputs(outputs: &[&str]) -> FakeRunner {
    FakeRunner {
        capture_queue: outputs
            .iter()
            .map(ToString::to_string)
            .chain(cleanup_fallback_cycles())
            .collect(),
        ..FakeRunner::default()
    }
}

pub(super) fn register_fixture(container_dir: &Path, wt: &Path, name: &str, branch: &str) {
    let admin = container_dir.join("repo/.git/worktrees").join(name);
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::write(
        container_dir.join("repo/.git/HEAD"),
        "ref: refs/heads/main\n",
    )
    .unwrap();
    std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
    std::fs::write(
        admin.join("gitdir"),
        format!("{}\n", wt.join(".git").display()),
    )
    .unwrap();
    std::fs::write(admin.join("commondir"), "../..\n").unwrap();
    std::fs::write(admin.join("HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
}

pub(super) fn rec(container_dir: &Path) -> IsolationRecord {
    let wt = crate::materialize::worktree_path_for(container_dir, "/workspace/jackin", "jackin-x");
    std::fs::create_dir_all(&wt).unwrap();
    register_fixture(container_dir, &wt, "jackin-x", "jackin/scratch/jackin-x");
    IsolationRecord {
        workspace_name: Some(jackin_core::WorkspaceName::parse("jackin").unwrap()),
        mount_dst: "/workspace/jackin".into(),
        original_src: container_dir.join("repo").to_string_lossy().into(),
        isolation: MountIsolation::Worktree,
        worktree_path: wt.to_string_lossy().into(),
        scratch_branch: "jackin/scratch/jackin-x".into(),
        base_commit: "abc".into(),
        selector_key: "x".into(),
        container_name: "jackin-x".into(),
        cleanup_status: CleanupStatus::Active,
    }
}

pub(super) fn ferow(name: &str, tip: &str, upstream: &str, track: &str) -> String {
    format!("{name}\t{tip}\t{upstream}\t{track}\tEND")
}

pub(super) struct ScriptedPrompt(pub(super) VecDeque<ExitDialogChoice>);

impl FinalizerPrompt for ScriptedPrompt {
    fn ask_exit_dialog(
        &mut self,
        _c: &str,
        _records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice> {
        Ok(self.0.pop_front().expect("scripted prompt exhausted"))
    }
}

pub(super) struct RecordingPrompt {
    answer: ExitDialogChoice,
    pub(super) seen: Vec<PreservedReason>,
}

impl RecordingPrompt {
    pub(super) fn new(answer: ExitDialogChoice) -> Self {
        Self {
            answer,
            seen: Vec::new(),
        }
    }
}

impl FinalizerPrompt for RecordingPrompt {
    fn ask_exit_dialog(
        &mut self,
        _c: &str,
        records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice> {
        for (_, r) in records {
            self.seen.push(*r);
        }
        Ok(self.answer)
    }
}

pub(super) fn fake_failing_capture(outputs: &[&str], fail_pattern: &str) -> FakeRunner {
    FakeRunner {
        capture_queue: outputs
            .iter()
            .map(ToString::to_string)
            .chain(cleanup_fallback_cycles())
            .collect(),
        fail_on: vec![fail_pattern.into()],
        ..FakeRunner::default()
    }
}

pub(super) fn rec_at(
    container_dir: &Path,
    mount_dst: &str,
    scratch_branch: &str,
) -> IsolationRecord {
    let container_name = scratch_branch.strip_prefix("jackin/scratch/").unwrap();
    let wt = crate::materialize::worktree_path_for(container_dir, mount_dst, container_name);
    std::fs::create_dir_all(&wt).unwrap();
    register_fixture(container_dir, &wt, container_name, scratch_branch);
    IsolationRecord {
        workspace_name: Some(jackin_core::WorkspaceName::parse("ws").unwrap()),
        mount_dst: mount_dst.into(),
        original_src: container_dir.join("repo").to_string_lossy().into(),
        isolation: MountIsolation::Worktree,
        worktree_path: wt.to_string_lossy().into(),
        scratch_branch: scratch_branch.into(),
        base_commit: "abc".into(),
        selector_key: "x".into(),
        container_name: container_name.into(),
        cleanup_status: CleanupStatus::Active,
    }
}
