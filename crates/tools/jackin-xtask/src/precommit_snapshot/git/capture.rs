// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    env,
    ffi::OsString,
    fs,
    io::Read,
    os::unix::{ffi::OsStringExt, fs::MetadataExt},
    path::{Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};

use super::super::process::{CommandBudget, EnvironmentMode, SetupControls, capture_strict};
#[cfg(test)]
use std::time::Duration;

#[derive(Debug)]
pub(in crate::precommit_snapshot) struct LiveRepository {
    pub(in crate::precommit_snapshot) invocation_dir: PathBuf,
    pub(in crate::precommit_snapshot) root: PathBuf,
    pub(in crate::precommit_snapshot) index: PathBuf,
    pub(in crate::precommit_snapshot) shared_index: Option<PathBuf>,
    pub(in crate::precommit_snapshot) objects: PathBuf,
    pub(in crate::precommit_snapshot) common_git_dir: PathBuf,
    pub(in crate::precommit_snapshot) worktree_git_dir: PathBuf,
    pub(in crate::precommit_snapshot) shallow: Option<PathBuf>,
    pub(in crate::precommit_snapshot) alternates: Option<OsString>,
    pub(in crate::precommit_snapshot) object_format: OsString,
    pub(in crate::precommit_snapshot) head: Option<OsString>,
    pub(in crate::precommit_snapshot) index_inputs: Vec<IndexInput>,
    pub(in crate::precommit_snapshot) controls: SetupControls,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::precommit_snapshot) struct IndexInput {
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

impl IndexInput {
    pub(super) fn admitted_path(&self) -> &Path {
        &self.path
    }

    pub(super) fn admitted_length(&self) -> u64 {
        self.length
    }

    pub(super) fn admitted_digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

impl LiveRepository {
    pub(in crate::precommit_snapshot) fn capture(controls: &SetupControls) -> Result<Self> {
        Self::capture_at_with_controls(
            &env::current_dir().context("reading current directory")?,
            controls,
        )
    }

    #[cfg(test)]
    pub(in crate::precommit_snapshot) fn capture_at(root: &Path) -> Result<Self> {
        Self::capture_at_with_controls(root, &SetupControls::for_test(Duration::from_mins(10)))
    }

    fn capture_at_with_controls(root: &Path, controls: &SetupControls) -> Result<Self> {
        let invocation_dir = fs::canonicalize(root).context("resolving invocation directory")?;
        let root =
            output_live_path_at(&invocation_dir, &["rev-parse", "--show-toplevel"], controls)?;
        let root = fs::canonicalize(&root).context("resolving physical repository root")?;
        let index = output_live_path_at(
            &root,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
            controls,
        )?;
        let shared_index = output_live_optional_path_at(
            &root,
            &["rev-parse", "--path-format=absolute", "--shared-index-path"],
            controls,
        )?;
        let objects = output_live_path_at(
            &root,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "objects",
            ],
            controls,
        )?;
        let common_git_dir = output_live_path_at(
            &root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            controls,
        )?;
        let worktree_git_dir =
            output_live_path_at(&root, &["rev-parse", "--absolute-git-dir"], controls)?;
        let shallow = match env::var_os("GIT_SHALLOW_FILE") {
            Some(path) => {
                let path = PathBuf::from(path);
                if path.is_absolute() {
                    path
                } else {
                    root.join(path)
                }
            }
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
        let index_inputs = index_inputs(&index, shared_index.as_deref(), controls)?;
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
            shared_index,
            objects,
            common_git_dir,
            worktree_git_dir,
            shallow: (!shallow.as_os_str().is_empty() && shallow.is_file()).then_some(shallow),
            alternates,
            object_format,
            head,
            index_inputs,
            controls: controls.clone(),
        })
    }
}

pub(in crate::precommit_snapshot) fn index_inputs(
    index: &Path,
    shared_index: Option<&Path>,
    controls: &SetupControls,
) -> Result<Vec<IndexInput>> {
    let mut paths = Vec::with_capacity(2);
    match fs::symlink_metadata(index) {
        Ok(_) => paths.push(index.to_path_buf()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspecting live index {}", index.display()));
        }
    }
    if let Some(shared_index) = shared_index {
        paths.push(shared_index.to_path_buf());
    }
    paths.sort();
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        controls.check_start()?;
        let before = fs::symlink_metadata(&path)
            .with_context(|| format!("inspecting live index input {}", path.display()))?;
        ensure!(
            before.file_type().is_file(),
            "live index input is not a regular file: {}",
            path.display()
        );
        let mut file = super::super::process::open_readonly(&path)
            .with_context(|| format!("opening live index input {}", path.display()))?;
        let opened = file.metadata().context("reading opened index metadata")?;
        ensure!(
            same_file_identity(&before, &opened),
            "live index input changed while opening"
        );
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024].into_boxed_slice();
        loop {
            controls.check_start()?;
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

pub(in crate::precommit_snapshot) fn output_live_path_at(
    root: &Path,
    args: &[&str],
    controls: &SetupControls,
) -> Result<PathBuf> {
    output_live_optional_path_at(root, args, controls)?.context("Git returned an empty path record")
}

pub(in crate::precommit_snapshot) fn output_live_optional_path_at(
    root: &Path,
    args: &[&str],
    controls: &SetupControls,
) -> Result<Option<PathBuf>> {
    let mut command = Command::new("git");
    command.args(args).current_dir(root);
    let output = capture_strict(
        &mut command,
        controls,
        EnvironmentMode::Inherit,
        super::super::process::METADATA_OUTPUT_LIMIT,
        1024 * 1024,
        CommandBudget::Probe,
    )
    .with_context(|| format!("running git {}", args.join(" ")))?;
    let bytes = checked(output, "reading live Git path")?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let path = without_record_terminator(&bytes)?;
    ensure!(!path.contains(&0), "Git returned an invalid path record");
    Ok((!path.is_empty()).then(|| PathBuf::from(OsString::from_vec(path.to_vec()))))
}

pub(in crate::precommit_snapshot) fn output_live_at(
    root: &Path,
    args: &[&str],
    controls: &SetupControls,
) -> Result<String> {
    let mut command = Command::new("git");
    command.args(args).current_dir(root);
    let output = capture_strict(
        &mut command,
        controls,
        EnvironmentMode::Inherit,
        super::super::process::METADATA_OUTPUT_LIMIT,
        1024 * 1024,
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
        super::super::process::METADATA_OUTPUT_LIMIT,
        1024 * 1024,
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

pub(in crate::precommit_snapshot) fn without_record_terminator(bytes: &[u8]) -> Result<&[u8]> {
    ensure!(
        bytes.last() == Some(&b'\n'),
        "Git output is missing its record terminator"
    );
    Ok(&bytes[..bytes.len() - 1])
}

pub(in crate::precommit_snapshot) fn checked(output: Output, operation: &str) -> Result<Vec<u8>> {
    if !output.status.success() {
        bail!("{operation} failed with {}", output.status);
    }
    Ok(output.stdout)
}
