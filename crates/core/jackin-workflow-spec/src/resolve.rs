// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Input resolution. Task, reference, and test-bundle bytes are read
//! relative to the workflow file's directory and hashed before the
//! plan hash is computed. The repository `HEAD` selector is resolved to
//! an exact commit once via `git`; the selector itself is never stored.
//! Agent selections are explicit facts: live selection wiring arrives
//! in a later step, and a missing selection is a typed error, never an
//! interactive prompt.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::digest::sha256_hex;
use crate::error::{Diagnostic, ErrorCode, WorkflowError, WorkflowResult};
use crate::model::RepositoryInput;
use crate::rules::{CheckedDefinition, Selections};

/// Maximum total test-bundle bytes frozen into a plan.
const MAX_BUNDLE_BYTES: u64 = 16 * 1024 * 1024;

/// One frozen input with its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenInput {
    /// Path relative to the workflow file's directory.
    pub logical_path: String,
    /// Frozen bytes.
    pub bytes: Vec<u8>,
    /// Hex SHA-256 over the bytes.
    pub digest: String,
}

/// Everything [`crate::rules::compile_plan`] needs beyond the checked
/// definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInputs {
    /// Frozen task file.
    pub task: FrozenInput,
    /// Frozen reference files.
    pub references: Vec<FrozenInput>,
    /// Frozen test bundle name.
    pub test_bundle_name: String,
    /// Frozen test bundle files.
    pub test_bundle: Vec<FrozenInput>,
    /// Hex digest over the bundle bytes.
    pub test_bundle_digest: String,
    /// Digest of the test image the bundle runs in.
    pub check_image_digest: String,
    /// Frozen repository state.
    pub repository: RepositoryInput,
    /// Frozen module digests from evaluation, keyed by virtual name.
    pub module_digests: BTreeMap<String, String>,
    /// Resolved agent selections.
    pub selections: Selections,
}

/// Read and freeze one file below `dir`.
fn freeze_file(dir: &Path, file_label: &str, logical: &str) -> WorkflowResult<FrozenInput> {
    let path = dir.join(logical);
    let bytes = std::fs::read(&path).map_err(|_| {
        WorkflowError::single(Diagnostic::new(
            file_label,
            logical,
            ErrorCode::MissingInput,
            format!("cannot read input file `{logical}`"),
            format!("create {} relative to the workflow file", path.display()),
        ))
    })?;
    Ok(FrozenInput {
        logical_path: logical.to_owned(),
        digest: sha256_hex(&bytes),
        bytes,
    })
}

/// Inputs for [`resolve_workflow_files`].
#[derive(Debug, Clone)]
pub struct FileResolution {
    /// Workflow file the relative inputs resolve from.
    pub workflow_file: PathBuf,
    /// Label used in diagnostics.
    pub file_label: String,
    /// Directory holding the frozen test bundle files.
    pub check_bundle_dir: PathBuf,
    /// Digest of the test image the bundle runs in.
    pub check_image_digest: String,
    /// Frozen module digests from evaluation, keyed by virtual name.
    pub module_digests: BTreeMap<String, String>,
    /// Frozen repository state.
    pub repository: RepositoryInput,
    /// Resolved agent selections.
    pub selections: Selections,
}

/// Resolve task, reference, and test-bundle inputs for a workflow file.
pub fn resolve_workflow_files(
    resolution: &FileResolution,
    checked: &CheckedDefinition,
) -> WorkflowResult<ResolvedInputs> {
    let file_label = resolution.file_label.as_str();
    let dir = resolution
        .workflow_file
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let task = freeze_file(dir, file_label, &checked.task)?;
    let mut references = Vec::new();
    for reference in &checked.references {
        references.push(freeze_file(dir, file_label, reference)?);
    }
    let mut bundle_files = Vec::new();
    let mut bundle_bytes: u64 = 0;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&resolution.check_bundle_dir)
        .map_err(|_| {
            WorkflowError::single(Diagnostic::new(
                file_label,
                "checks.bundle",
                ErrorCode::MissingInput,
                format!("cannot read test bundle `{}`", checked.checks_bundle),
                format!(
                    "provide the frozen bundle directory at {}",
                    resolution.check_bundle_dir.display()
                ),
            ))
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bytes = std::fs::read(&path).map_err(|_| {
            WorkflowError::single(Diagnostic::new(
                file_label,
                "checks.bundle",
                ErrorCode::MissingInput,
                format!("cannot read test bundle file `{name}`"),
                "repair the bundle directory permissions and retry",
            ))
        })?;
        bundle_bytes += bytes.len() as u64;
        if bundle_bytes > MAX_BUNDLE_BYTES {
            return Err(WorkflowError::single(Diagnostic::new(
                file_label,
                "checks.bundle",
                ErrorCode::InvalidDefinition,
                "test bundle exceeds 16MiB",
                "shrink the frozen test bundle",
            )));
        }
        bundle_files.push(FrozenInput {
            logical_path: name,
            digest: sha256_hex(&bytes),
            bytes,
        });
    }
    if bundle_files.is_empty() {
        return Err(WorkflowError::single(Diagnostic::new(
            file_label,
            "checks.bundle",
            ErrorCode::MissingInput,
            format!("test bundle `{}` is empty", checked.checks_bundle),
            format!(
                "place the frozen test files in {}",
                resolution.check_bundle_dir.display()
            ),
        )));
    }
    let mut bundle_digest_input = Vec::new();
    for file in &bundle_files {
        bundle_digest_input.extend_from_slice(file.logical_path.as_bytes());
        bundle_digest_input.push(0);
        bundle_digest_input.extend_from_slice(file.digest.as_bytes());
        bundle_digest_input.push(0);
    }
    Ok(ResolvedInputs {
        task,
        references,
        test_bundle_name: checked.checks_bundle.clone(),
        test_bundle: bundle_files,
        test_bundle_digest: sha256_hex(&bundle_digest_input),
        check_image_digest: resolution.check_image_digest.clone(),
        repository: resolution.repository.clone(),
        module_digests: resolution.module_digests.clone(),
        selections: resolution.selections.clone(),
    })
}

/// Resolve the repository `HEAD` selector in `repo_dir` to an exact
/// commit. The manifest digest covers the tracked files below the
/// directory.
pub fn resolve_repository(
    repo_dir: &Path,
    file_label: &str,
    repository_id: &str,
) -> WorkflowResult<RepositoryInput> {
    let git = |args: &[&str]| {
        let request = jackin_process::ExecRequest::new("git", args)
            .cwd(repo_dir)
            .timeout(Duration::from_secs(10));
        jackin_process::exec_sync(&request)
    };
    let missing = |detail: &str| {
        WorkflowError::single(Diagnostic::new(
            file_label,
            "repository.revision",
            ErrorCode::MissingInput,
            format!("cannot resolve repository HEAD: {detail}"),
            "run inside a git checkout with at least one commit",
        ))
    };
    let head = git(&["rev-parse", "HEAD"]).map_err(|error| missing(&error.to_string()))?;
    if !head.success {
        return Err(missing("not a git checkout with a HEAD commit"));
    }
    let commit = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    let manifest =
        git(&["ls-files", "-s", "--", "."]).map_err(|error| missing(&error.to_string()))?;
    if !manifest.success {
        return Err(missing("cannot list tracked files"));
    }
    let manifest_digest = sha256_hex(&manifest.stdout);
    Ok(RepositoryInput {
        repository_id: repository_id.to_owned(),
        base_commit: commit,
        manifest_digest,
    })
}

/// Resolve live agent selections. Live selection wiring arrives in a
/// later step; this always reports the gap as a typed error.
pub fn resolve_live_selections(file_label: &str) -> WorkflowResult<Selections> {
    Err(WorkflowError::single(Diagnostic::new(
        file_label,
        "implementer.account",
        ErrorCode::MissingInput,
        "live agent selection is not wired in this release",
        "rerun once the workflow CLI resolves the existing workspace selections",
    )))
}
