use crate::docker::CommandRunner;
use crate::isolation::cleanup::force_cleanup_isolated;
use crate::isolation::state::{CleanupStatus, IsolationRecord, read_records, upsert_record};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachOutcome {
    pub exit_code: Option<i32>,
    pub oom_killed: bool,
}

impl AttachOutcome {
    pub const fn still_running() -> Self {
        Self {
            exit_code: None,
            oom_killed: false,
        }
    }
    pub const fn stopped(code: i32) -> Self {
        Self {
            exit_code: Some(code),
            oom_killed: false,
        }
    }
    pub const fn oom_killed() -> Self {
        Self {
            exit_code: None,
            oom_killed: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeDecision {
    Preserved,
    Cleaned,
    ReturnToAgent,
}

pub trait FinalizerPrompt {
    fn ask_unsafe_cleanup(&mut self, container: &str, worktree_path: &str)
    -> anyhow::Result<usize>;
}

pub struct StdinPrompt;
impl FinalizerPrompt for StdinPrompt {
    fn ask_unsafe_cleanup(
        &mut self,
        container: &str,
        worktree_path: &str,
    ) -> anyhow::Result<usize> {
        let msg = format!(
            "Isolated worktree for {container} still has uncommitted changes:\n  {worktree_path}\n\nWhat do you want to do?"
        );
        crate::tui::prompt::prompt_choice(
            &msg,
            &[
                "Return to agent to address it",
                "Preserve worktree and exit",
                "Force delete worktree and discard changes",
            ],
        )
    }
}

pub fn finalize_foreground_session(
    container_name: &str,
    container_state_dir: &Path,
    outcome: AttachOutcome,
    is_interactive: bool,
    prompt: &mut impl FinalizerPrompt,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<FinalizeDecision> {
    if outcome.exit_code.is_none() || outcome.oom_killed || outcome.exit_code != Some(0) {
        return Ok(FinalizeDecision::Preserved);
    }
    finalize_clean_exit(
        container_name,
        container_state_dir,
        is_interactive,
        prompt,
        runner,
    )
}

fn finalize_clean_exit(
    container_name: &str,
    container_state_dir: &Path,
    is_interactive: bool,
    prompt: &mut impl FinalizerPrompt,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<FinalizeDecision> {
    let records = read_records(container_state_dir)?;
    let mut all_cleaned = true;
    let mut needs_prompt: Option<IsolationRecord> = None;

    for record in records {
        match assess_cleanup(&record, runner)? {
            CleanupAssessment::SafeToDelete => {
                force_cleanup_isolated(&record, container_state_dir, runner)?;
            }
            CleanupAssessment::PreservedDirty => {
                mark_preserved(container_state_dir, &record, CleanupStatus::PreservedDirty)?;
                all_cleaned = false;
                needs_prompt.get_or_insert(record);
            }
            CleanupAssessment::PreservedUnpushed => {
                mark_preserved(
                    container_state_dir,
                    &record,
                    CleanupStatus::PreservedUnpushed,
                )?;
                all_cleaned = false;
                needs_prompt.get_or_insert(record);
            }
        }
    }

    if all_cleaned {
        return Ok(FinalizeDecision::Cleaned);
    }

    let Some(rec) = needs_prompt else {
        return Ok(FinalizeDecision::Preserved);
    };

    if !is_interactive {
        eprintln!(
            "[jackin] preserved isolated worktree for {container_name}:\n         {wt}\n         reason: see cleanup status\n         run `jackin hardline {short}` to return, `jackin cd {short}` to inspect, or `jackin purge {short}` to discard",
            wt = rec.worktree_path,
            short = container_name.trim_start_matches("jackin-"),
        );
        return Ok(FinalizeDecision::Preserved);
    }

    match prompt.ask_unsafe_cleanup(container_name, &rec.worktree_path)? {
        0 => Ok(FinalizeDecision::ReturnToAgent),
        1 => Ok(FinalizeDecision::Preserved),
        2 => {
            force_cleanup_isolated(&rec, container_state_dir, runner)?;
            Ok(FinalizeDecision::Cleaned)
        }
        other => anyhow::bail!("unexpected prompt choice {other}"),
    }
}

#[derive(Debug)]
enum CleanupAssessment {
    SafeToDelete,
    PreservedDirty,
    PreservedUnpushed,
}

#[allow(clippy::unnecessary_wraps)] // capture failures fall back to unpushed
fn assess_cleanup(
    record: &IsolationRecord,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<CleanupAssessment> {
    let porcelain = runner
        .capture(
            "git",
            &["-C", &record.worktree_path, "status", "--porcelain"],
            None,
        )
        .unwrap_or_default();
    if !porcelain.trim().is_empty() {
        return Ok(CleanupAssessment::PreservedDirty);
    }
    let head = runner
        .capture(
            "git",
            &["-C", &record.worktree_path, "rev-parse", "HEAD"],
            None,
        )
        .unwrap_or_default()
        .trim()
        .to_string();
    if head == record.base_commit {
        return Ok(CleanupAssessment::SafeToDelete);
    }
    let upstream = runner
        .capture(
            "git",
            &[
                "-C",
                &record.worktree_path,
                "for-each-ref",
                "--format=%(upstream:short)",
                &format!("refs/heads/{}", record.scratch_branch),
            ],
            None,
        )
        .unwrap_or_default()
        .trim()
        .to_string();
    if upstream.is_empty() {
        return Ok(CleanupAssessment::PreservedUnpushed);
    }
    let branch_minus_upstream = runner
        .capture(
            "git",
            &[
                "-C",
                &record.worktree_path,
                "rev-list",
                &format!("{upstream}..{}", record.scratch_branch),
            ],
            None,
        )
        .unwrap_or_default();
    if branch_minus_upstream.trim().is_empty() {
        Ok(CleanupAssessment::SafeToDelete)
    } else {
        Ok(CleanupAssessment::PreservedUnpushed)
    }
}

fn mark_preserved(
    container_state_dir: &Path,
    record: &IsolationRecord,
    status: CleanupStatus,
) -> anyhow::Result<()> {
    let mut updated = record.clone();
    updated.cleanup_status = status;
    upsert_record(container_state_dir, updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct NoPrompt;
    impl FinalizerPrompt for NoPrompt {
        fn ask_unsafe_cleanup(&mut self, _c: &str, _w: &str) -> anyhow::Result<usize> {
            panic!("prompt should not be called in this test");
        }
    }

    use crate::runtime::test_support::FakeRunner;

    #[test]
    fn still_running_preserves_records() {
        let dir = TempDir::new().unwrap();
        let mut p = NoPrompt;
        let mut r = FakeRunner::default();
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::still_running(),
            false,
            &mut p,
            &mut r,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Preserved);
    }

    #[test]
    fn stopped_non_zero_preserves_records() {
        let dir = TempDir::new().unwrap();
        let mut p = NoPrompt;
        let mut r = FakeRunner::default();
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(137),
            false,
            &mut p,
            &mut r,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Preserved);
    }

    #[test]
    fn oom_killed_preserves_records() {
        let dir = TempDir::new().unwrap();
        let mut p = NoPrompt;
        let mut r = FakeRunner::default();
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::oom_killed(),
            false,
            &mut p,
            &mut r,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Preserved);
    }

    use crate::isolation::MountIsolation;
    use crate::isolation::state::write_records;
    use std::collections::VecDeque;

    fn fake_with_outputs(outputs: &[&str]) -> FakeRunner {
        FakeRunner {
            capture_queue: VecDeque::from(
                outputs.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ),
            ..FakeRunner::default()
        }
    }

    fn rec(container_dir: &Path) -> IsolationRecord {
        let wt = container_dir.join("isolated/workspace/jackin");
        std::fs::create_dir_all(&wt).unwrap();
        IsolationRecord {
            workspace: "jackin".into(),
            mount_dst: "/workspace/jackin".into(),
            original_src: container_dir.join("repo").to_string_lossy().into(),
            isolation: MountIsolation::Worktree,
            worktree_path: wt.to_string_lossy().into(),
            scratch_branch: "jackin/scratch/x".into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }
    }

    #[test]
    fn clean_worktree_with_head_equal_base_deletes_record() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();

        // Capture queue: status --porcelain (clean), rev-parse HEAD (== base)
        let mut runner = fake_with_outputs(&["", "abc\n"]);
        let mut p = NoPrompt;
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(0),
            false,
            &mut p,
            &mut runner,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Cleaned);
        assert!(read_records(dir.path()).unwrap().is_empty());
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("worktree remove --force"))
        );
        assert!(runner.run_recorded.iter().any(|c| c.contains("branch -D")));
    }

    #[test]
    fn clean_worktree_with_pushed_commits_deletes_record() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // Capture queue:
        //   status --porcelain (clean)
        //   rev-parse HEAD (different from base)
        //   for-each-ref upstream:short -> "origin/jackin/scratch/x"
        //   rev-list <upstream>..<branch> -> "" (all reachable)
        let mut runner = fake_with_outputs(&["", "newhead\n", "origin/jackin/scratch/x\n", ""]);
        let mut p = NoPrompt;
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(0),
            false,
            &mut p,
            &mut runner,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Cleaned);
        assert!(read_records(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn clean_worktree_with_unpushed_commits_preserves() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // Capture queue:
        //   status --porcelain (clean)
        //   rev-parse HEAD (different)
        //   for-each-ref upstream:short -> "origin/jackin/scratch/x"
        //   rev-list <upstream>..<branch> -> "deadbeef" (one local commit not on upstream)
        let mut runner =
            fake_with_outputs(&["", "newhead\n", "origin/jackin/scratch/x\n", "deadbeef\n"]);
        let mut p = NoPrompt;
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(0),
            false,
            &mut p,
            &mut runner,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Preserved);
        let recs = read_records(dir.path()).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
    }

    #[test]
    fn clean_worktree_no_upstream_preserves_when_head_diverged() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // Capture queue:
        //   status --porcelain (clean)
        //   rev-parse HEAD (different)
        //   for-each-ref upstream:short -> "" (no upstream)
        let mut runner = fake_with_outputs(&["", "newhead\n", ""]);
        let mut p = NoPrompt;
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(0),
            false,
            &mut p,
            &mut runner,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Preserved);
        let recs = read_records(dir.path()).unwrap();
        assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
    }

    struct ScriptedPrompt(VecDeque<usize>);
    impl FinalizerPrompt for ScriptedPrompt {
        fn ask_unsafe_cleanup(&mut self, _c: &str, _w: &str) -> anyhow::Result<usize> {
            Ok(self.0.pop_front().expect("scripted prompt exhausted"))
        }
    }

    #[test]
    fn dirty_worktree_interactive_preserve_choice_keeps_state() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let mut runner = fake_with_outputs(&[" M file\n"]);
        let mut p = ScriptedPrompt(VecDeque::from([1]));
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(0),
            true,
            &mut p,
            &mut runner,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Preserved);
        let recs = read_records(dir.path()).unwrap();
        assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedDirty);
    }

    #[test]
    fn dirty_worktree_interactive_force_delete_runs_cleanup() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let mut runner = fake_with_outputs(&[" M file\n"]);
        let mut p = ScriptedPrompt(VecDeque::from([2]));
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(0),
            true,
            &mut p,
            &mut runner,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Cleaned);
        assert!(read_records(dir.path()).unwrap().is_empty());
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("worktree remove --force"))
        );
    }

    #[test]
    fn dirty_worktree_interactive_return_to_agent_signals_caller() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let mut runner = fake_with_outputs(&[" M file\n"]);
        let mut p = ScriptedPrompt(VecDeque::from([0]));
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(0),
            true,
            &mut p,
            &mut runner,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::ReturnToAgent);
        let recs = read_records(dir.path()).unwrap();
        assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedDirty);
    }

    #[test]
    fn dirty_worktree_non_interactive_prints_warning_and_preserves() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let mut runner = fake_with_outputs(&[" M file\n"]);
        let mut p = NoPrompt;
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(0),
            false,
            &mut p,
            &mut runner,
        )
        .unwrap();
        assert_eq!(dec, FinalizeDecision::Preserved);
        let recs = read_records(dir.path()).unwrap();
        assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedDirty);
    }
}
