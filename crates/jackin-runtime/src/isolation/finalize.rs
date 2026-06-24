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
use jackin_config::DirtyExitPolicy;
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

impl PreservedReason {
    /// Full operator-facing description used in preserve/discard log lines.
    /// One source of truth so the wording can't drift between the policy
    /// branches that emit it.
    fn describe(self) -> &'static str {
        match self {
            Self::Dirty => "uncommitted changes",
            Self::Unpushed => "unpushed commits on a local branch",
        }
    }

    /// Terser tag for the space-constrained exit-dialog per-record list, where
    /// the worktree path already carries most of the context. Centralized here
    /// so it can't drift from [`Self::describe`].
    fn describe_terse(self) -> &'static str {
        match self {
            Self::Dirty => "uncommitted changes",
            Self::Unpushed => "unpushed commits",
        }
    }
}

/// D23: exit dialog returns one of three choices for ALL preserved records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDialogChoice {
    /// Restart the container and let the operator address the dirty worktrees.
    ReturnToRole,
    /// Preserve all dirty/unpushed records and exit cleanly.
    KeepAll,
    /// Force-delete all preserved records and exit.
    DiscardAll,
}

pub trait FinalizerPrompt {
    /// D23: one-screen exit dialog covering all preserved records at once.
    ///
    /// Called when `dirty_exit_policy == Ask` and at least one worktree is
    /// dirty/unpushed. The implementer shows all records in a single surface
    /// and returns the operator's choice for the batch.
    fn ask_exit_dialog(
        &mut self,
        container: &str,
        records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice>;
}

#[derive(Debug)]
pub struct RichCleanupPrompt;
impl FinalizerPrompt for RichCleanupPrompt {
    fn ask_exit_dialog(
        &mut self,
        container: &str,
        records: &[(IsolationRecord, PreservedReason)],
    ) -> anyhow::Result<ExitDialogChoice> {
        Ok(rich_exit_dialog(container, records))
    }
}

/// D23/D24: one-screen exit dialog with `I`-key inspect support.
/// Shows all preserved worktrees and offers three batch choices
/// (Return | Keep all | Discard all). The operator can press `I` to open
/// the D24 inspect surface (file list + diff) before confirming.
fn rich_exit_dialog(
    container: &str,
    records: &[(IsolationRecord, PreservedReason)],
) -> ExitDialogChoice {
    // D24: pre-fetch changed-file lists for each preserved worktree.
    let worktrees_per_record: Vec<Vec<jackin_launch::WorktreeInspect>> = records
        .iter()
        .map(|(rec, _)| {
            vec![crate::isolation::git_inspect::worktree_inspect(
                &rec.worktree_path,
            )]
        })
        .collect();

    let mut context = vec![
        PromptContextLine::Emphasis(format!(
            "Container {container} exited with unsaved isolated work."
        )),
        PromptContextLine::Blank,
    ];
    for (rec, reason) in records {
        let reason_tag = reason.describe_terse();
        context.push(PromptContextLine::Path(rec.worktree_path.clone()));
        context.push(PromptContextLine::Muted(format!("  ({reason_tag})")));
    }
    context.push(PromptContextLine::Blank);
    context.push(PromptContextLine::Muted(
        "Choose how jackin' should handle these worktrees. Press I to inspect changes.".to_owned(),
    ));

    let options = vec![
        "Return to role to address it".to_owned(),
        "Keep all and exit".to_owned(),
        "Discard all and exit".to_owned(),
    ];

    match crate::runtime::progress::standalone_exit_dialog_with_inspect(
        "Isolated Worktrees",
        &context,
        options,
        &worktrees_per_record,
    ) {
        Ok(0) => ExitDialogChoice::ReturnToRole,
        Ok(2) => ExitDialogChoice::DiscardAll,
        Ok(_) => ExitDialogChoice::KeepAll,
        Err(err) => {
            let message = format!(
                "Container {container} has unsaved isolated work.\n\nCould not render the exit dialog:\n{err:#}\n\nAll worktrees will be preserved."
            );
            let _unused =
                crate::runtime::progress::standalone_error_popup("Exit Dialog Error", &message);
            ExitDialogChoice::KeepAll
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "session finalization inherently needs all of: name, path, outcome, interactive flag, policy, prompt, docker, runner"
)]
pub async fn finalize_foreground_session(
    container_name: &str,
    container_state_dir: &Path,
    outcome: AttachOutcome,
    is_interactive: bool,
    dirty_exit_policy: DirtyExitPolicy,
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
                dirty_exit_policy,
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
        dirty_exit_policy,
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
    dirty_exit_policy: DirtyExitPolicy,
    prompt: &mut impl FinalizerPrompt,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<FinalizeDecision> {
    let records = read_records(container_state_dir)?;
    let mut preserved_records: Vec<(IsolationRecord, PreservedReason)> = Vec::new();

    // First pass: assess each record. Auto-clean safe ones; collect every
    // preserved record so the prompt loop below can address them all.
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

    // D8: apply dirty_exit_policy before checking is_interactive.
    // `discard` and `keep` skip all prompts and TUI.
    match dirty_exit_policy {
        DirtyExitPolicy::Discard => {
            // D17: operator opted in to unconditional discard — no confirmation.
            let mut any_failed = false;
            for (rec, reason) in &preserved_records {
                if let Err(e) = force_cleanup_isolated(rec, container_state_dir, runner).await {
                    let reason_str = reason.describe();
                    eprintln!(
                        "[jackin] warning: discard-policy force-delete of `{wt}` failed ({reason_str}): {e}\n         re-run `jackin purge {short}` to retry",
                        wt = rec.worktree_path,
                        short = crate::instance::naming::instance_id_from_container_base(
                            container_name
                        )
                        .unwrap_or(container_name),
                    );
                    any_failed = true;
                }
            }
            return if any_failed {
                Ok(FinalizeDecision::Preserved)
            } else {
                Ok(FinalizeDecision::Cleaned)
            };
        }
        DirtyExitPolicy::Keep => {
            // Auto-preserve all without prompting.
            for (rec, reason) in &preserved_records {
                let reason_str = reason.describe();
                eprintln!(
                    "[jackin] preserved isolated worktree for {container_name} (keep policy):\n         {wt}\n         reason: {reason_str}",
                    wt = rec.worktree_path,
                );
            }
            return Ok(FinalizeDecision::Preserved);
        }
        DirtyExitPolicy::Ask => {
            // Fall through to the interactive/non-interactive prompt logic.
        }
    }

    if !is_interactive {
        // Non-interactive + ask policy: warn and preserve.
        for (rec, reason) in &preserved_records {
            let reason_str = reason.describe();
            eprintln!(
                "[jackin] preserved isolated worktree for {container_name}:\n         {wt}\n         reason: {reason_str}\n         run `jackin hardline {short}` to return, inspect the path above directly, or `jackin purge {short}` to discard",
                wt = rec.worktree_path,
                short = crate::instance::naming::instance_id_from_container_base(container_name)
                    .unwrap_or(container_name),
            );
        }
        return Ok(FinalizeDecision::Preserved);
    }

    // Interactive + ask: D23 one-screen dialog covering all preserved records.
    //
    // A `force_cleanup_isolated` failure must not propagate as `Err` from
    // this function — convert to per-record warning and fall back to Preserved.
    match prompt.ask_exit_dialog(container_name, &preserved_records)? {
        ExitDialogChoice::ReturnToRole => Ok(FinalizeDecision::ReturnToAgent),
        ExitDialogChoice::KeepAll => Ok(FinalizeDecision::Preserved),
        ExitDialogChoice::DiscardAll => {
            let mut any_failed = false;
            for (rec, _reason) in preserved_records {
                if let Err(e) = force_cleanup_isolated(&rec, container_state_dir, runner).await {
                    eprintln!(
                        "[jackin] warning: force-delete of isolated worktree `{wt}` failed: {e}\n         record retained — re-run `jackin purge {short}` to retry",
                        wt = rec.worktree_path,
                        short = crate::instance::naming::instance_id_from_container_base(
                            container_name
                        )
                        .unwrap_or(container_name),
                    );
                    any_failed = true;
                }
            }
            if any_failed {
                Ok(FinalizeDecision::Preserved)
            } else {
                Ok(FinalizeDecision::Cleaned)
            }
        }
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
