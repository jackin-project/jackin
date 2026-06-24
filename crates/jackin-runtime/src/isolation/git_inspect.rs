//! Synchronous git helpers for the D24 Inspect surface.
//!
//! The git-spawning helpers run git via `std::process::Command` rather than the
//! async `CommandRunner` because every caller is on an OS thread driving a
//! crossterm raw-mode dialog loop, not a Tokio task. They pipe stdout/stderr
//! (never inherit): `wait_with_output` only captures piped streams, and an
//! inherited stream would scribble git's output over the raw-mode alternate
//! screen. (`working_content_sync` reads the file directly and spawns nothing.)

use std::path::Path;
use std::process::Stdio;

/// One line from `git status --porcelain`.
#[derive(Debug, Clone)]
pub struct ChangedFile {
    /// Porcelain status code — `M` modified, `A` added, `D` deleted,
    /// `?` untracked, etc. Multi-char codes use the first non-space character.
    pub status: char,
    /// Path relative to the worktree root, as reported by `--porcelain`.
    pub path: String,
}

/// Run `git -C <worktree> status --porcelain` and parse the output into a list
/// of changed files.
///
/// Returns an empty list on any error (git missing/failed) so callers degrade
/// gracefully to an empty changed-files pane.
pub fn changed_files_sync(worktree_path: &str) -> Vec<ChangedFile> {
    let Ok(output) = std::process::Command::new("git")
        .args(["-C", worktree_path, "status", "--porcelain"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(std::process::Child::wait_with_output)
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_porcelain(&text)
}

fn parse_porcelain(text: &str) -> Vec<ChangedFile> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            // Porcelain v1: "XY filename" — 2-char status code then a space then path.
            let status = line.chars().find(|c| !c.is_whitespace()).unwrap_or('?');
            let path = line.get(3..).unwrap_or("").trim().to_owned();
            ChangedFile { status, path }
        })
        .collect()
}

/// Read the HEAD version of `rel_path` relative to `worktree_path`.
///
/// Returns `None` when the file does not exist in HEAD (added/untracked).
pub fn head_content_sync(worktree_path: &str, rel_path: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["-C", worktree_path, "show", &format!("HEAD:{rel_path}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?
        .wait_with_output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Read the working-tree version of `rel_path` inside `worktree_path`.
///
/// Returns `None` when the file does not exist (deleted).
pub fn working_content_sync(worktree_path: &str, rel_path: &str) -> Option<String> {
    let full = Path::new(worktree_path).join(rel_path);
    std::fs::read_to_string(full).ok()
}

/// Build the D24 inspect data for one worktree: every changed file paired with
/// its HEAD and working-tree content. The single source of truth for the
/// inspect shape, shared by the exit dialog (`finalize`) and the launch dialog
/// (`restore`) so the two surfaces never drift.
pub fn worktree_inspect(worktree_path: &str) -> jackin_launch::WorktreeInspect {
    let files = changed_files_sync(worktree_path)
        .iter()
        .map(|f| jackin_launch::FileDiff {
            status: f.status,
            path: f.path.clone(),
            before: head_content_sync(worktree_path, &f.path),
            after: working_content_sync(worktree_path, &f.path),
        })
        .collect();
    jackin_launch::WorktreeInspect {
        label: worktree_path.to_owned(),
        files,
    }
}

#[cfg(test)]
mod tests;
