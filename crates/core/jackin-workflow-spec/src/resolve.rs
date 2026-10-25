// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::digest::{bundle_manifest_digest, sha256_hex};
use crate::error::{Diagnostic, ErrorCode, WorkflowError, WorkflowResult};
use crate::model::RepositoryInput;
use crate::rules::{CheckedDefinition, Selections};

const MAX_BUNDLE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenInput {
    pub logical_path: String,

    pub bytes: Vec<u8>,

    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInputs {
    pub task: FrozenInput,

    pub references: Vec<FrozenInput>,

    pub test_bundle_name: String,

    pub test_bundle: Vec<FrozenInput>,

    pub test_bundle_digest: String,

    pub check_image_digest: String,

    pub repository: RepositoryInput,

    pub module_digests: BTreeMap<String, String>,

    pub selections: Selections,
}

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

#[derive(Debug, Clone)]
pub struct FileResolution {
    pub workflow_file: PathBuf,

    pub file_label: String,

    pub check_bundle_dir: PathBuf,

    pub check_image_digest: String,

    pub module_digests: BTreeMap<String, String>,

    pub repository: RepositoryInput,

    pub selections: Selections,
}

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
    let test_bundle_digest = bundle_manifest_digest(
        bundle_files
            .iter()
            .map(|file| (file.logical_path.as_str(), file.digest.as_str())),
    );
    Ok(ResolvedInputs {
        task,
        references,
        test_bundle_name: checked.checks_bundle.clone(),
        test_bundle: bundle_files,
        test_bundle_digest,
        check_image_digest: resolution.check_image_digest.clone(),
        repository: resolution.repository.clone(),
        module_digests: resolution.module_digests.clone(),
        selections: resolution.selections.clone(),
    })
}

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

pub fn resolve_workspace_repository(
    candidates: &[PathBuf],
    file_label: &str,
    repository_id: &str,
    workspace_name: &str,
) -> WorkflowResult<RepositoryInput> {
    for candidate in candidates {
        if head_commit(candidate).is_some() {
            return resolve_repository(candidate, file_label, repository_id);
        }
    }
    Err(WorkflowError::single(Diagnostic::new(
        file_label,
        "repository.revision",
        ErrorCode::MissingInput,
        format!("workspace `{workspace_name}` has no git checkout with a HEAD commit"),
        "point the workspace workdir or a mount source at a git checkout and retry",
    )))
}

fn head_commit(dir: &Path) -> Option<String> {
    let request = jackin_process::ExecRequest::new("git", ["rev-parse", "HEAD"])
        .cwd(dir)
        .timeout(Duration::from_secs(10));
    let outcome = jackin_process::exec_sync(&request).ok()?;
    if !outcome.success {
        return None;
    }
    let commit = String::from_utf8_lossy(&outcome.stdout).trim().to_owned();
    if commit.is_empty() {
        None
    } else {
        Some(commit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAgentFacts {
    pub runtime: String,

    pub agent_slug: String,

    pub account_id: String,

    pub auth_mode: String,

    pub requested_model: Option<String>,

    pub actual_model: Option<String>,

    pub binary_digest: String,

    pub image_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSelectionEnv {
    pub workspace_name: String,

    pub role_id: String,

    pub role_revision: String,

    pub implementer: LiveAgentFacts,

    pub reviewer: LiveAgentFacts,
}

pub fn resolve_live_selections(
    file_label: &str,
    env: &LiveSelectionEnv,
) -> WorkflowResult<Selections> {
    let missing = |field: String, message: String, action: &str| {
        WorkflowError::single(Diagnostic::new(
            file_label,
            field,
            ErrorCode::MissingInput,
            message,
            action,
        ))
    };
    if env.workspace_name.is_empty() {
        return Err(missing(
            "workspace".to_owned(),
            "no workspace selection was provided".to_owned(),
            "select the existing workspace before planning",
        ));
    }
    if env.role_id.is_empty() {
        return Err(missing(
            "role".to_owned(),
            format!("workspace `{}` has no role selected", env.workspace_name),
            "run `jackin load <role>` to select a role for the workspace",
        ));
    }
    if env.role_revision.is_empty() {
        return Err(missing(
            "role.revision".to_owned(),
            format!("role `{}` has no resolvable revision", env.role_id),
            "refresh the cached role checkout and retry",
        ));
    }
    Ok(Selections {
        implementer: resolve_live_agent(
            file_label,
            "implementer",
            "claude",
            &env.role_id,
            &env.role_revision,
            &env.implementer,
            &missing,
        )?,
        reviewer: resolve_live_agent(
            file_label,
            "reviewer",
            "codex",
            &env.role_id,
            &env.role_revision,
            &env.reviewer,
            &missing,
        )?,
    })
}

fn resolve_live_agent(
    file_label: &str,
    side: &str,
    expected_runtime: &str,
    role_id: &str,
    role_revision: &str,
    facts: &LiveAgentFacts,
    missing: &dyn Fn(String, String, &str) -> WorkflowError,
) -> WorkflowResult<crate::model::AgentSelection> {
    if facts.runtime != expected_runtime {
        return Err(WorkflowError::single(Diagnostic::new(
            file_label,
            format!("{side}.runtime"),
            ErrorCode::InvalidDefinition,
            format!(
                "{side} selection uses runtime `{}`, expected `{expected_runtime}`",
                facts.runtime
            ),
            format!("resolve the {side} selection for the definition runtime"),
        )));
    }
    let require = |name: &'static str, value: &str, action: &'static str| {
        if value.is_empty() {
            Err(missing(
                format!("{side}.{name}"),
                format!("{side} selection is missing `{name}`"),
                action,
            ))
        } else {
            Ok(())
        }
    };
    require(
        "agent",
        &facts.agent_slug,
        "repair the workspace agent selection",
    )?;
    require(
        "account",
        &facts.account_id,
        "assign an account to the workspace",
    )?;
    require(
        "auth_mode",
        &facts.auth_mode,
        "repair the workspace account configuration",
    )?;
    require(
        "binary",
        &facts.binary_digest,
        "install the agent binary on PATH",
    )?;
    require("image", &facts.image_digest, "build or pull the role image")?;
    let Some(requested_model) = facts
        .requested_model
        .as_deref()
        .filter(|model| !model.is_empty())
    else {
        return Err(missing(
            format!("{side}.model"),
            format!("{side} selection has no requested model"),
            "set a model on the agent configuration or account",
        ));
    };
    Ok(crate::model::AgentSelection {
        runtime: facts.runtime.clone(),
        agent_slug: facts.agent_slug.clone(),
        role_id: role_id.to_owned(),
        role_revision: role_revision.to_owned(),
        account_id: facts.account_id.clone(),
        auth_mode: facts.auth_mode.clone(),
        requested_model: requested_model.to_owned(),
        actual_model: facts.actual_model.clone(),
        binary_digest: facts.binary_digest.clone(),
        image_digest: facts.image_digest.clone(),
    })
}
