// All git invocations from this module are local-only:
//   git status --porcelain
//   git for-each-ref --format=... refs/heads/
//   git rev-list <upstream>..<branch>
//   git symbolic-ref --quiet HEAD          (detached-HEAD guard)
//   git rev-parse HEAD                     (detached-HEAD guard; only when HEAD is detached)
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

/// Why the post-attach finalizer is preserving a worktree instead of
/// auto-cleaning it.
///
/// Drives the prompt wording so the operator sees a description that
/// matches what is actually at risk — a clean tree with unpushed
/// commits looks nothing like a dirty tree, and using one catch-all
/// message for both trains operators to ignore the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreservedReason {
    /// `git status --porcelain` returned non-empty output. There are
    /// real working-tree edits that have not been committed.
    Dirty,
    /// The working tree is clean but at least one local branch has
    /// commits that we cannot prove have shipped (no upstream, real
    /// commits past upstream, or a git capture failure on the per-branch
    /// loop), or HEAD is detached with commits past `base_commit`.
    Unpushed,
}

pub trait FinalizerPrompt {
    fn ask_unsafe_cleanup(
        &mut self,
        container: &str,
        worktree_path: &str,
        reason: PreservedReason,
    ) -> anyhow::Result<usize>;
}

pub struct StdinPrompt;
impl FinalizerPrompt for StdinPrompt {
    fn ask_unsafe_cleanup(
        &mut self,
        container: &str,
        worktree_path: &str,
        reason: PreservedReason,
    ) -> anyhow::Result<usize> {
        let msg = match reason {
            PreservedReason::Dirty => format!(
                "Isolated worktree for {container} has uncommitted changes:\n  {worktree_path}\n\nWhat do you want to do?"
            ),
            PreservedReason::Unpushed => format!(
                "Isolated worktree for {container} has unpushed commits on a local branch:\n  {worktree_path}\n\nWhat do you want to do?"
            ),
        };
        crate::tui::prompt::prompt_choice(
            &msg,
            &[
                "Return to role to address it",
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
    let mut preserved_records: Vec<(IsolationRecord, PreservedReason)> = Vec::new();

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
                preserved_records.push((record, PreservedReason::Dirty));
            }
            CleanupAssessment::PreservedUnpushed => {
                mark_preserved(
                    container_state_dir,
                    &record,
                    CleanupStatus::PreservedUnpushed,
                )?;
                preserved_records.push((record, PreservedReason::Unpushed));
            }
        }
    }

    if preserved_records.is_empty() {
        return Ok(FinalizeDecision::Cleaned);
    }

    if !is_interactive {
        // Non-interactive: print one warning per preserved record so the
        // operator sees every worktree path that survived, not just the
        // first one. Include the per-reason phrasing so the warning is
        // actionable without having to inspect cleanup_status by hand.
        for (rec, reason) in &preserved_records {
            let reason_str = match reason {
                PreservedReason::Dirty => "uncommitted changes",
                PreservedReason::Unpushed => "unpushed commits on a local branch",
            };
            eprintln!(
                "[jackin] preserved isolated worktree for {container_name}:\n         {wt}\n         reason: {reason_str}\n         run `jackin hardline {short}` to return, inspect the path above directly, or `jackin purge {short}` to discard",
                wt = rec.worktree_path,
                short = container_name.trim_start_matches("jackin-"),
            );
        }
        return Ok(FinalizeDecision::Preserved);
    }

    // Interactive: prompt for each preserved record. "Return to role"
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
    for (rec, reason) in preserved_records {
        match prompt.ask_unsafe_cleanup(container_name, &rec.worktree_path, reason)? {
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
/// The contract is enforced **per local branch in the worktree**, not
/// just `record.scratch_branch`. Roles (and external tooling such as
/// the Superpowers plugin in Claude Code) routinely create their own
/// `feature/*` branch inside the worktree and abandon the scratch
/// branch at `base_commit`. The original implementation hardcoded
/// `record.scratch_branch` in the upstream/rev-list checks and so
/// always saw "no upstream" even when the role's actual branch had
/// already been pushed and squash-merged on the remote — producing
/// the spurious "still has uncommitted changes" prompt on every
/// clean exit.
///
/// Per-branch policy table (from worktree-cleanup-assessment.mdx):
///
/// | Branch state                                                  | Decision |
/// |---------------------------------------------------------------|----------|
/// | tip == `base_commit`                                          | Safe     |
/// | tip moved, no upstream                                        | Unsafe   |
/// | tip moved, upstream set, upstream tracking ref not `[gone]`, `rev-list` empty   | Safe  |
/// | tip moved, upstream set, upstream tracking ref not `[gone]`, `rev-list` non-empty | Unsafe |
/// | tip moved, upstream `[gone]` (squash-merged + pruned)         | Safe     |
/// | any `git` capture error                                       | Unsafe   |
///
/// After all named branches pass, a detached-HEAD guard runs:
///
/// | Detached-HEAD state                                           | Decision |
/// |---------------------------------------------------------------|----------|
/// | `symbolic-ref HEAD` succeeds (HEAD on branch)                 | Safe     |
/// | `symbolic-ref HEAD` fails, `rev-parse HEAD` == `base_commit`  | Safe     |
/// | `symbolic-ref HEAD` fails, `rev-parse HEAD` != `base_commit`  | Unsafe   |
/// | `symbolic-ref HEAD` fails, `rev-parse HEAD` also fails        | Unsafe   |
///
/// `[gone]` upstream is treated as Safe because squash-merge with
/// remote-branch-deletion is the dominant GitHub workflow: there is
/// no purely-local git operation that proves "my local branch was
/// squash-merged into main" (squash-merge breaks `git branch -r
/// --contains HEAD` reachability by design), and without this rule
/// every squash-merged worktree would be permanently preserved.
/// Operator-error mitigation: the host repo's reflog still holds the
/// commits if an operator deletes a remote branch by accident.
///
/// Each `runner.capture` failure is matched explicitly and routed to
/// `PreservedUnpushed` (the "I don't know, keep it" outcome) with a
/// `debug_log!` of the underlying error so `--debug` shows what went
/// wrong.
#[allow(clippy::unnecessary_wraps)] // Result lets us propagate from inner ? if a future revision adds Err arms
#[allow(clippy::too_many_lines)] // Linear policy table is clearer inline than split across helpers
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

    // Enumerate every local branch in the worktree and classify each.
    // Format columns separated by tab (\t = %09 in git format-spec):
    //   refname:short  objectname  upstream:short  upstream:track
    // upstream:track yields the literal string "[gone]" (with brackets)
    // when the configured upstream ref no longer resolves locally —
    // typically because the remote branch was deleted after a PR merge
    // and the next `git fetch --prune` removed the remote-tracking ref.
    let raw = match runner.capture(
        "git",
        &[
            "-C",
            &record.worktree_path,
            "for-each-ref",
            "--format=%(refname:short)%09%(objectname)%09%(upstream:short)%09%(upstream:track)",
            "refs/heads/",
        ],
        None,
    ) {
        Ok(s) => s,
        Err(e) => {
            debug_log!(
                "isolation",
                "finalize assess: for-each-ref refs/heads/ failed for {wt}: {e}; preserving as unpushed (cannot enumerate branches)",
                wt = record.worktree_path,
            );
            return Ok(CleanupAssessment::PreservedUnpushed);
        }
    };
    if raw.trim().is_empty() {
        // A worktree with zero local branches is pathological — even a
        // freshly materialized worktree carries the scratch branch.
        // Refuse to delete what we can't account for.
        debug_log!(
            "isolation",
            "finalize assess: for-each-ref refs/heads/ returned no branches for {wt}; preserving as unpushed",
            wt = record.worktree_path,
        );
        return Ok(CleanupAssessment::PreservedUnpushed);
    }

    for line in raw.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        // `split('\t')` keeps trailing empty fields (e.g. when both
        // upstream:short and upstream:track are empty), which is what we
        // want — the column count is fixed at four.
        let mut parts = line.split('\t');
        let name = parts.next().unwrap_or("");
        let tip = parts.next().unwrap_or("").trim();
        let upstream = parts.next().unwrap_or("").trim();
        let track = parts.next().unwrap_or("").trim();

        if name.is_empty() || tip.is_empty() {
            // Malformed row — fail closed.
            debug_log!(
                "isolation",
                "finalize assess: malformed for-each-ref row for {wt}: {line:?}; preserving as unpushed",
                wt = record.worktree_path,
            );
            return Ok(CleanupAssessment::PreservedUnpushed);
        }

        if tip == record.base_commit {
            // Branch tip is at the recorded base — by definition no work
            // was done on this branch (covers the abandoned scratch
            // branch in the captured rename case).
            continue;
        }

        if upstream.is_empty() {
            // Tip moved past base, no upstream configured — genuinely
            // local-only work that we must preserve.
            debug_log!(
                "isolation",
                "finalize assess: branch {name} in {wt} is ahead of base with no upstream; preserving as unpushed",
                wt = record.worktree_path,
            );
            return Ok(CleanupAssessment::PreservedUnpushed);
        }

        // `[gone]` (or bare `gone` in some git versions) means the
        // upstream ref is configured but the remote-tracking ref was
        // pruned. Treat as Safe — see the policy comment above.
        if track == "[gone]" || track == "gone" {
            debug_log!(
                "isolation",
                "finalize assess: branch {name} in {wt} has upstream={upstream} marked gone; treating as merged-and-pruned (safe)",
                wt = record.worktree_path,
            );
            continue;
        }

        let ahead = match runner.capture(
            "git",
            &[
                "-C",
                &record.worktree_path,
                "rev-list",
                &format!("{upstream}..{name}"),
            ],
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                debug_log!(
                    "isolation",
                    "finalize assess: rev-list {upstream}..{name} failed for {wt}: {e}; preserving as unpushed (cannot verify all commits pushed)",
                    wt = record.worktree_path,
                );
                return Ok(CleanupAssessment::PreservedUnpushed);
            }
        };
        if !ahead.trim().is_empty() {
            debug_log!(
                "isolation",
                "finalize assess: branch {name} in {wt} has commits past upstream {upstream}; preserving as unpushed",
                wt = record.worktree_path,
            );
            return Ok(CleanupAssessment::PreservedUnpushed);
        }
    }

    // Detached-HEAD guard: commits made while HEAD is detached don't
    // appear under refs/heads/ and slip past the branch loop above.
    // `symbolic-ref --quiet HEAD` exits 0 on an attached branch and
    // fails (exit 1) on a detached HEAD — both failure and a capture
    // error are treated as potentially unsafe.
    if runner
        .capture(
            "git",
            &[
                "-C",
                &record.worktree_path,
                "symbolic-ref",
                "--quiet",
                "HEAD",
            ],
            None,
        )
        .is_err()
    {
        // `symbolic-ref` fails on detached HEAD (exit 1) and on any git
        // error — both are unsafe until we can verify HEAD is at base.
        debug_log!(
            "isolation",
            "finalize assess: symbolic-ref HEAD failed for {wt} (detached HEAD or error); checking rev-parse HEAD",
            wt = record.worktree_path,
        );
        match runner.capture(
            "git",
            &["-C", &record.worktree_path, "rev-parse", "HEAD"],
            None,
        ) {
            Ok(head_sha) if head_sha.trim() == record.base_commit.trim() => {
                // Detached HEAD parked at base — no unreachable commits.
            }
            Ok(head_sha) => {
                debug_log!(
                    "isolation",
                    "finalize assess: detached HEAD {sha} != base {base} in {wt}; preserving as unpushed",
                    sha = head_sha.trim(),
                    base = record.base_commit.trim(),
                    wt = record.worktree_path,
                );
                return Ok(CleanupAssessment::PreservedUnpushed);
            }
            Err(e) => {
                debug_log!(
                    "isolation",
                    "finalize assess: rev-parse HEAD failed for {wt}: {e}; preserving as unpushed (cannot verify detached HEAD state)",
                    wt = record.worktree_path,
                );
                return Ok(CleanupAssessment::PreservedUnpushed);
            }
        }
    }

    Ok(CleanupAssessment::SafeToDelete)
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
        fn ask_unsafe_cleanup(
            &mut self,
            _c: &str,
            _w: &str,
            _r: PreservedReason,
        ) -> anyhow::Result<usize> {
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

    /// Format one for-each-ref row exactly the way the production
    /// query renders it (tab-separated columns).
    fn ferow(name: &str, tip: &str, upstream: &str, track: &str) -> String {
        format!("{name}\t{tip}\t{upstream}\t{track}")
    }

    #[test]
    fn clean_worktree_with_head_equal_base_deletes_record() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();

        // Capture queue:
        //   status --porcelain           (clean)
        //   for-each-ref refs/heads/     (single scratch branch at base_commit)
        //   symbolic-ref HEAD            (HEAD on scratch branch → attached)
        let branches = format!("{}\n", ferow("jackin/scratch/x", "abc", "", ""));
        let mut runner = fake_with_outputs(&["", &branches, "refs/heads/jackin/scratch/x"]);
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
        //   for-each-ref -> single branch ahead of base with reachable upstream
        //   rev-list <upstream>..<branch> -> "" (all reachable)
        //   symbolic-ref HEAD            (HEAD on scratch branch → attached)
        let branches = format!(
            "{}\n",
            ferow("jackin/scratch/x", "newhead", "origin/jackin/scratch/x", "",)
        );
        let mut runner = fake_with_outputs(&["", &branches, "", "refs/heads/jackin/scratch/x"]);
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
        //   for-each-ref -> single branch ahead of base with upstream set
        //   rev-list <upstream>..<branch> -> "deadbeef" (one local commit not on upstream)
        let branches = format!(
            "{}\n",
            ferow(
                "jackin/scratch/x",
                "newhead",
                "origin/jackin/scratch/x",
                "[ahead 1]",
            )
        );
        let mut runner = fake_with_outputs(&["", &branches, "deadbeef\n"]);
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
        //   for-each-ref -> single branch ahead of base with no upstream
        let branches = format!("{}\n", ferow("jackin/scratch/x", "newhead", "", ""));
        let mut runner = fake_with_outputs(&["", &branches]);
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
        fn ask_unsafe_cleanup(
            &mut self,
            _c: &str,
            _w: &str,
            _r: PreservedReason,
        ) -> anyhow::Result<usize> {
            Ok(self.0.pop_front().expect("scripted prompt exhausted"))
        }
    }

    /// Capture-and-assert version of `ScriptedPrompt`: records every
    /// reason it was passed so tests can pin the per-assessment wording
    /// path through the prompt.
    struct RecordingPrompt {
        answers: VecDeque<usize>,
        seen: Vec<PreservedReason>,
    }

    impl RecordingPrompt {
        fn new(answers: impl IntoIterator<Item = usize>) -> Self {
            Self {
                answers: VecDeque::from_iter(answers),
                seen: Vec::new(),
            }
        }
    }

    impl FinalizerPrompt for RecordingPrompt {
        fn ask_unsafe_cleanup(
            &mut self,
            _c: &str,
            _w: &str,
            r: PreservedReason,
        ) -> anyhow::Result<usize> {
            self.seen.push(r);
            Ok(self.answers.pop_front().expect("scripted prompt exhausted"))
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
    fn assess_cleanup_for_each_ref_failure_preserves_unpushed() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // status clean, then for-each-ref refs/heads/ errors.
        let mut runner = fake_failing_capture(&[""], "for-each-ref");
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
        // status clean, for-each-ref returns one branch ahead with
        // upstream still configured (not gone), then rev-list fails.
        // The fail-closed Err arm must route to PreservedUnpushed, not
        // silently treat the failure as "no commits ahead".
        let branches = format!(
            "{}\n",
            ferow(
                "jackin/scratch/x",
                "newhead",
                "origin/jackin/scratch/x",
                "[ahead 1]",
            )
        );
        let mut runner = fake_failing_capture(&["", &branches], "rev-list");
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
        write_records(dir.path(), &[r1, r2]).unwrap();
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
        write_records(dir.path(), &[r1, r2]).unwrap();
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
        write_records(dir.path(), &[r1, r2, r3]).unwrap();
        // All three records assess to PreservedDirty.
        let mut runner = fake_with_outputs(&[" M f1\n", " M f2\n", " M f3\n"]);
        // Operator: force-delete first, then return-to-role on second.
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
        write_records(dir.path(), &[r1, r2]).unwrap();
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
        write_records(dir.path(), &[r1, r2]).unwrap();
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
    fn assess_cleanup_empty_for_each_ref_preserves_unpushed() {
        // Defense in depth: a worktree that reports zero local branches
        // is pathological — even a freshly materialized worktree carries
        // the scratch branch. Refuse to delete what we can't account for.
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // status clean, then for-each-ref returns empty (no branches).
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

    // ---------------------------------------------------------------
    // Per-branch policy table (Piece 1 of the worktree-cleanup
    // assessment fix). Each test pins one row of the table, with the
    // worktree state shape that produces the captured-state bug from
    // worktree-cleanup-assessment.mdx.
    // ---------------------------------------------------------------

    /// Renamed-branch happy path. Scratch branch parked at `base_commit`;
    /// role's renamed `feature/x` branch is ahead of base with a
    /// reachable upstream and rev-list returns empty (all commits
    /// pushed). Pre-fix this returned `PreservedUnpushed` because the
    /// upstream check was hardcoded against the abandoned scratch
    /// branch (which has no upstream by construction).
    #[test]
    fn renamed_branch_pushed_clean_is_safe_to_delete() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // status clean,
        // for-each-ref enumerates two branches:
        //   - scratch at base_commit (no upstream)
        //   - feature/x ahead, upstream set, no [gone]
        // rev-list <upstream>..feature/x → empty (everything pushed)
        // symbolic-ref HEAD             (HEAD on feature/x → attached)
        let branches = [
            ferow("jackin/scratch/x", "abc", "", ""),
            ferow("feature/x", "newhead", "origin/feature/x", ""),
        ]
        .join("\n")
            + "\n";
        let mut runner = fake_with_outputs(&["", &branches, "", "refs/heads/feature/x"]);
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

    /// Squash-merged-and-pruned branch. Scratch parked at base; the
    /// role's `feature/x` branch is ahead with upstream set, but the
    /// upstream-tracking column shows `[gone]` because the remote
    /// branch was deleted after the PR merge and pruned locally. The
    /// `[gone]` heuristic must mark this Safe; pre-fix the rev-list
    /// would have errored on the missing upstream and the Err arm
    /// would have routed to `PreservedUnpushed`.
    #[test]
    fn squash_merged_pruned_branch_is_safe_to_delete() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let branches = [
            ferow("jackin/scratch/x", "abc", "", ""),
            ferow("feature/x", "newhead", "origin/feature/x", "[gone]"),
        ]
        .join("\n")
            + "\n";
        // No rev-list call expected — `[gone]` short-circuits to Safe.
        // symbolic-ref HEAD  (HEAD on feature/x → attached)
        let mut runner = fake_with_outputs(&["", &branches, "refs/heads/feature/x"]);
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
            !runner.recorded.iter().any(|c| c.contains("rev-list")),
            "[gone] short-circuit must not invoke rev-list; recorded={:?}",
            runner.recorded,
        );
    }

    /// Renamed branch ahead of base with no upstream — genuine local
    /// work; preserve. Pre-fix this also returned `PreservedUnpushed`
    /// (correct outcome) but only by accident of the wrong-branch check.
    #[test]
    fn renamed_branch_no_upstream_preserves_unpushed() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let branches = [
            ferow("jackin/scratch/x", "abc", "", ""),
            ferow("feature/x", "newhead", "", ""),
        ]
        .join("\n")
            + "\n";
        let mut runner = fake_with_outputs(&["", &branches]);
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

    /// Renamed branch ahead with upstream set, rev-list returns commits
    /// → real unpushed work, preserve.
    #[test]
    fn renamed_branch_with_unpushed_commits_preserves() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let branches = [
            ferow("jackin/scratch/x", "abc", "", ""),
            ferow("feature/x", "newhead", "origin/feature/x", "[ahead 2]"),
        ]
        .join("\n")
            + "\n";
        let mut runner = fake_with_outputs(&["", &branches, "deadbeef\ncafef00d\n"]);
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

    /// Multiple non-trivial branches, all safe by different paths
    /// (one merged-and-pruned, one pushed-clean). All-Safe → cleanup.
    #[test]
    fn multiple_branches_all_safe_deletes_record() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let branches = [
            ferow("jackin/scratch/x", "abc", "", ""),
            ferow("feature/a", "aaaa", "origin/feature/a", "[gone]"),
            ferow("feature/b", "bbbb", "origin/feature/b", ""),
        ]
        .join("\n")
            + "\n";
        // status clean, branch enumeration, rev-list for feature/b only
        // (feature/a short-circuits via [gone], scratch via tip==base),
        // then symbolic-ref HEAD (HEAD on feature/b → attached).
        let mut runner = fake_with_outputs(&["", &branches, "", "refs/heads/feature/b"]);
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
        let revlist_calls = runner
            .recorded
            .iter()
            .filter(|c| c.contains("rev-list"))
            .count();
        assert_eq!(
            revlist_calls, 1,
            "[gone] and tip==base must short-circuit; only feature/b should hit rev-list. recorded={:?}",
            runner.recorded,
        );
    }

    /// Multiple branches, one with real unpushed work → preserve.
    #[test]
    fn multiple_branches_one_unsafe_preserves() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let branches = [
            ferow("jackin/scratch/x", "abc", "", ""),
            ferow("feature/a", "aaaa", "origin/feature/a", ""),
            ferow("feature/b", "bbbb", "", ""), // ahead, no upstream → unsafe
        ]
        .join("\n")
            + "\n";
        // for-each-ref, then rev-list for feature/a (empty == pushed),
        // then enumeration hits feature/b and short-circuits to
        // PreservedUnpushed without another rev-list.
        let mut runner = fake_with_outputs(&["", &branches, ""]);
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

    /// Prompt-wording variant: a `PreservedUnpushed` assessment must
    /// reach the prompt with reason=Unpushed (not Dirty). Pre-fix,
    /// `ask_unsafe_cleanup` had no reason argument so the wording was
    /// hardcoded to "uncommitted changes" for both paths.
    #[test]
    fn unpushed_branch_prompts_with_unpushed_reason() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
        // status clean → for-each-ref → ahead+no-upstream → preserve
        let mut runner = fake_with_outputs(&["", &branches]);
        let mut p = RecordingPrompt::new([1]); // operator picks "preserve"
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
        assert_eq!(p.seen, vec![PreservedReason::Unpushed]);
    }

    /// Counterpart: a dirty worktree must reach the prompt with
    /// reason=Dirty so the operator sees "uncommitted changes".
    #[test]
    fn dirty_worktree_prompts_with_dirty_reason() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let mut runner = fake_with_outputs(&[" M file\n"]);
        let mut p = RecordingPrompt::new([1]);
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
        assert_eq!(p.seen, vec![PreservedReason::Dirty]);
    }

    // ---------------------------------------------------------------
    // Malformed for-each-ref row — fail-closed guard (line 377).
    // These tests replace the old assess_cleanup_empty_head_*
    // test (which exercised the removed rev-parse HEAD path). The
    // equivalent invariant is now: an empty `name` or empty `tip`
    // in a for-each-ref row must never match base_commit and must
    // always route to PreservedUnpushed.
    // ---------------------------------------------------------------

    #[test]
    fn assess_cleanup_malformed_row_empty_name_preserves_unpushed() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // Empty name column — malformed row, fail closed.
        let branches = format!("{}\n", ferow("", "newhead", "", ""));
        let mut runner = fake_with_outputs(&["", &branches]);
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

    #[test]
    fn assess_cleanup_malformed_row_empty_tip_preserves_unpushed() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // Empty tip column — must not compare equal to any base_commit,
        // including an empty one; always fails closed.
        let branches = format!("{}\n", ferow("feature/x", "", "origin/feature/x", ""));
        let mut runner = fake_with_outputs(&["", &branches]);
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

    // ---------------------------------------------------------------
    // Non-interactive PreservedUnpushed path. The non-interactive
    // eprintln uses per-reason wording; this pins that the Unpushed
    // arm is reached (FinalizeDecision::Preserved +
    // CleanupStatus::PreservedUnpushed) when is_interactive=false.
    // ---------------------------------------------------------------

    #[test]
    fn unpushed_worktree_non_interactive_prints_warning_and_preserves() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // status clean, branch ahead of base with no upstream → PreservedUnpushed
        let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
        let mut runner = fake_with_outputs(&["", &branches]);
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

    // ---------------------------------------------------------------
    // Interactive prompt choices for PreservedUnpushed.
    // Counterparts to the Dirty interactive tests; pins that the
    // three-way prompt dispatch works for both preservation paths.
    // ---------------------------------------------------------------

    #[test]
    fn unpushed_branch_interactive_force_delete_runs_cleanup() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
        let mut runner = fake_with_outputs(&["", &branches]);
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
    fn unpushed_branch_interactive_return_to_agent_signals_caller() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let branches = format!("{}\n", ferow("feature/x", "newhead", "", ""));
        let mut runner = fake_with_outputs(&["", &branches]);
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
        assert_eq!(recs[0].cleanup_status, CleanupStatus::PreservedUnpushed);
    }

    // ---------------------------------------------------------------
    // Bare `gone` track annotation (no brackets). Some git versions
    // emit `gone` instead of `[gone]`; both must short-circuit to Safe.
    // ---------------------------------------------------------------

    #[test]
    fn bare_gone_track_is_safe_to_delete() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // Bare `gone` (no brackets) must also short-circuit to Safe.
        let branches = [
            ferow("jackin/scratch/x", "abc", "", ""),
            ferow("feature/x", "newhead", "origin/feature/x", "gone"),
        ]
        .join("\n")
            + "\n";
        // No rev-list expected — bare `gone` short-circuits just like `[gone]`.
        // symbolic-ref HEAD  (HEAD on feature/x → attached)
        let mut runner = fake_with_outputs(&["", &branches, "refs/heads/feature/x"]);
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
            !runner.recorded.iter().any(|c| c.contains("rev-list")),
            "bare gone short-circuit must not invoke rev-list; recorded={:?}",
            runner.recorded,
        );
    }

    // ---------------------------------------------------------------
    // Detached-HEAD guard. Commits made in detached-HEAD mode don't
    // appear in refs/heads/ and would slip past the branch loop.
    // `symbolic-ref --quiet HEAD` is the sentinel: Err = detached (or
    // git error); both are treated as unsafe unless rev-parse HEAD
    // confirms HEAD is parked at base_commit.
    // ---------------------------------------------------------------

    #[test]
    fn detached_head_past_base_preserves_unpushed() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // All named branches are safe (scratch at base), but HEAD is
        // detached and points at a commit past base_commit.
        let branches = format!("{}\n", ferow("jackin/scratch/x", "abc", "", ""));
        // Queue: status, for-each-ref, rev-parse HEAD (symbolic-ref fails).
        let mut runner = fake_failing_capture(&["", &branches, "deadbeef"], "symbolic-ref");
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

    #[test]
    fn detached_head_at_base_is_safe_to_delete() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // Detached HEAD parked exactly at base_commit ("abc") — no
        // unreachable commits; safe to clean.
        let branches = format!("{}\n", ferow("jackin/scratch/x", "abc", "", ""));
        // Queue: status, for-each-ref, rev-parse HEAD (= "abc\n" → trims to base_commit).
        // Using the real git rev-parse output format (trailing newline) so trim() is exercised.
        let mut runner = fake_failing_capture(&["", &branches, "abc\n"], "symbolic-ref");
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
    fn detached_head_rev_parse_failure_preserves_unpushed() {
        let dir = TempDir::new().unwrap();
        let r = rec(dir.path());
        std::fs::create_dir_all(&r.original_src).unwrap();
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        // Both symbolic-ref and rev-parse fail → fail-closed.
        let branches = format!("{}\n", ferow("jackin/scratch/x", "abc", "", ""));
        let mut runner = FakeRunner {
            capture_queue: std::collections::VecDeque::from(vec![String::new(), branches]),
            fail_on: vec!["symbolic-ref".to_string(), "rev-parse".to_string()],
            ..FakeRunner::default()
        };
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
