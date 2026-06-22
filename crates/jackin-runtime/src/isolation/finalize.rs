//! Post-attach foreground-session finalizer: classifies worktree state and
//! decides whether to auto-clean or preserve an isolated mount.
//!
//! All git invocations are local-only — no network access. Safe to call after
//! a hardline-locked (offline) attach.
//!
//! Invariant: a worktree with uncommitted changes (`Dirty`) or unpushed
//! commits (`Unpushed`) is always preserved; auto-clean only runs on a clean,
//! fully-pushed tree with a confirmed exit.

#![expect(
    clippy::print_stderr,
    reason = "isolation finalization emits operator-visible preservation and cleanup warnings"
)]

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

use crate::isolation::cleanup::force_cleanup_isolated;
use crate::isolation::state::{CleanupStatus, IsolationRecord, read_records, upsert_record};
use crate::runtime::attach::JACKIN_STATUS_CMD;
use crate::runtime::progress::PromptContextLine;
use jackin_core::CommandRunner;
use jackin_diagnostics::debug_log;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachOutcome {
    /// Container is still running after the foreground attach returned.
    StillRunning,
    /// Container exited with the given code.
    Stopped(i32),
    /// Kernel OOM-killed the container.
    OomKilled,
}

impl AttachOutcome {
    pub const fn still_running() -> Self {
        Self::StillRunning
    }
    pub const fn stopped(code: i32) -> Self {
        Self::Stopped(code)
    }
    pub const fn oom_killed() -> Self {
        Self::OomKilled
    }

    pub(crate) fn as_label(self) -> String {
        match self {
            Self::StillRunning => "still_running".to_owned(),
            Self::Stopped(code) => format!("stopped_{code}"),
            Self::OomKilled => "oom_killed".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeDecision {
    Preserved,
    Cleaned,
    ReturnToAgent,
}

impl FinalizeDecision {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Preserved => "preserved",
            Self::Cleaned => "cleaned",
            Self::ReturnToAgent => "return_to_agent",
        }
    }
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

#[derive(Debug)]
pub struct RichCleanupPrompt;
impl FinalizerPrompt for RichCleanupPrompt {
    fn ask_unsafe_cleanup(
        &mut self,
        container: &str,
        worktree_path: &str,
        reason: PreservedReason,
    ) -> anyhow::Result<usize> {
        Ok(rich_cleanup_prompt(container, worktree_path, reason))
    }
}

/// Forced-choice worktree-cleanup picker, rendered through the shared launch
/// dialog vocabulary (`standalone_select_with_context`) so it inherits the
/// same backdrop, centering, hints, and key handling as every other launch
/// dialog. Returns the option index: 0 = return to role, 1 = preserve,
/// 2 = force-delete.
fn rich_cleanup_prompt(container: &str, worktree_path: &str, reason: PreservedReason) -> usize {
    let reason_line = match reason {
        PreservedReason::Dirty => "has uncommitted changes",
        PreservedReason::Unpushed => "has unpushed commits on a local branch",
    };
    let context = vec![
        PromptContextLine::Emphasis(format!("Container {container} {reason_line}.")),
        PromptContextLine::Blank,
        PromptContextLine::Path(worktree_path.to_owned()),
        PromptContextLine::Blank,
        PromptContextLine::Muted("Choose how jackin' should handle this worktree.".to_owned()),
    ];
    let options = vec![
        "Return to role to address it".to_owned(),
        "Preserve worktree and exit".to_owned(),
        "Force delete worktree and discard changes".to_owned(),
    ];
    match crate::runtime::progress::standalone_select_with_context(
        "Isolated Worktree",
        &context,
        options,
    ) {
        Ok(choice) => choice,
        Err(err) => {
            let reason_str = match reason {
                PreservedReason::Dirty => "uncommitted changes",
                PreservedReason::Unpushed => "unpushed commits on a local branch",
            };
            let message = format!(
                "Container {container} {reason_str}.\n\n{worktree_path}\n\nCould not render the cleanup dialog:\n{err:#}\n\nThe worktree will be preserved."
            );
            let _unused = crate::runtime::progress::standalone_error_popup(
                "Isolated Worktree Error",
                &message,
            );
            1
        }
    }
}

pub async fn finalize_foreground_session(
    container_name: &str,
    container_state_dir: &Path,
    outcome: AttachOutcome,
    is_interactive: bool,
    prompt: &mut impl FinalizerPrompt,
    docker: &impl jackin_docker::docker_client::DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<FinalizeDecision> {
    debug_log!(
        "isolation",
        "finalize_foreground_session: container={c} outcome={o:?} interactive={i}",
        c = container_name,
        o = outcome,
        i = is_interactive,
    );
    if !matches!(outcome, AttachOutcome::Stopped(0)) {
        // Non-zero exit, OOM-kill, or still-running → preserve by default.
        // Exception: StillRunning with no active jackin sessions means the
        // Capsule has not exited yet after the foreground client returned.
        // Fall through to finalize_clean_exit so isolation worktrees are
        // swept normally.
        if matches!(outcome, AttachOutcome::StillRunning)
            && !has_jackin_sessions(docker, container_name).await
        {
            debug_log!(
                "isolation",
                "finalize: container={c} still running but no jackin sessions; \
                 capsule still running after clean exit — proceeding to isolation cleanup",
                c = container_name,
            );
            return finalize_clean_exit(
                container_name,
                container_state_dir,
                is_interactive,
                prompt,
                runner,
            )
            .await;
        }
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
    .await
}

async fn has_jackin_sessions(
    docker: &impl jackin_docker::docker_client::DockerApi,
    container_name: &str,
) -> bool {
    // Only an explicit `Sessions: 0` header proves the capsule is
    // idle. Empty/malformed stdout still routes to "unknown/present"
    // — a torn write or a daemon restart mid-call must not auto-clean.
    // Header parser is shared with `runtime::attach::inspect_agent_sessions`
    // so a future drift in the header shape touches one definition,
    // not two parsers that can silently disagree on edge cases.
    match docker
        .exec_capture(container_name, &["sh", "-c", JACKIN_STATUS_CMD])
        .await
    {
        Ok(output) => match crate::runtime::attach::parse_session_count(&output) {
            Some(0) => false,
            Some(_) => true,
            None => {
                eprintln!(
                    "[jackin] warning: could not parse jackin session status in {container_name}; \
                     treating as sessions-present — run `jackin purge {container_name}` to clean \
                     up isolation worktrees if this was a clean exit"
                );
                true
            }
        },
        Err(e) => {
            // Docker unreachable or container stopped between the exit-code check
            // and this exec. Treat conservatively as sessions-present — the
            // finalize path must not auto-clean records for a container that may
            // still have active sessions.
            eprintln!(
                "[jackin] warning: could not check jackin sessions in {container_name} ({e}); \
                 treating as sessions-present — run `jackin purge {container_name}` to clean \
                 up isolation worktrees if this was a clean exit"
            );
            true
        }
    }
}

async fn finalize_clean_exit(
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
        let assessment = assess_cleanup(&record, runner).await?;
        debug_log!(
            "isolation",
            "finalize assess: container={c} mount={d} → {a:?}",
            c = record.container_name,
            d = record.mount_dst,
            a = assessment,
        );
        match assessment {
            CleanupAssessment::SafeToDelete => {
                force_cleanup_isolated(&record, container_state_dir, runner).await?;
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
                short = crate::instance::naming::instance_id_from_container_base(container_name)
                    .unwrap_or(container_name),
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
                if let Err(e) = force_cleanup_isolated(&rec, container_state_dir, runner).await {
                    eprintln!(
                        "[jackin] warning: force-delete of isolated worktree `{wt}` failed: {e}\n         record retained — re-run `jackin purge {short}` to retry after resolving the underlying issue",
                        wt = rec.worktree_path,
                        short = crate::instance::naming::instance_id_from_container_base(
                            container_name
                        )
                        .unwrap_or(container_name),
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
#[expect(clippy::too_many_lines)] // Linear policy table is clearer inline than split across helpers
async fn assess_cleanup(
    record: &IsolationRecord,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<CleanupAssessment> {
    let porcelain = match runner
        .capture(
            "git",
            &["-C", &record.worktree_path, "status", "--porcelain"],
            None,
        )
        .await
    {
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
    let raw = match runner
        .capture(
            "git",
            &[
                "-C",
                &record.worktree_path,
                "for-each-ref",
                "--format=%(refname:short)%09%(objectname)%09%(upstream:short)%09%(upstream:track)",
                "refs/heads/",
            ],
            None,
        )
        .await
    {
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

        let ahead = match runner
            .capture(
                "git",
                &[
                    "-C",
                    &record.worktree_path,
                    "rev-list",
                    &format!("{upstream}..{name}"),
                ],
                None,
            )
            .await
        {
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
        .await
        .is_err()
    {
        // `symbolic-ref` fails on detached HEAD (exit 1) and on any git
        // error — both are unsafe until we can verify HEAD is at base.
        debug_log!(
            "isolation",
            "finalize assess: symbolic-ref HEAD failed for {wt} (detached HEAD or error); checking rev-parse HEAD",
            wt = record.worktree_path,
        );
        match runner
            .capture(
                "git",
                &["-C", &record.worktree_path, "rev-parse", "HEAD"],
                None,
            )
            .await
        {
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
mod tests;
