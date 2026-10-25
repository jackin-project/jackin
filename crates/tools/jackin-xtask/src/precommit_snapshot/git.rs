// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::VecDeque,
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{Read, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
    },
    path::{Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{Context, Result, bail, ensure};
use jackin_process::SpoolCleanupObservation;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::process::{CommandBudget, EnvironmentMode, SetupControls, capture_strict};
#[cfg(test)]
use std::time::Duration;
mod snapshot;

const LIVE_GIT_STDOUT_LIMIT: usize = super::process::METADATA_OUTPUT_LIMIT;
const LIVE_GIT_STDERR_LIMIT: usize = 1024 * 1024;

#[derive(Debug)]
pub(super) struct LiveRepository {
    pub(super) invocation_dir: PathBuf,
    pub(super) root: PathBuf,
    pub(super) index: PathBuf,
    pub(super) objects: PathBuf,
    pub(super) common_git_dir: PathBuf,
    pub(super) worktree_git_dir: PathBuf,
    pub(super) shallow: Option<PathBuf>,
    pub(super) alternates: Option<OsString>,
    pub(super) object_format: OsString,
    pub(super) head: Option<OsString>,
    pub(super) head_tree: Option<String>,
    index_inputs: Vec<IndexInput>,
    controls: SetupControls,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IndexInput {
    path: PathBuf,
    device: u64,
    inode: u64,
    mode: u32,
    links: u64,
    length: u64,
    mtime: i64,
    mtime_nsec: i64,
    digest: [u8; 32],
}

impl LiveRepository {
    pub(super) fn capture(controls: &SetupControls) -> Result<Self> {
        Self::capture_at_with_controls(
            &env::current_dir().context("reading current directory")?,
            controls,
        )
    }

    #[cfg(test)]
    pub(super) fn capture_at(root: &Path) -> Result<Self> {
        Self::capture_at_with_controls(root, &SetupControls::for_test(Duration::from_secs(10 * 60)))
    }

    fn capture_at_with_controls(root: &Path, controls: &SetupControls) -> Result<Self> {
        let invocation_dir = fs::canonicalize(root).context("resolving invocation directory")?;
        let root =
            output_live_path_at(&invocation_dir, &["rev-parse", "--show-toplevel"], controls)?;
        let root = fs::canonicalize(&root).context("resolving physical repository root")?;
        let index = match env::var_os("GIT_INDEX_FILE") {
            Some(path) => absolute_from(&invocation_dir, PathBuf::from(path)),
            None => output_live_path_at(
                &root,
                &["rev-parse", "--path-format=absolute", "--git-path", "index"],
                controls,
            )?,
        };
        let objects = match env::var_os("GIT_OBJECT_DIRECTORY") {
            Some(path) => absolute_from(&invocation_dir, PathBuf::from(path)),
            None => output_live_path_at(
                &root,
                &[
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-path",
                    "objects",
                ],
                controls,
            )?,
        };
        let common_git_dir = output_live_path_at(
            &root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            controls,
        )?;
        let worktree_git_dir =
            output_live_path_at(&root, &["rev-parse", "--absolute-git-dir"], controls)?;
        let shallow = match env::var_os("GIT_SHALLOW_FILE") {
            Some(path) => absolute_from(&invocation_dir, PathBuf::from(path)),
            None => output_live_path_at(
                &root,
                &[
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-path",
                    "shallow",
                ],
                controls,
            )?,
        };
        let object_format = OsString::from(output_live_at(
            &root,
            &["rev-parse", "--show-object-format"],
            controls,
        )?);
        let head = optional_live_at(
            &root,
            &["rev-parse", "--verify", "--quiet", "HEAD"],
            controls,
        )?
        .map(OsString::from);
        let head_tree = optional_live_at(
            &root,
            &["rev-parse", "--verify", "--quiet", "HEAD^{tree}"],
            controls,
        )?;
        let index_inputs = index_inputs(&index)?;
        let alternates = env::var_os("GIT_ALTERNATE_OBJECT_DIRECTORIES");
        ensure!(objects.is_dir(), "Git object directory is missing");
        ensure!(common_git_dir.is_dir(), "Git common directory is missing");
        ensure!(
            worktree_git_dir.is_dir(),
            "Git worktree directory is missing"
        );
        Ok(Self {
            invocation_dir,
            root,
            index,
            objects,
            common_git_dir,
            worktree_git_dir,
            shallow: (!shallow.as_os_str().is_empty() && shallow.is_file()).then_some(shallow),
            alternates,
            object_format,
            head,
            head_tree,
            index_inputs,
            controls: controls.clone(),
        })
    }
}

fn absolute_from(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn index_inputs(index: &Path) -> Result<Vec<IndexInput>> {
    let mut paths = Vec::new();
    match fs::symlink_metadata(index) {
        Ok(_) => paths.push(index.to_path_buf()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspecting live index {}", index.display()));
        }
    }
    if let Some(parent) = index.parent()
        && parent.is_dir()
    {
        for entry in
            crate::fs_util::read_dir_sorted(parent).context("reading live index directory")?
        {
            if entry.file_name().as_bytes().starts_with(b"sharedindex.") {
                paths.push(entry.path());
            }
        }
    }
    paths.sort();
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        let before = fs::symlink_metadata(&path)
            .with_context(|| format!("inspecting live index input {}", path.display()))?;
        ensure!(
            before.file_type().is_file(),
            "live index input is not a regular file: {}",
            path.display()
        );
        let mut file = super::process::open_readonly(&path)
            .with_context(|| format!("opening live index input {}", path.display()))?;
        let opened = file.metadata().context("reading opened index metadata")?;
        ensure!(
            same_file_identity(&before, &opened),
            "live index input changed while opening"
        );
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024].into_boxed_slice();
        loop {
            let count = file.read(&mut buffer).context("hashing live index input")?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        let after_fd = file.metadata().context("restating opened index input")?;
        let after_path = fs::symlink_metadata(&path)
            .with_context(|| format!("restating live index input {}", path.display()))?;
        ensure!(
            same_file_identity(&before, &after_fd)
                && same_file_identity(&before, &after_path)
                && same_file_content_state(&before, &after_fd)
                && same_file_content_state(&before, &after_path),
            "live index input changed while it was hashed"
        );
        inputs.push(IndexInput {
            path,
            device: before.dev(),
            inode: before.ino(),
            mode: before.mode(),
            links: before.nlink(),
            length: before.len(),
            mtime: before.mtime(),
            mtime_nsec: before.mtime_nsec(),
            digest: hasher.finalize().into(),
        });
    }
    if !inputs.iter().any(|input| input.path == index) {
        ensure!(
            !index.exists(),
            "live Git index could not be admitted as a regular file"
        );
    }
    Ok(inputs)
}

fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino() && left.file_type() == right.file_type()
}

fn same_file_content_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.mode() == right.mode()
        && left.nlink() == right.nlink()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
}

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
    command
}

fn output_live_path_at(root: &Path, args: &[&str], controls: &SetupControls) -> Result<PathBuf> {
    let mut command = Command::new("git");
    command.args(args).current_dir(root);
    let output = capture_strict(
        &mut command,
        controls,
        EnvironmentMode::Inherit,
        LIVE_GIT_STDOUT_LIMIT,
        LIVE_GIT_STDERR_LIMIT,
        CommandBudget::Probe,
    )
    .with_context(|| format!("running git {}", args.join(" ")))?;
    let bytes = checked(output, "reading live Git path")?;
    let path = without_record_terminator(&bytes)?;
    ensure!(
        !path.is_empty() && !path.contains(&0),
        "Git returned an invalid path record"
    );
    Ok(PathBuf::from(OsString::from_vec(path.to_vec())))
}

fn output_live_at(root: &Path, args: &[&str], controls: &SetupControls) -> Result<String> {
    let mut command = Command::new("git");
    command.args(args).current_dir(root);
    let output = capture_strict(
        &mut command,
        controls,
        EnvironmentMode::Inherit,
        LIVE_GIT_STDOUT_LIMIT,
        LIVE_GIT_STDERR_LIMIT,
        CommandBudget::Probe,
    )
    .with_context(|| format!("running git {}", args.join(" ")))?;
    let bytes = checked(output, "reading live Git text state")?;
    let record = without_record_terminator(&bytes)?;
    ensure!(
        !record.contains(&0) && !record.contains(&b'\n') && !record.contains(&b'\r'),
        "Git returned a malformed text record"
    );
    let text = std::str::from_utf8(record).context("decoding live Git text state")?;
    ensure!(text.is_ascii(), "Git returned non-ASCII text state");
    Ok(text.to_owned())
}

fn optional_live_at(
    root: &Path,
    args: &[&str],
    controls: &SetupControls,
) -> Result<Option<String>> {
    let mut command = Command::new("git");
    command.args(args).current_dir(root);
    let output = capture_strict(
        &mut command,
        controls,
        EnvironmentMode::Inherit,
        LIVE_GIT_STDOUT_LIMIT,
        LIVE_GIT_STDERR_LIMIT,
        CommandBudget::Probe,
    )
    .with_context(|| format!("running git {}", args.join(" ")))?;
    match output.status.code() {
        Some(0) => {
            let record = without_record_terminator(&output.stdout)?;
            ensure!(
                !record.contains(&0) && !record.contains(&b'\n') && !record.contains(&b'\r'),
                "Git returned a malformed text record"
            );
            let text = std::str::from_utf8(record).context("decoding live Git text state")?;
            ensure!(text.is_ascii(), "Git returned non-ASCII text state");
            Ok(Some(text.to_owned()))
        }
        Some(1) => Ok(None),
        _ => bail!("git {} failed with {}", args.join(" "), output.status),
    }
}

fn without_record_terminator(bytes: &[u8]) -> Result<&[u8]> {
    ensure!(
        bytes.last() == Some(&b'\n'),
        "Git output is missing its record terminator"
    );
    Ok(&bytes[..bytes.len() - 1])
}

fn checked(output: Output, operation: &str) -> Result<Vec<u8>> {
    if !output.status.success() {
        bail!("{operation} failed with {}", output.status);
    }
    Ok(output.stdout)
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
