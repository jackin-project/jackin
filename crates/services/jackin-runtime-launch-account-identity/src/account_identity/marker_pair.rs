// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cooperative serialization for the selected and admission marker pair.
//!
//! This gate assumes an existing trusted container root and cooperating
//! writers. It does not make a same-uid namespace hostile to its owner.

use anyhow::{Context as _, ensure};
use rustix::fs::{AtFlags, FlockOperation, Mode, OFlags, flock, open, statat};
use std::fs::{File, Metadata};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RootIdentity {
    device: u64,
    inode: u64,
}

impl RootIdentity {
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

/// Holds an independently-opened root descriptor and its nonblocking flock.
pub(super) struct MarkerPairLease {
    root: PathBuf,
    directory: File,
    identity: RootIdentity,
}

impl MarkerPairLease {
    pub(super) fn acquire(root: &Path) -> anyhow::Result<Self> {
        Self::acquire_inner(root, false)?
            .ok_or_else(|| anyhow::anyhow!("marker root is absent: {}", root.display()))
    }

    pub(super) fn acquire_optional(root: &Path) -> anyhow::Result<Option<Self>> {
        Self::acquire_inner(root, true)
    }

    fn acquire_inner(root: &Path, missing_is_false: bool) -> anyhow::Result<Option<Self>> {
        let named_before = match std::fs::symlink_metadata(root) {
            Ok(metadata) => metadata,
            Err(error) if missing_is_false && error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot inspect marker root {}", root.display()));
            }
        };
        validate_root_metadata(&named_before, root)?;

        let directory = match open(
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(descriptor) => File::from(descriptor),
            Err(error) if missing_is_false && error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "cannot open trusted marker root {} without following links",
                        root.display()
                    )
                });
            }
        };
        let descriptor_before = directory
            .metadata()
            .context("cannot inspect opened marker root descriptor")?;
        validate_root_metadata(&descriptor_before, root)?;

        let identity = RootIdentity::from_metadata(&descriptor_before);
        ensure!(
            identity == RootIdentity::from_metadata(&named_before),
            "marker root identity changed while it was being opened: {}",
            root.display()
        );

        flock(&directory, FlockOperation::NonBlockingLockExclusive).with_context(|| {
            format!(
                "marker root is busy or locking is unsupported: {}",
                root.display()
            )
        })?;

        let lease = Self {
            root: root.to_path_buf(),
            directory,
            identity,
        };
        lease.ensure_root_identity()?;
        Ok(Some(lease))
    }

    /// Rechecks that the opened directory is still the safe named root.
    pub(super) fn ensure_root_identity(&self) -> anyhow::Result<()> {
        let descriptor = self
            .directory
            .metadata()
            .context("cannot recheck opened marker root descriptor")?;
        let named = std::fs::symlink_metadata(&self.root)
            .with_context(|| format!("cannot recheck marker root {}", self.root.display()))?;
        validate_root_metadata(&descriptor, &self.root)?;
        validate_root_metadata(&named, &self.root)?;

        let descriptor_identity = RootIdentity::from_metadata(&descriptor);
        let named_identity = RootIdentity::from_metadata(&named);
        ensure!(
            descriptor_identity == self.identity
                && named_identity == self.identity
                && descriptor_identity == named_identity,
            "marker root identity changed while its lease was held: {}",
            self.root.display()
        );
        Ok(())
    }

    /// Refuses any reserved journal entry without opening or modifying it.
    pub(super) fn ensure_no_reserved_journal(&self, name: &str) -> anyhow::Result<()> {
        self.ensure_root_identity()?;
        match statat(&self.directory, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => anyhow::bail!(
                "marker recovery is required; reserved transaction entry is present: {name}"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("cannot inspect reserved marker transaction entry {name}")
                });
            }
        }
        self.ensure_root_identity()
    }
}

fn validate_root_metadata(metadata: &Metadata, root: &Path) -> anyhow::Result<()> {
    ensure!(
        !metadata.file_type().is_symlink() && metadata.is_dir(),
        "marker root must be an existing non-symlink directory: {}",
        root.display()
    );
    ensure!(
        metadata.uid() == rustix::process::geteuid().as_raw(),
        "marker root must be owned by the effective user: {}",
        root.display()
    );
    let mode = metadata.permissions().mode();
    ensure!(
        mode & 0o022 == 0,
        "marker root must not be group- or other-writable: {}",
        root.display()
    );
    Ok(())
}

#[cfg(test)]
type SelectedRenameHook = Box<dyn FnOnce(&Path) -> anyhow::Result<()> + 'static>;

#[cfg(test)]
thread_local! {
    static SELECTED_RENAME_HOOK: std::cell::RefCell<Option<SelectedRenameHook>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) struct SelectedRenameHookGuard;

#[cfg(test)]
pub(super) fn pause_after_selected_rename_once(
    hook: impl FnOnce(&Path) -> anyhow::Result<()> + 'static,
) -> SelectedRenameHookGuard {
    SELECTED_RENAME_HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(
            slot.is_none(),
            "nested marker publication hooks are not supported"
        );
        *slot = Some(Box::new(hook));
    });
    SelectedRenameHookGuard
}

#[cfg(test)]
impl Drop for SelectedRenameHookGuard {
    fn drop(&mut self) {
        SELECTED_RENAME_HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

pub(super) fn pause_after_selected_rename(root: &Path) -> anyhow::Result<()> {
    #[cfg(test)]
    if let Some(hook) = SELECTED_RENAME_HOOK.with(|slot| slot.borrow_mut().take()) {
        hook(root)?;
    }

    #[cfg(not(test))]
    let _ = root;

    Ok(())
}
