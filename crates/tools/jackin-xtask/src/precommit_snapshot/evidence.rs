// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs::{self, DirBuilder},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use super::process::create_private_new;
use anyhow::{Context, Result, bail, ensure};
use nix::unistd::geteuid;
use uuid::Uuid;

pub(super) fn persist(common_git_dir: &Path, patch: &[u8]) -> Result<PathBuf> {
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
    match fs::symlink_metadata(&root) {
        Ok(metadata) => verify_private_directory(&root, &metadata, "hook evidence root")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            DirBuilder::new()
                .recursive(false)
                .mode(0o700)
                .create(&root)
                .context("creating private hook evidence root")?;
            let metadata =
                fs::symlink_metadata(&root).context("checking private hook evidence root")?;
            verify_private_directory(&root, &metadata, "hook evidence root")?;
        }
        Err(error) => return Err(error).context("checking hook evidence root"),
    }

    let directory = create_unique_directory(&root)?;
    let patch_path = directory.join("fixes.patch");
    let mut file =
        create_private_new(&patch_path).context("creating private staged-source evidence")?;
    file.write_all(patch)
        .context("writing private staged-source evidence")?;
    file.sync_all()
        .context("syncing private staged-source evidence")?;
    let metadata = file
        .metadata()
        .context("checking private staged-source evidence")?;
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
    Ok(directory.join("fixes.patch"))
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

fn verify_private_directory(path: &Path, metadata: &fs::Metadata, role: &str) -> Result<()> {
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

fn create_unique_directory(root: &Path) -> Result<PathBuf> {
    for _ in 0..8 {
        let name = format!("pre-commit-{}", Uuid::new_v4());
        let path = root.join(name);
        match DirBuilder::new().recursive(false).mode(0o700).create(&path) {
            Ok(()) => {
                let metadata =
                    fs::symlink_metadata(&path).context("checking evidence directory")?;
                verify_private_directory(&path, &metadata, "hook evidence invocation directory")?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error).context("creating unique private evidence directory"),
        }
    }
    bail!("unable to allocate unique private hook evidence directory")
}
