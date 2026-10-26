use anyhow::{Context as _, ensure};
use rustix::fs::{
    AtFlags, FlockOperation, Mode, OFlags, RenameFlags, fchmod, flock, fsync, ftruncate, open,
    openat, renameat_with, statat, unlinkat,
};
use std::fs::{File, Metadata};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RootIdentity {
    pub(super) device: u64,
    pub(super) inode: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EntryStat {
    pub(super) device: u64,
    pub(super) inode: u64,
    pub(super) mode: u32,
    pub(super) owner: u32,
    pub(super) links: u64,
    pub(super) size: u64,
}

impl EntryStat {
    pub(super) fn from_stat(stat: rustix::fs::Stat) -> anyhow::Result<Self> {
        Ok(Self {
            device: u64::try_from(i128::from(stat.st_dev))
                .context("invalid marker entry device")?,
            inode: u64::try_from(i128::from(stat.st_ino)).context("invalid marker entry inode")?,
            mode: u32::try_from(i128::from(stat.st_mode)).context("invalid marker entry mode")?,
            owner: u32::try_from(i128::from(stat.st_uid)).context("invalid marker entry owner")?,
            links: u64::try_from(i128::from(stat.st_nlink))
                .context("invalid marker entry link count")?,
            size: u64::try_from(i128::from(stat.st_size)).context("invalid marker entry size")?,
        })
    }

    pub(super) fn is_regular(self) -> bool {
        self.mode & 0o170_000 == 0o100_000
    }

    pub(super) fn permissions(self) -> u32 {
        self.mode & 0o7777
    }
}

fn permission_mode(mode: u32) -> anyhow::Result<Mode> {
    ensure!(
        mode & !0o777 == 0,
        "invalid marker entry permission mode: {mode:o}"
    );

    let mut permissions = Mode::empty();
    for (bit, flag) in [
        (0o400, Mode::RUSR),
        (0o200, Mode::WUSR),
        (0o100, Mode::XUSR),
        (0o040, Mode::RGRP),
        (0o020, Mode::WGRP),
        (0o010, Mode::XGRP),
        (0o004, Mode::ROTH),
        (0o002, Mode::WOTH),
        (0o001, Mode::XOTH),
    ] {
        permissions.set(flag, mode & bit != 0);
    }
    Ok(permissions)
}

impl RootIdentity {
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

pub(super) struct MarkerPairLease {
    root: PathBuf,
    directory: File,
    identity: RootIdentity,
}

impl MarkerPairLease {
    pub(super) fn acquire(root: &Path) -> anyhow::Result<Self> {
        Self::acquire_inner(root, false, FlockOperation::NonBlockingLockExclusive)?
            .ok_or_else(|| anyhow::anyhow!("marker root is absent: {}", root.display()))
    }

    pub(super) fn acquire_optional(root: &Path) -> anyhow::Result<Option<Self>> {
        Self::acquire_inner(root, true, FlockOperation::NonBlockingLockShared)
    }

    fn acquire_inner(
        root: &Path,
        missing_is_false: bool,
        operation: FlockOperation,
    ) -> anyhow::Result<Option<Self>> {
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

        flock(&directory, operation).with_context(|| {
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

    pub(super) fn root_identity(&self) -> RootIdentity {
        self.identity
    }

    pub(super) fn root_path(&self) -> &Path {
        &self.root
    }

    pub(super) fn open_entry(&self, name: &str, flags: OFlags) -> anyhow::Result<File> {
        self.ensure_root_identity()?;
        let descriptor = openat(
            &self.directory,
            name,
            flags | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .with_context(|| format!("cannot open marker root entry {name} without following links"))?;
        let file = File::from(descriptor);
        self.ensure_root_identity()?;
        Ok(file)
    }

    pub(super) fn create_entry(&self, name: &str, mode: u32) -> anyhow::Result<File> {
        self.ensure_root_identity()?;
        let native_mode = permission_mode(mode)?;
        let descriptor = openat(
            &self.directory,
            name,
            OFlags::WRONLY
                | OFlags::CREATE
                | OFlags::EXCL
                | OFlags::NOFOLLOW
                | OFlags::CLOEXEC
                | OFlags::NONBLOCK,
            native_mode,
        )
        .with_context(|| format!("cannot create marker root entry {name} exclusively"))?;
        let file = File::from(descriptor);
        let stat = EntryStat::from_stat(rustix::fs::fstat(&file)?)?;
        if stat.permissions() != mode {
            fchmod(&file, native_mode)
                .with_context(|| format!("cannot set newly created marker entry mode: {name}"))?;
        }
        self.ensure_file_name_matches(name, &file)?;
        Ok(file)
    }

    pub(super) fn ensure_file_name_matches(&self, name: &str, file: &File) -> anyhow::Result<()> {
        self.ensure_root_identity()?;
        let descriptor = EntryStat::from_stat(rustix::fs::fstat(file)?)?;
        let named = self
            .entry_stat(name)?
            .ok_or_else(|| anyhow::anyhow!("marker root entry disappeared: {name}"))?;
        ensure!(
            descriptor == named,
            "marker root entry identity changed while open: {name}"
        );
        self.ensure_root_identity()
    }

    pub(super) fn sync_file(&self, file: &File) -> anyhow::Result<()> {
        self.ensure_root_identity()?;
        fsync(file).context("cannot sync marker transaction file")?;
        self.ensure_root_identity()
    }

    pub(super) fn sync_root(&self) -> anyhow::Result<()> {
        self.ensure_root_identity()?;
        fsync(&self.directory).context("cannot sync marker root directory")?;
        self.ensure_root_identity()
    }

    pub(super) fn truncate_file(&self, file: &File) -> anyhow::Result<()> {
        self.ensure_root_identity()?;
        ftruncate(file, 0).context("cannot truncate transaction-owned marker file")?;
        self.ensure_root_identity()
    }

    pub(super) fn set_file_mode(&self, file: &File, mode: u32) -> anyhow::Result<()> {
        self.ensure_root_identity()?;
        let native_mode = permission_mode(mode)?;
        fchmod(file, native_mode).context("cannot set transaction-owned marker mode")?;
        self.ensure_root_identity()
    }

    pub(super) fn rename_entry(&self, from: &str, to: &str) -> anyhow::Result<()> {
        self.ensure_root_identity()?;
        renameat_with(
            &self.directory,
            from,
            &self.directory,
            to,
            RenameFlags::empty(),
        )
        .with_context(|| format!("cannot rename marker transaction entry {from} to {to}"))?;
        self.ensure_root_identity()
    }

    pub(super) fn unlink_entry(&self, name: &str) -> anyhow::Result<()> {
        self.ensure_root_identity()?;
        unlinkat(&self.directory, name, AtFlags::empty())
            .with_context(|| format!("cannot unlink marker transaction entry {name}"))?;
        self.ensure_root_identity()
    }

    pub(super) fn entry_stat(&self, name: &str) -> anyhow::Result<Option<EntryStat>> {
        self.ensure_root_identity()?;
        let stat = match statat(&self.directory, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot inspect marker root entry {name}"));
            }
        };
        self.ensure_root_identity()?;
        Ok(Some(EntryStat::from_stat(stat)?))
    }

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
