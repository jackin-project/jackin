// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveProfile {
    Release,
    Preview,
}

impl ArchiveProfile {
    pub(super) fn arguments(self) -> &'static [&'static str] {
        match self {
            Self::Release => &["--release"],
            Self::Preview => &["--profile", "preview"],
        }
    }
    fn directory(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Preview => "preview",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ArchiveLayout {
    root: PathBuf,
    target: &'static str,
    profile: ArchiveProfile,
}

impl ArchiveLayout {
    pub(crate) fn new(root: &Path, target: &'static str, profile: ArchiveProfile) -> Result<Self> {
        ensure!(
            super::JACKIN_TARGETS.iter().any(|t| t.rust == target),
            "unknown archive layout target"
        );
        let root = std::path::absolute(root).context("resolving Cargo build root")?;
        ensure!(
            root.components()
                .all(|c| !matches!(c, Component::ParentDir | Component::CurDir)),
            "noncanonical Cargo build root"
        );
        let root = canonical_root(&root)?;
        Ok(Self {
            root,
            target,
            profile,
        })
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn target(&self) -> &'static str {
        self.target
    }
    pub(super) fn profile(&self) -> ArchiveProfile {
        self.profile
    }
    pub(crate) fn target_output(&self) -> PathBuf {
        self.root.join(self.target).join(self.profile.directory())
    }
    pub(crate) fn host_output(&self) -> PathBuf {
        self.root.join(self.profile.directory())
    }
    pub(crate) fn artifacts(&self) -> PathBuf {
        self.root.join(match self.profile {
            ArchiveProfile::Release => "cargo-build-artifacts.jsonl",
            ArchiveProfile::Preview => "cargo-build-artifacts-preview.jsonl",
        })
    }

    pub(crate) fn validate_path(&self, path: &Path) -> Result<()> {
        ensure!(
            path.is_absolute()
                && path
                    .components()
                    .all(|c| !matches!(c, Component::ParentDir | Component::CurDir)),
            "noncanonical Cargo artifact path"
        );
        let relative = path
            .strip_prefix(&self.root)
            .context("Cargo artifact is outside build root")?;
        let mut current = self.root.clone();
        for part in std::iter::once(None).chain(relative.components().map(Some)) {
            if let Some(part) = part {
                current.push(part);
            }
            match fs::symlink_metadata(&current) {
                Ok(meta) => ensure!(
                    !meta.file_type().is_symlink(),
                    "Cargo artifact path contains a symlink alias"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("checking Cargo artifact custody"),
            }
        }
        Ok(())
    }
}

fn canonical_root(root: &Path) -> Result<PathBuf> {
    let mut existing = root;
    let mut suffix = Vec::new();
    while !existing.exists() {
        suffix.push(
            existing
                .file_name()
                .context("Cargo build root has no existing ancestor")?,
        );
        existing = existing
            .parent()
            .context("Cargo build root has no parent")?;
    }
    let mut canonical =
        fs::canonicalize(existing).context("canonicalizing trusted Cargo build root")?;
    for part in suffix.into_iter().rev() {
        canonical.push(part);
    }
    Ok(canonical)
}
