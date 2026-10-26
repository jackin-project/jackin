// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs::{self, DirBuilder, Metadata},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use super::process::{SetupControls, create_private_new};
use anyhow::{Context, Result, bail, ensure};
use jackin_process_directory::{CheckedInstallRoot, InstallFileKind};
use nix::unistd::geteuid;
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use std::fs::File;
use uuid::Uuid;

pub(super) fn persist(
    common_git_dir: &Path,
    patch: &[u8],
    controls: &SetupControls,
) -> Result<PathBuf> {
    controls.check_start()?;
    ensure!(
        !patch.is_empty(),
        "refusing to persist an empty staged-source patch"
    );
    let physical = fs::canonicalize(common_git_dir).context("resolving Git common directory")?;
    ensure!(
        physical == common_git_dir,
        "Git common directory traverses a symlink"
    );
    validate_ancestors(&physical)?;

    let root = physical.join("jackin-hook-evidence");
    match DirBuilder::new().mode(0o700).create(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).context("creating private hook evidence root"),
    }
    let metadata = fs::symlink_metadata(&root).context("checking private hook evidence root")?;
    verify_private_directory(&root, &metadata, "hook evidence root")?;

    let root_descriptor = open_private_directory(&root).context("checking private evidence ACL")?;
    controls.check_start()?;
    root_descriptor
        .sync_all()
        .context("syncing private hook evidence root")?;
    controls.check_start()?;
    sync_directory(&physical).context("syncing Git common directory entry")?;
    controls.check_start()?;

    persist_invocation(&root, &root_descriptor, patch, controls)
}

fn persist_invocation(
    root: &Path,
    root_descriptor: &File,
    patch: &[u8],
    controls: &SetupControls,
) -> Result<PathBuf> {
    controls.check_start()?;
    let (directory, directory_identity) = create_unique_directory(root)?;
    let mut cleanup = FailedInvocationCleanup::new(directory.clone(), directory_identity);
    let invocation_descriptor =
        open_private_directory(&directory).context("checking invocation evidence ACL")?;
    controls.check_start()?;
    invocation_descriptor
        .sync_all()
        .context("syncing private hook evidence invocation directory")?;
    controls.check_start()?;
    root_descriptor
        .sync_all()
        .context("syncing invocation directory entry")?;
    controls.check_start()?;
    let patch_path = directory.join("fixes.patch");
    let mut file =
        create_private_new(&patch_path).context("creating private staged-source evidence")?;
    let metadata = file
        .metadata()
        .context("checking private staged-source evidence")?;
    cleanup.record_patch(&metadata);
    write_controlled(&mut file, patch, controls)
        .context("writing private staged-source evidence")?;
    controls.check_start()?;
    file.sync_all()
        .context("syncing private staged-source evidence")?;
    controls.check_start()?;
    ensure!(
        metadata.is_file() && metadata.nlink() == 1,
        "private staged-source evidence has unsafe file identity"
    );
    ensure!(
        metadata.uid() == geteuid().as_raw(),
        "private staged-source evidence has unexpected owner"
    );
    ensure!(
        metadata.permissions().mode() & 0o777 == 0o600,
        "private staged-source evidence permissions are not 0600"
    );
    let checked_root = CheckedInstallRoot::from_pinned_directory(
        invocation_descriptor
            .try_clone()
            .context("duplicating private invocation directory")?,
    )
    .context("pinning private invocation directory")?;
    let checked_file = checked_root
        .open_file(Path::new("fixes.patch"), InstallFileKind::Receipt)
        .context("reopening private staged-source evidence")?;
    CheckedInstallRoot::validate_confidential_file(checked_file.file())
        .context("checking private staged-source evidence ACL")?;
    checked_file
        .recheck()
        .context("rechecking private staged-source evidence custody")?;
    controls.check_start()?;
    invocation_descriptor
        .sync_all()
        .context("syncing private staged-source evidence directory")?;
    controls.check_start()?;
    cleanup.disarm();
    Ok(directory.join("fixes.patch"))
}

pub(super) fn write_controlled(
    file: &mut File,
    bytes: &[u8],
    controls: &SetupControls,
) -> Result<()> {
    for chunk in bytes.chunks(64 * 1024) {
        controls.check_start()?;
        file.write_all(chunk)?;
    }
    controls.check_start()
}

struct FailedInvocationCleanup {
    directory: PathBuf,
    directory_identity: (u64, u64),
    patch_identity: Option<(u64, u64)>,
    armed: bool,
}

impl FailedInvocationCleanup {
    fn new(directory: PathBuf, identity: (u64, u64)) -> Self {
        Self {
            directory,
            directory_identity: identity,
            patch_identity: None,
            armed: true,
        }
    }

    fn record_patch(&mut self, metadata: &Metadata) {
        self.patch_identity = Some((metadata.dev(), metadata.ino()));
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for FailedInvocationCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }

        if let (Some(expected), Ok(metadata)) = (
            self.patch_identity,
            fs::symlink_metadata(self.directory.join("fixes.patch")),
        ) && metadata.is_file()
            && !metadata.file_type().is_symlink()
            && (metadata.dev(), metadata.ino()) == expected
            && metadata.uid() == geteuid().as_raw()
            && metadata.permissions().mode() & 0o777 == 0o600
        {
            drop(fs::remove_file(self.directory.join("fixes.patch")));
        }

        if let Ok(metadata) = fs::symlink_metadata(&self.directory)
            && metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && (metadata.dev(), metadata.ino()) == self.directory_identity
            && metadata.uid() == geteuid().as_raw()
            && metadata.permissions().mode() & 0o777 == 0o700
        {
            // remove_dir succeeds only if this invocation left it empty.
            drop(fs::remove_dir(&self.directory));
        }
    }
}

fn open_private_directory(path: &Path) -> Result<File> {
    let descriptor = open(
        path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .with_context(|| format!("opening private directory {}", path.display()))?;
    let file = File::from(descriptor);
    CheckedInstallRoot::validate_confidential_directory(&file)
        .with_context(|| format!("validating private directory {}", path.display()))?;
    let path_metadata = fs::symlink_metadata(path)
        .with_context(|| format!("rechecking private directory path {}", path.display()))?;
    let descriptor_metadata = file
        .metadata()
        .with_context(|| format!("checking private directory descriptor {}", path.display()))?;
    ensure!(
        path_metadata.is_dir()
            && !path_metadata.file_type().is_symlink()
            && path_metadata.dev() == descriptor_metadata.dev()
            && path_metadata.ino() == descriptor_metadata.ino(),
        "private directory path no longer names its validated descriptor"
    );
    Ok(file)
}

fn sync_directory(path: &Path) -> Result<()> {
    let descriptor = open(
        path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .with_context(|| format!("opening directory {} for sync", path.display()))?;
    File::from(descriptor)
        .sync_all()
        .with_context(|| format!("syncing directory {}", path.display()))
}

fn validate_ancestors(path: &Path) -> Result<()> {
    let mut current = PathBuf::from("/");
    for component in path.components().skip(1) {
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .with_context(|| format!("checking Git directory ancestor {}", current.display()))?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "Git directory ancestor is not a physical directory"
        );
        let mode = metadata.permissions().mode();
        ensure!(
            mode & 0o022 == 0 || mode & 0o1000 != 0,
            "Git directory ancestor is group or world writable without a sticky bit"
        );
        ensure!(
            metadata.uid() == 0 || metadata.uid() == geteuid().as_raw(),
            "Git directory ancestor has unexpected owner"
        );
    }
    Ok(())
}

fn verify_private_directory(path: &Path, metadata: &Metadata, role: &str) -> Result<()> {
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "{role} is not a physical directory"
    );
    ensure!(
        metadata.uid() == geteuid().as_raw(),
        "{role} has unexpected owner"
    );
    ensure!(
        metadata.permissions().mode() & 0o777 == 0o700,
        "{role} permissions are not 0700"
    );
    ensure!(path.is_absolute(), "{role} path is not absolute");
    Ok(())
}

fn create_unique_directory(root: &Path) -> Result<(PathBuf, (u64, u64))> {
    for _ in 0..8 {
        let name = format!("pre-commit-{}", Uuid::new_v4());
        let path = root.join(name);
        match DirBuilder::new().recursive(false).mode(0o700).create(&path) {
            Ok(()) => {
                let metadata =
                    fs::symlink_metadata(&path).context("checking evidence directory")?;
                verify_private_directory(&path, &metadata, "hook evidence invocation directory")?;
                return Ok((path, (metadata.dev(), metadata.ino())));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error).context("creating unique private evidence directory"),
        }
    }
    bail!("unable to allocate unique private hook evidence directory")
}

#[cfg(test)]
mod tests;
