// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::VecDeque,
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use jackin_process::SpoolCleanupObservation;
use tempfile::TempDir;

use super::process::SetupControls;

mod capture;
mod snapshot;
pub(in crate::precommit_snapshot) use capture::{LiveRepository, checked, index_inputs};
#[cfg(test)]
pub(in crate::precommit_snapshot) use capture::{
    output_live_at, output_live_optional_path_at, output_live_path_at, without_record_terminator,
};

pub(super) struct OwnedSnapshot {
    directory: Option<TempDir>,
    pub(super) scratch_root: PathBuf,
    pub(super) root: PathBuf,
    git_dir: PathBuf,
    index: PathBuf,
    objects: PathBuf,
    invocation_dir: PathBuf,
    extra_alternates: Option<OsString>,
    pub(in crate::precommit_snapshot) controls: SetupControls,
    #[cfg(test)]
    test_git_program: Option<PathBuf>,
}

pub(super) struct GateCompletion {
    pub(super) success: bool,
    pub(super) code: Option<i32>,
    pub(super) signal: Option<i32>,
    pub(super) execution: jackin_process::SpoolExecutionObservation,
    pub(super) capture: jackin_process::SpoolCaptureObservation,
    pub(super) failures: Vec<jackin_process::SpoolFailure>,
    pub(super) cleanup: SpoolCleanupObservation,
}

fn isolated_git(
    invocation_dir: &Path,
    root: &Path,
    git_dir: &Path,
    index: &Path,
    objects: &Path,
    extra_alternates: Option<&OsStr>,
    args: &[impl AsRef<OsStr>],
) -> Command {
    let mut command = Command::new("git");
    command.args(args.iter().map(AsRef::as_ref));
    configure_isolated_git(
        &mut command,
        invocation_dir,
        root,
        git_dir,
        index,
        objects,
        extra_alternates,
    );
    command
}

#[cfg(test)]
#[expect(
    clippy::too_many_arguments,
    reason = "private test seam mirrors the isolated Git command boundary"
)]
fn isolated_git_with_program(
    program: &Path,
    invocation_dir: &Path,
    root: &Path,
    git_dir: &Path,
    index: &Path,
    objects: &Path,
    extra_alternates: Option<&OsStr>,
    args: &[impl AsRef<OsStr>],
) -> Command {
    let mut command = Command::new(program);
    command.args(args.iter().map(AsRef::as_ref));
    configure_isolated_git(
        &mut command,
        invocation_dir,
        root,
        git_dir,
        index,
        objects,
        extra_alternates,
    );
    command
}

fn configure_isolated_git(
    command: &mut Command,
    invocation_dir: &Path,
    root: &Path,
    git_dir: &Path,
    index: &Path,
    objects: &Path,
    extra_alternates: Option<&OsStr>,
) {
    command
        .current_dir(invocation_dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_DIR", git_dir)
        .env("GIT_WORK_TREE", root)
        .env("GIT_INDEX_FILE", index)
        .env("GIT_OBJECT_DIRECTORY", objects)
        .env("GIT_OPTIONAL_LOCKS", "0");
    if let Some(alternates) = extra_alternates {
        command.env("GIT_ALTERNATE_OBJECT_DIRECTORIES", alternates);
    }
}

fn private_spool(path: &Path) -> Result<fs::File> {
    super::process::create_private_new(path)
        .with_context(|| format!("creating private gate output spool {}", path.display()))
}

fn write_private_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = super::process::create_private_new(path)
        .with_context(|| format!("creating private path list {}", path.display()))?;
    file.write_all(bytes)
        .context("writing private new-path list")?;
    file.sync_all().context("syncing private new-path list")
}

pub(super) fn validate_link(root: &Path, path: &Path) -> Result<()> {
    let relative = path
        .strip_prefix(root)
        .context("staged symlink is outside snapshot")?;
    let mut pending = relative
        .components()
        .map(|component| component.as_os_str().to_os_string())
        .collect::<VecDeque<_>>();
    let mut resolved = PathBuf::new();
    let mut link_count = 0;
    while let Some(component) = pending.pop_front() {
        if component == "." {
            continue;
        }
        if component == ".." {
            ensure!(resolved.pop(), "staged symlink escapes pre-commit snapshot");
            continue;
        }
        let candidate = root.join(&resolved).join(&component);
        match fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                link_count += 1;
                ensure!(
                    link_count <= 40,
                    "staged symlink chain exceeds resolution limit"
                );
                let target = fs::read_link(&candidate)
                    .with_context(|| format!("reading staged symlink {}", candidate.display()))?;
                ensure!(
                    !target.is_absolute(),
                    "staged symlink has an absolute target"
                );
                let rest = pending;
                pending = target
                    .components()
                    .map(|component| component.as_os_str().to_os_string())
                    .collect();
                pending.extend(rest);
            }
            Ok(metadata) => {
                ensure!(
                    pending.is_empty() || metadata.is_dir(),
                    "staged symlink path traverses a non-directory"
                );
                resolved.push(component);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                resolved.push(component);
                finish_missing_suffix(&mut resolved, &mut pending)?;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("resolving staged symlink {}", candidate.display()));
            }
        }
    }
    Ok(())
}

fn finish_missing_suffix(resolved: &mut PathBuf, pending: &mut VecDeque<OsString>) -> Result<()> {
    while let Some(rest) = pending.pop_front() {
        match rest.to_str() {
            Some(".") => {}
            Some("..") => ensure!(resolved.pop(), "staged symlink escapes pre-commit snapshot"),
            _ => resolved.push(rest),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
