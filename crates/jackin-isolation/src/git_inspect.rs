// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Synchronous git helpers for the D24 Inspect surface.
//!
//! The git-spawning helpers run git via [`jackin_process`] (sync facade) rather
//! than the async `CommandRunner` because every caller is on an OS thread
//! driving a crossterm raw-mode dialog loop, not a Tokio task. Streams are
//! piped (never inherit): an inherited stream would scribble git's output over
//! the raw-mode alternate screen. (`working_content_sync` reads the file
//! directly and spawns nothing.)

use std::path::Path;

// One source of truth for the porcelain shape: reuse jackin-core's `ChangedFile`
// + `parse_porcelain` rather than duplicating the type and parser here. Imported
// privately — callers needing the type take it from `jackin_core` directly.
use jackin_core::{ChangedFile, parse_porcelain};
use jackin_process::ExecRequest;

fn git_output(request: &ExecRequest) -> anyhow::Result<jackin_process::ExecResult> {
    use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue};

    let operation = jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::PROCESS_COMMAND,
        &[jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::process::classify_executable(&request.program).as_str(),
            ),
        }],
    );
    let result = jackin_process::exec_sync(request);
    let completion = match &result {
        Ok(output) => {
            if let Some(code) = output.code {
                let _attribute = operation.set_attr(jackin_telemetry::Attr {
                    key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
                    value: jackin_telemetry::Value::I64(i64::from(code)),
                });
            }
            if output.timed_out {
                (OutcomeValue::Timeout, Some(ErrorType::Timeout))
            } else if output.success {
                (OutcomeValue::Success, None)
            } else {
                (OutcomeValue::Failure, Some(ErrorType::ProcessExitNonzero))
            }
        }
        Err(_) => (OutcomeValue::Failure, Some(ErrorType::ProcessSpawnError)),
    };
    operation.complete(completion.0, completion.1);
    result
}

/// Run `git -C <worktree> status --porcelain` and parse the output into a list
/// of changed files.
///
/// Returns an empty list on any error (git missing/failed) so callers degrade
/// gracefully to an empty changed-files pane.
pub fn changed_files_sync(worktree_path: &str) -> Vec<ChangedFile> {
    let request = ExecRequest::new("git", ["-C", worktree_path, "status", "--porcelain"]);
    let Ok(output) = git_output(&request) else {
        return Vec::new();
    };
    if !output.success {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_porcelain(&text)
}

/// Read the HEAD version of `rel_path` relative to `worktree_path`.
///
/// Returns `None` when the file does not exist in HEAD (added/untracked).
pub fn head_content_sync(worktree_path: &str, rel_path: &str) -> Option<String> {
    let request = ExecRequest::new(
        "git",
        ["-C", worktree_path, "show", &format!("HEAD:{rel_path}")],
    );
    let output = git_output(&request).ok()?;
    if !output.success {
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
pub fn worktree_inspect(worktree_path: &str) -> jackin_core::WorktreeInspect {
    let files = changed_files_sync(worktree_path)
        .iter()
        .map(|f| jackin_core::FileDiff {
            status: f.status,
            path: f.path.clone(),
            before: head_content_sync(worktree_path, &f.path),
            after: working_content_sync(worktree_path, &f.path),
        })
        .collect();
    jackin_core::WorktreeInspect {
        label: worktree_path.to_owned(),
        files,
    }
}

#[cfg(test)]
mod tests;
