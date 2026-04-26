// All git invocations from this module are local-only:
//   git status --porcelain
//   git rev-parse HEAD
//   git for-each-ref --format=%(upstream:short) refs/heads/...
//   git rev-list <upstream>..<branch>
//   git worktree remove --force
//   git branch -D
// None require network access. The shared finalizer is safe to call
// after a hardline-locked attach (offline lockdown).

use crate::debug_log;
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
    debug_log!(
        "isolation",
        "finalize_foreground_session: container={c} exit_code={ec:?} oom_killed={oom} interactive={i}",
        c = container_name,
        ec = outcome.exit_code,
        oom = outcome.oom_killed,
        i = is_interactive,
    );
    if outcome.exit_code.is_none() || outcome.oom_killed || outcome.exit_code != Some(0) {
        debug_log!(
            "isolation",
            "finalize: container={c} preserved (non-clean exit)",
            c = container_name,
        );
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
    let mut preserved_records: Vec<IsolationRecord> = Vec::new();

    // First pass: assess each record. Auto-clean safe ones; collect every
    // preserved record so the prompt loop below can address them all (a
    // workspace can have multiple isolated mounts on different host repos
    // and each may need an independent decision).
    for record in records {
        let assessment = assess_cleanup(&record, runner)?;
        debug_log!(
            "isolation",
            "finalize assess: container={c} mount={d} → {a:?}",
            c = record.container_name,
            d = record.mount_dst,
            a = assessment,
        );
        match assessment {
            CleanupAssessment::SafeToDelete => {
                force_cleanup_isolated(&record, container_state_dir, runner)?;
            }
            CleanupAssessment::PreservedDirty => {
                mark_preserved(container_state_dir, &record, CleanupStatus::PreservedDirty)?;
                preserved_records.push(record);
            }
            CleanupAssessment::PreservedUnpushed => {
                mark_preserved(
                    container_state_dir,
                    &record,
                    CleanupStatus::PreservedUnpushed,
                )?;
                preserved_records.push(record);
            }
        }
    }

    if preserved_records.is_empty() {
        return Ok(FinalizeDecision::Cleaned);
    }

    if !is_interactive {
        // Non-interactive: print one warning per preserved record so the
        // operator sees every worktree path that survived, not just the
        // first one.
        for rec in &preserved_records {
            eprintln!(
                "[jackin] preserved isolated worktree for {container_name}:\n         {wt}\n         reason: see cleanup status\n         run `jackin hardline {short}` to return, inspect the path above directly, or `jackin purge {short}` to discard",
                wt = rec.worktree_path,
                short = container_name.trim_start_matches("jackin-"),
            );
        }
        return Ok(FinalizeDecision::Preserved);
    }

    // Interactive: prompt for each preserved record. "Return to agent"
    // applies to the whole container (we restart it) so it short-circuits
    // immediately. "Preserve" and "Force delete" are per-record decisions.
    // The container teardown only happens (`Cleaned`) when *every*
    // preserved record was force-deleted.
    //
    // A `force_cleanup_isolated` failure (`bail!` from cleanup.rs's
    // verify-and-bail flow) must not propagate as an `Err` from this
    // function — that would leave the operator with a raw error from
    // deep in the cleanup path, no `Preserved` signal to the caller,
    // the container left running without an explicit teardown decision,
    // and any subsequent records in the loop never prompted. Convert
    // such failures to a per-record warning + `any_preserved_after_prompt`
    // and continue. The caller treats the resulting `Preserved` as
    // "container survives, run `jackin purge` later to retry."
    let mut any_preserved_after_prompt = false;
    for rec in preserved_records {
        match prompt.ask_unsafe_cleanup(container_name, &rec.worktree_path)? {
            0 => return Ok(FinalizeDecision::ReturnToAgent),
            1 => {
                // Already marked PreservedDirty / PreservedUnpushed above;
                // nothing more to write.
                any_preserved_after_prompt = true;
            }
            2 => {
                if let Err(e) = force_cleanup_isolated(&rec, container_state_dir, runner) {
                    eprintln!(
                        "[jackin] warning: force-delete of isolated worktree `{wt}` failed: {e}\n         record retained — re-run `jackin purge {short}` to retry after resolving the underlying issue",
                        wt = rec.worktree_path,
                        short = container_name.trim_start_matches("jackin-"),
                    );
                    any_preserved_after_prompt = true;
                }
            }
            other => anyhow::bail!("unexpected prompt choice {other}"),
        }
    }

    if any_preserved_after_prompt {
        Ok(FinalizeDecision::Preserved)
    } else {
        Ok(FinalizeDecision::Cleaned)
    }
}

#[derive(Debug)]
enum CleanupAssessment {
    SafeToDelete,
    PreservedDirty,
    PreservedUnpushed,
}

/// Assess whether `record`'s worktree is safe to auto-clean on a clean
/// container exit. The contract: on **any** ambiguity — including a
/// transient git failure that prevents us from answering the question —
/// fall through to a `Preserved*` assessment so the operator can recover
/// the worktree manually. We must never return `SafeToDelete` from a
/// state we couldn't observe; doing so would garbage-collect unpushed
/// commits the operator made inside the container.
///
/// Each `runner.capture` failure is matched explicitly and routed to
/// `PreservedUnpushed` (the "I don't know, keep it" outcome) with a
/// `debug_log!` of the underlying error so `--debug` shows what went
/// wrong. The empty-string outputs that git itself can produce
/// (no upstream, no commits ahead) keep their natural meaning.
#[allow(clippy::unnecessary_wraps)] // Result lets us propagate from inner ? if a future revision adds Err arms
fn assess_cleanup(
    record: &IsolationRecord,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<CleanupAssessment> {
    let porcelain = match runner.capture(
        "git",
        &["-C", &record.worktree_path, "status", "--porcelain"],
        None,
    ) {
        Ok(s) => s,
        Err(e) => {
            debug_log!(
                "isolation",
                "finalize assess: status --porcelain failed for {wt}: {e}; preserving as unpushed (cannot observe state)",
                wt = record.worktree_path,
            );
            return Ok(CleanupAssessment::PreservedUnpushed);
        }
    };
    if !porcelain.trim().is_empty() {
        return Ok(CleanupAssessment::PreservedDirty);
    }
    let head = match runner.capture(
        "git",
        &["-C", &record.worktree_path, "rev-parse", "HEAD"],
        None,
    ) {
        Ok(s) => s.trim().to_string(),
        Err(e) => {
            debug_log!(
                "isolation",
                "finalize assess: rev-parse HEAD failed for {wt}: {e}; preserving as unpushed (cannot compare to base)",
                wt = record.worktree_path,
            );
            return Ok(CleanupAssessment::PreservedUnpushed);
        }
    };
    // Defense in depth: even on Ok, refuse to compare empty HEAD to
    // empty base_commit (both unlikely, but the comparison would yield
    // SafeToDelete which is exactly the wrong answer).
    if head.is_empty() {
        debug_log!(
            "isolation",
            "finalize assess: rev-parse HEAD returned empty for {wt}; preserving as unpushed",
            wt = record.worktree_path,
        );
        return Ok(CleanupAssessment::PreservedUnpushed);
    }
    if head == record.base_commit {
        return Ok(CleanupAssessment::SafeToDelete);
    }
    let upstream = match runner.capture(
        "git",
        &[
            "-C",
            &record.worktree_path,
            "for-each-ref",
            "--format=%(upstream:short)",
            &format!("refs/heads/{}", record.scratch_branch),
        ],
        None,
    ) {
        Ok(s) => s.trim().to_string(),
        Err(e) => {
            debug_log!(
                "isolation",
                "finalize assess: for-each-ref failed for {wt}: {e}; preserving as unpushed (cannot resolve upstream)",
                wt = record.worktree_path,
            );
            return Ok(CleanupAssessment::PreservedUnpushed);
        }
    };
    if upstream.is_empty() {
        return Ok(CleanupAssessment::PreservedUnpushed);
    }
    let branch_minus_upstream = match runner.capture(
        "git",
        &[
            "-C",
            &record.worktree_path,
            "rev-list",
            &format!("{upstream}..{}", record.scratch_branch),
        ],
        None,
    ) {
        Ok(s) => s,
        Err(e) => {
            debug_log!(
                "isolation",
                "finalize assess: rev-list failed for {wt}: {e}; preserving as unpushed (cannot verify all commits pushed)",
                wt = record.worktree_path,
            );
            return Ok(CleanupAssessment::PreservedUnpushed);
        }
    };
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

    /// Build a runner whose git captures will fail at a specific stage.
    /// `fail_pattern` is matched as substring against the recorded command.
    /// Anything before the failing capture comes from `outputs`.
    fn fake_failing_capture(outputs: &[&str], fail_pattern: &str) -> FakeRunner {
        FakeRunner {
            capture_queue: VecDeque::from(
                outputs.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ),
            fail_on: vec![fail_pattern.into()],
            ..FakeRunner::default()
        }
    }

    // The block of tests below pins the safety contract from the docstring
    // on `assess_cleanup`: any git capture failure must route to
    // `PreservedUnpushed` (not `SafeToDelete`) so a transient git error
    // never garbage-collects unpushed scratch-branch commits.

    #[test]
    fn assess_cleanup_status_capture_failure_preserves_unpushed() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // status --porcelain errors → must NOT be treated as clean tree.
        let mut runner = fake_failing_capture(&[], "status --porcelain");
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
        // Critically: no git worktree remove / branch -D should have run.
        assert!(
            !runner
                .run_recorded
                .iter()
                .any(|c| c.contains("worktree remove --force")),
            "must not delete worktree when status capture failed; recorded={:?}",
            runner.run_recorded,
        );
    }

    #[test]
    fn assess_cleanup_rev_parse_failure_preserves_unpushed() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // status returns clean (empty), then rev-parse HEAD errors.
        let mut runner = fake_failing_capture(&[""], "rev-parse HEAD");
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
        assert!(
            !runner
                .run_recorded
                .iter()
                .any(|c| c.contains("worktree remove --force"))
        );
    }

    #[test]
    fn assess_cleanup_for_each_ref_failure_preserves_unpushed() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // status clean, HEAD differs from base, then for-each-ref errors.
        let mut runner = fake_failing_capture(&["", "newhead\n"], "for-each-ref");
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
        assert!(
            !runner
                .run_recorded
                .iter()
                .any(|c| c.contains("worktree remove --force"))
        );
    }

    #[test]
    fn assess_cleanup_rev_list_failure_preserves_unpushed() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // Make it all the way to rev-list, then fail. Without this fix the
        // empty-string fallback would have returned SafeToDelete here and
        // the unpushed commits would have been garbage-collected.
        let mut runner =
            fake_failing_capture(&["", "newhead\n", "origin/jackin/scratch/x\n"], "rev-list");
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
        assert!(
            !runner
                .run_recorded
                .iter()
                .any(|c| c.contains("worktree remove --force")),
            "rev-list failure must not auto-delete; recorded={:?}",
            runner.run_recorded,
        );
    }

    fn rec_at(container_dir: &Path, mount_dst: &str, scratch_branch: &str) -> IsolationRecord {
        let rel = mount_dst.trim_matches('/');
        let wt = container_dir.join(format!("isolated/{rel}"));
        std::fs::create_dir_all(&wt).unwrap();
        IsolationRecord {
            workspace: "ws".into(),
            mount_dst: mount_dst.into(),
            original_src: container_dir.join("repo").to_string_lossy().into(),
            isolation: MountIsolation::Worktree,
            worktree_path: wt.to_string_lossy().into(),
            scratch_branch: scratch_branch.into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }
    }

    /// Multi-mount workspace with two preserved records, operator picks
    /// "force delete" on both. Both worktrees must be cleaned and the
    /// caller signaled `Cleaned` so the container teardown proceeds.
    #[test]
    fn multi_mount_force_delete_on_each_cleans_all_records() {
        let dir = TempDir::new().unwrap();
        let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
        let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
        std::fs::create_dir_all(&r1.original_src).unwrap();
        write_records(dir.path(), &[r1.clone(), r2.clone()]).unwrap();
        // Both records assess to PreservedDirty (status returns dirty for each).
        let mut runner = fake_with_outputs(&[" M file\n", " M file\n"]);
        // Operator chooses option 2 (force delete) for both.
        let mut p = ScriptedPrompt(VecDeque::from([2, 2]));
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
        assert!(
            read_records(dir.path()).unwrap().is_empty(),
            "both records should be removed after force-delete on both",
        );
        let removes = runner
            .run_recorded
            .iter()
            .filter(|c| c.contains("worktree remove --force"))
            .count();
        assert_eq!(
            removes, 2,
            "must run worktree-remove for BOTH preserved mounts; recorded={:?}",
            runner.run_recorded
        );
    }

    /// Multi-mount workspace where the operator force-deletes one and
    /// preserves the other. The container must NOT be torn down (only one
    /// of two records was actually cleaned), and the second worktree must
    /// remain on disk with its preserved status.
    #[test]
    fn multi_mount_mixed_decision_signals_preserved() {
        let dir = TempDir::new().unwrap();
        let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
        let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
        std::fs::create_dir_all(&r1.original_src).unwrap();
        write_records(dir.path(), &[r1.clone(), r2.clone()]).unwrap();
        let mut runner = fake_with_outputs(&[" M file\n", " M file\n"]);
        // First record force-deleted, second preserved.
        let mut p = ScriptedPrompt(VecDeque::from([2, 1]));
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(0),
            true,
            &mut p,
            &mut runner,
        )
        .unwrap();
        assert_eq!(
            dec,
            FinalizeDecision::Preserved,
            "must NOT signal Cleaned when any record was preserved — the \
             container would be torn down and the preserved worktree's only \
             reconnection path (jackin hardline) would be lost",
        );
        let recs = read_records(dir.path()).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].mount_dst, "/workspace/b");
        assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedDirty);
    }

    /// `ReturnToAgent` chosen on the SECOND prompt of a 3-record loop
    /// must short-circuit immediately. Records 3..N are never prompted,
    /// no further force-delete runs, and the caller restarts the
    /// container. Both records 1 (force-deleted) and 2 (pending decision)
    /// would have left state somewhere — this pins that the early-return
    /// happens cleanly.
    #[test]
    fn multi_mount_return_to_agent_on_second_prompt_short_circuits() {
        let dir = TempDir::new().unwrap();
        let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
        let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
        let r3 = rec_at(dir.path(), "/workspace/c", "jackin/scratch/x-c");
        std::fs::create_dir_all(&r1.original_src).unwrap();
        write_records(dir.path(), &[r1.clone(), r2.clone(), r3.clone()]).unwrap();
        // All three records assess to PreservedDirty.
        let mut runner = fake_with_outputs(&[" M f1\n", " M f2\n", " M f3\n"]);
        // Operator: force-delete first, then return-to-agent on second.
        // Third should never be prompted.
        let mut p = ScriptedPrompt(VecDeque::from([2, 0]));
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
        // First was force-deleted; second and third remain on disk.
        let recs = read_records(dir.path()).unwrap();
        assert_eq!(recs.len(), 2);
        let mut dsts: Vec<_> = recs.iter().map(|r| r.mount_dst.clone()).collect();
        dsts.sort();
        assert_eq!(dsts, vec!["/workspace/b", "/workspace/c"]);
        // Exactly one worktree-remove ran (for record 1 only).
        let removes = runner
            .run_recorded
            .iter()
            .filter(|c| c.contains("worktree remove --force"))
            .count();
        assert_eq!(
            removes, 1,
            "ReturnToAgent on the 2nd prompt must NOT trigger cleanup of records 3..N; recorded={:?}",
            runner.run_recorded
        );
    }

    /// `force_cleanup_isolated` failing partway through the prompt loop
    /// must NOT propagate as Err. The caller would see a raw cleanup
    /// error, the container would be left running without a Preserved
    /// signal, and any subsequent records would never be prompted.
    /// Instead the failure is logged and the loop continues.
    #[test]
    fn multi_mount_cleanup_failure_in_loop_does_not_abort() {
        let dir = TempDir::new().unwrap();
        let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
        let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
        std::fs::create_dir_all(&r1.original_src).unwrap();
        write_records(dir.path(), &[r1.clone(), r2.clone()]).unwrap();
        // Both records assess to PreservedDirty (status returns dirty
        // for each), then force_cleanup_isolated runs git commands.
        // We simulate the first mount's `git branch -D` failing AND
        // the verify capture (`git branch --list`) confirming the
        // branch is still present → force_cleanup_isolated bails.
        // Pre-fix: that bail would propagate via `?` and the second
        // record would never be prompted.
        let mut runner = FakeRunner {
            // Capture queue: status for r1, status for r2, then verify
            // capture for r1 (says branch IS present — triggers bail),
            // then verify capture for r2 (says branch absent — proceed).
            capture_queue: VecDeque::from([
                " M f1\n".to_string(),
                " M f2\n".to_string(),
                "  jackin/scratch/x-a\n".to_string(),
                String::new(),
            ]),
            // git branch -D for r1's branch fails; r2's branch -D succeeds.
            fail_on: vec!["branch -D jackin/scratch/x-a".into()],
            ..FakeRunner::default()
        };
        // Operator force-deletes both.
        let mut p = ScriptedPrompt(VecDeque::from([2, 2]));
        let dec = finalize_foreground_session(
            "jackin-x",
            dir.path(),
            AttachOutcome::stopped(0),
            true,
            &mut p,
            &mut runner,
        )
        .expect("loop must NOT propagate the cleanup Err — caller would see a raw error");
        assert_eq!(
            dec,
            FinalizeDecision::Preserved,
            "first record's failed cleanup must surface as Preserved (record retained); \
             second record was successfully force-deleted",
        );
        let recs = read_records(dir.path()).unwrap();
        assert_eq!(
            recs.len(),
            1,
            "first record retained (cleanup bailed), second removed (force-deleted ok)"
        );
        assert_eq!(recs[0].mount_dst, "/workspace/a");
    }

    /// Non-interactive multi-mount: every preserved record's path is
    /// printed to stderr so the operator sees all of them, not just the
    /// first.
    #[test]
    fn multi_mount_non_interactive_marks_all_preserved() {
        let dir = TempDir::new().unwrap();
        let r1 = rec_at(dir.path(), "/workspace/a", "jackin/scratch/x-a");
        let r2 = rec_at(dir.path(), "/workspace/b", "jackin/scratch/x-b");
        std::fs::create_dir_all(&r1.original_src).unwrap();
        write_records(dir.path(), &[r1.clone(), r2.clone()]).unwrap();
        let mut runner = fake_with_outputs(&[" M file\n", " M file\n"]);
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
        assert_eq!(recs.len(), 2);
        assert!(
            recs.iter()
                .all(|r| r.cleanup_status == CleanupStatus::PreservedDirty),
            "every record must be marked preserved, not just the first",
        );
    }

    #[test]
    fn assess_cleanup_empty_head_does_not_compare_equal_to_empty_base() {
        // Defense in depth: even if both `head` and `base_commit` are
        // somehow empty strings (corrupted record + degraded git),
        // the assessment must NOT return SafeToDelete via "" == "".
        let dir = TempDir::new().unwrap();
        let mut r = rec(dir.path());
        r.base_commit = String::new();
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // status clean, then rev-parse returns empty (not an error, but
        // empty stdout — pathological but possible with broken git wrappers).
        let mut runner = fake_with_outputs(&["", ""]);
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
}
