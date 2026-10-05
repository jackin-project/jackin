// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared worktree dirty/unpushed assessment.
//!
//! Pure orchestration over the [`CommandRunner`] seam — this module spawns
//! nothing itself, so it stays inside `jackin-core`'s no-subprocess contract.
//! The host (`jackin-runtime` finalize/cleanup) and the in-container Capsule
//! daemon both call this, so they can never disagree on what "dirty" means.
//!
//! The assessment is **fail-closed**: on any ambiguity — including a transient
//! git failure that prevents observing state — it returns [`WorktreeState::Unpushed`]
//! ("I don't know, keep it") rather than [`WorktreeState::Clean`]. Returning
//! `Clean` from a state we could not observe would garbage-collect unpushed
//! commits made inside the container.

#[cfg(test)]
mod tests;

use crate::runner::CommandRunner;

/// Result of assessing one isolated worktree for safe auto-cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeState {
    /// Working tree is clean and every local branch is proven pushed (or parked
    /// at `base_commit`). Safe to auto-clean.
    Clean,
    /// `git status --porcelain` returned non-empty output — real uncommitted or
    /// untracked changes in the working tree.
    Dirty,
    /// The working tree is clean but at least one local branch has commits not
    /// proven pushed (no upstream, real commits past upstream, detached HEAD off
    /// base), or a git capture failed and we fail closed.
    Unpushed,
}

impl WorktreeState {
    /// Whether this state must be preserved rather than auto-cleaned.
    #[must_use]
    pub fn is_dirty_or_unpushed(self) -> bool {
        matches!(self, Self::Dirty | Self::Unpushed)
    }
}

/// One line from `git status --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    /// Porcelain status code — `M` modified, `A` added, `D` deleted, `?`
    /// untracked, etc. Multi-char codes use the first non-space character.
    pub status: char,
    /// Path relative to the worktree root, as reported by `--porcelain`.
    pub path: String,
}

/// Parse `git status --porcelain` v1 output into a changed-files list. Pure.
#[must_use]
pub fn parse_porcelain(text: &str) -> Vec<ChangedFile> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            // Porcelain v1: "XY filename" — 2-char status code, a space, then path.
            let status = line.chars().find(|c| !c.is_whitespace()).unwrap_or('?');
            let path = line.get(3..).unwrap_or("").trim().to_owned();
            ChangedFile { status, path }
        })
        .collect()
}

/// Capture and parse the changed-files list for `worktree_path` via `runner`.
///
/// Returns an empty list on any error so callers degrade gracefully to "no
/// changed files" rather than failing. Use [`assess_worktree`] for the cleanup
/// decision; this is for the summary count and the Inspect file list.
pub async fn changed_files(
    worktree_path: &str,
    runner: &mut impl CommandRunner,
) -> Vec<ChangedFile> {
    match runner
        .capture("git", &["-C", worktree_path, "status", "--porcelain"], None)
        .await
    {
        Ok(text) => parse_porcelain(&text),
        Err(_) => Vec::new(),
    }
}

/// Count commits on any local branch that are not reachable from any remote —
/// i.e. unpushed work — via `git log --branches --not --remotes`.
///
/// Unlike [`assess_worktree`] this needs no `base_commit`, so an in-container
/// caller that does not have the host's isolation record can still surface an
/// unpushed-commit count for the dirty-exit summary. Returns 0 on any error.
pub async fn unpushed_commit_count(worktree_path: &str, runner: &mut impl CommandRunner) -> usize {
    match runner
        .capture(
            "git",
            &[
                "-C",
                worktree_path,
                "log",
                "--branches",
                "--not",
                "--remotes",
                "--format=%H",
            ],
            None,
        )
        .await
    {
        Ok(out) => out.lines().filter(|line| !line.trim().is_empty()).count(),
        Err(_) => 0,
    }
}

/// Assess whether `worktree_path` is safe to auto-clean.
///
/// `log` receives diagnostic lines describing each fail-closed decision; the
/// caller decides whether they belong in operator output or a registered
/// privacy-safe signal. The full per-branch policy lives in the host's finalize docs.
///
/// # Errors
/// Never returns `Err` today (every git failure is mapped to a fail-closed
/// `Unpushed`); the `Result` is kept so a future revision can propagate.
pub async fn assess_worktree(
    worktree_path: &str,
    base_commit: &str,
    runner: &mut impl CommandRunner,
    mut log: impl FnMut(&str),
) -> anyhow::Result<WorktreeState> {
    let porcelain = match runner
        .capture("git", &["-C", worktree_path, "status", "--porcelain"], None)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            log(&format!(
                "assess: status --porcelain failed for {worktree_path}: {e}; preserving as unpushed (cannot observe state)"
            ));
            return Ok(WorktreeState::Unpushed);
        }
    };
    if !porcelain.trim().is_empty() {
        return Ok(WorktreeState::Dirty);
    }

    let branches = assess_branches(worktree_path, base_commit, runner, &mut log).await;
    if branches != WorktreeState::Clean {
        return Ok(branches);
    }

    Ok(assess_detached_head(worktree_path, base_commit, runner, &mut log).await)
}

struct BranchRef<'a> {
    name: &'a str,
    tip: &'a str,
    upstream: &'a str,
    track: &'a str,
}

async fn assess_branches(
    worktree_path: &str,
    base_commit: &str,
    runner: &mut impl CommandRunner,
    log: &mut impl FnMut(&str),
) -> WorktreeState {
    // Enumerate every local branch and classify each. Columns are tab-separated:
    //   refname  objectname  upstream  upstream:track
    // upstream:track is the literal "[gone]" when the upstream ref no longer
    // resolves locally (remote branch deleted after a merge, then pruned).
    let raw = match runner
        .capture(
            "git",
            &[
                "-C",
                worktree_path,
                "for-each-ref",
                "--format=%(refname)%09%(objectname)%09%(upstream)%09%(upstream:track)%09END",
                "refs/heads/",
            ],
            None,
        )
        .await
    {
        Ok(s) => s,
        Err(e) => {
            log(&format!(
                "assess: for-each-ref refs/heads/ failed for {worktree_path}: {e}; preserving as unpushed (cannot enumerate branches)"
            ));
            return WorktreeState::Unpushed;
        }
    };
    if raw.trim().is_empty() {
        // Even a freshly materialized worktree carries the scratch branch; zero
        // local branches is pathological — refuse to delete what we can't account for.
        log(&format!(
            "assess: for-each-ref refs/heads/ returned no branches for {worktree_path}; preserving as unpushed"
        ));
        return WorktreeState::Unpushed;
    }

    for line in raw.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        // A terminal sentinel preserves empty fields through runner whitespace trimming.
        let mut parts = line.split('\t');
        let name = parts.next().unwrap_or("");
        let tip = parts.next().unwrap_or("").trim();
        let upstream = parts.next().unwrap_or("").trim();
        let track = parts.next().unwrap_or("").trim();
        let sentinel = parts.next().unwrap_or("");

        if name.is_empty() || tip.is_empty() || sentinel != "END" || parts.next().is_some() {
            log(&format!(
                "assess: malformed for-each-ref row for {worktree_path}: {line:?}; preserving as unpushed"
            ));
            return WorktreeState::Unpushed;
        }

        if branch_is_unpushed(
            worktree_path,
            BranchRef {
                name,
                tip,
                upstream,
                track,
            },
            base_commit,
            runner,
            log,
        )
        .await
        {
            return WorktreeState::Unpushed;
        }
    }

    WorktreeState::Clean
}

async fn branch_is_unpushed(
    worktree_path: &str,
    branch: BranchRef<'_>,
    base_commit: &str,
    runner: &mut impl CommandRunner,
    log: &mut impl FnMut(&str),
) -> bool {
    let BranchRef {
        name,
        tip,
        upstream,
        track,
    } = branch;
    if tip == base_commit {
        // Branch tip at the recorded base — no work was done on this branch.
        return false;
    }
    if upstream.is_empty() {
        log(&format!(
            "assess: branch {name} in {worktree_path} is ahead of base with no upstream; preserving as unpushed"
        ));
        return true;
    }

    // A pruned upstream proves nothing about merge or push. Require every
    // commit at the captured tip to remain reachable from a remote ref.
    if track == "[gone]" || track == "gone" {
        let unreachable = match runner
            .capture(
                "git",
                &[
                    "-C",
                    worktree_path,
                    "rev-list",
                    "--not",
                    "--remotes",
                    "--not",
                    "--end-of-options",
                    tip,
                ],
                None,
            )
            .await
        {
            Ok(output) => output,
            Err(error) => {
                log(&format!(
                    "assess: remote reachability check for branch {name} in {worktree_path} failed: {error}; preserving as unpushed"
                ));
                return true;
            }
        };
        if !unreachable.trim().is_empty() {
            log(&format!(
                "assess: branch {name} in {worktree_path} has a gone upstream and commits unreachable from remote refs; preserving as unpushed"
            ));
            return true;
        }
        return false;
    }

    let range = format!("{upstream}..{tip}");
    let ahead = match runner
        .capture(
            "git",
            &["-C", worktree_path, "rev-list", "--end-of-options", &range],
            None,
        )
        .await
    {
        Ok(s) => s,
        Err(e) => {
            log(&format!(
                "assess: rev-list {upstream}..{name} failed for {worktree_path}: {e}; preserving as unpushed (cannot verify all commits pushed)"
            ));
            return true;
        }
    };
    if !ahead.trim().is_empty() {
        log(&format!(
            "assess: branch {name} in {worktree_path} has commits past upstream {upstream}; preserving as unpushed"
        ));
        return true;
    }
    false
}

async fn assess_detached_head(
    worktree_path: &str,
    base_commit: &str,
    runner: &mut impl CommandRunner,
    log: &mut impl FnMut(&str),
) -> WorktreeState {
    // Detached-HEAD guard: commits made while HEAD is detached don't appear
    // under refs/heads/ and slip past the branch loop. `symbolic-ref --quiet
    // HEAD` exits 0 on an attached branch and fails on a detached HEAD.
    if runner
        .capture(
            "git",
            &["-C", worktree_path, "symbolic-ref", "--quiet", "HEAD"],
            None,
        )
        .await
        .is_err()
    {
        log(&format!(
            "assess: symbolic-ref HEAD failed for {worktree_path} (detached HEAD or error); checking rev-parse HEAD"
        ));
        match runner
            .capture("git", &["-C", worktree_path, "rev-parse", "HEAD"], None)
            .await
        {
            Ok(head_sha) if head_sha.trim() == base_commit.trim() => {
                // Detached HEAD parked at base — no unreachable commits.
            }
            Ok(head_sha) => {
                log(&format!(
                    "assess: detached HEAD {sha} != base {base} in {worktree_path}; preserving as unpushed",
                    sha = head_sha.trim(),
                    base = base_commit.trim(),
                ));
                return WorktreeState::Unpushed;
            }
            Err(e) => {
                log(&format!(
                    "assess: rev-parse HEAD failed for {worktree_path}: {e}; preserving as unpushed (cannot verify detached HEAD state)"
                ));
                return WorktreeState::Unpushed;
            }
        }
    }

    WorktreeState::Clean
}
