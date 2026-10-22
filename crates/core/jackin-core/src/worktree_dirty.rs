// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests;

use crate::runner::CommandRunner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeState {
    Clean,
    Dirty,
    Unpushed,
}

impl WorktreeState {
    #[must_use]
    pub fn is_dirty_or_unpushed(self) -> bool {
        matches!(self, Self::Dirty | Self::Unpushed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub status: char,
    pub path: String,
}

#[must_use]
pub fn parse_porcelain(text: &str) -> Vec<ChangedFile> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let status = line.chars().find(|c| !c.is_whitespace()).unwrap_or('?');
            let path = line.get(3..).unwrap_or("").trim().to_owned();
            ChangedFile { status, path }
        })
        .collect()
}

fn is_remote_tracking_ref_row(text: &str) -> bool {
    let row = text.strip_suffix('\n').unwrap_or(text);
    let row = row.strip_suffix('\r').unwrap_or(row);
    let Some(name) = row.strip_prefix("refs/remotes/") else {
        return false;
    };

    !name.is_empty() && name.len() <= 1024 && !name.chars().any(char::is_whitespace)
}

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

    let raw = match runner
        .capture(
            "git",
            &[
                "-C",
                worktree_path,
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
            log(&format!(
                "assess: for-each-ref refs/heads/ failed for {worktree_path}: {e}; preserving as unpushed (cannot enumerate branches)"
            ));
            return Ok(WorktreeState::Unpushed);
        }
    };
    if raw.trim().is_empty() {
        log(&format!(
            "assess: for-each-ref refs/heads/ returned no branches for {worktree_path}; preserving as unpushed"
        ));
        return Ok(WorktreeState::Unpushed);
    }

    for line in raw.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let name = parts.next().unwrap_or("");
        let tip = parts.next().unwrap_or("").trim();
        let upstream = parts.next().unwrap_or("").trim();
        let track = parts.next().unwrap_or("").trim();

        if name.is_empty() || tip.is_empty() {
            log(&format!(
                "assess: malformed for-each-ref row for {worktree_path}: {line:?}; preserving as unpushed"
            ));
            return Ok(WorktreeState::Unpushed);
        }

        if tip == base_commit {
            continue;
        }

        if upstream.is_empty() {
            log(&format!(
                "assess: branch {name} in {worktree_path} is ahead of base with no upstream; preserving as unpushed"
            ));
            return Ok(WorktreeState::Unpushed);
        }

        if track == "[gone]" || track == "gone" {
            let contains_arg = format!("--contains={tip}");
            let remote_refs = match runner
                .capture(
                    "git",
                    &[
                        "-C",
                        worktree_path,
                        "for-each-ref",
                        &contains_arg,
                        "--count=1",
                        "--format=%(refname)",
                        "refs/remotes/",
                    ],
                    None,
                )
                .await
            {
                Ok(refs) => refs,
                Err(e) => {
                    log(&format!(
                        "assess: remote reachability query for branch {name} in {worktree_path} failed: {e}; preserving as unpushed (cannot verify the tip is retained remotely)"
                    ));
                    return Ok(WorktreeState::Unpushed);
                }
            };
            if !is_remote_tracking_ref_row(&remote_refs) {
                log(&format!(
                    "assess: branch {name} in {worktree_path} has upstream={upstream} marked gone, but no remote-tracking ref proves its tip is retained; preserving as unpushed"
                ));
                return Ok(WorktreeState::Unpushed);
            }
            log(&format!(
                "assess: branch {name} in {worktree_path} has upstream={upstream} marked gone and a remote-tracking ref contains its tip; treating as pushed and pruned"
            ));
            continue;
        }

        let ahead = match runner
            .capture(
                "git",
                &[
                    "-C",
                    worktree_path,
                    "rev-list",
                    &format!("{upstream}..{name}"),
                ],
                None,
            )
            .await
        {
            Ok(s) => s,
            Err(e) => {
                log(&format!(
                    "assess: rev-list {upstream}..{name} failed for {worktree_path}: {e}; preserving as unpushed (cannot verify all commits pushed)"
                ));
                return Ok(WorktreeState::Unpushed);
            }
        };
        if !ahead.trim().is_empty() {
            log(&format!(
                "assess: branch {name} in {worktree_path} has commits past upstream {upstream}; preserving as unpushed"
            ));
            return Ok(WorktreeState::Unpushed);
        }
    }

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
            Ok(head_sha) if head_sha.trim() == base_commit.trim() => {}
            Ok(head_sha) => {
                log(&format!(
                    "assess: detached HEAD {sha} != base {base} in {worktree_path}; preserving as unpushed",
                    sha = head_sha.trim(),
                    base = base_commit.trim(),
                ));
                return Ok(WorktreeState::Unpushed);
            }
            Err(e) => {
                log(&format!(
                    "assess: rev-parse HEAD failed for {worktree_path}: {e}; preserving as unpushed (cannot verify detached HEAD state)"
                ));
                return Ok(WorktreeState::Unpushed);
            }
        }
    }

    Ok(WorktreeState::Clean)
}
