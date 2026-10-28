// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::AsFd,
        unix::fs::{MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use jackin_process_directory::{
    CheckedInstallFile, CheckedInstallRoot, CheckedSourceRoot, InstallFileKind, RunningImage,
};
use nix::{
    errno::Errno,
    fcntl::{AtFlags, OFlag, openat, renameat},
    sys::stat::{FchmodatFlags, Mode, SFlag, fchmod, fchmodat, fstat, fstatat, mkdirat},
    unistd::{UnlinkatFlags, fsync, unlinkat},
};

const INSTALL_ROOT: &str = "jackin-precommit-owner";
const OWNER_FILE: &str = "owner";
const INSTALL_LOCK: &str = "install.lock";

pub(super) struct OwnerInstallation {
    lock: File,
    owner_path: PathBuf,
    parent: CheckedSourceRoot,
    root: File,
    owner: CheckedInstallFile,
}

impl OwnerInstallation {
    pub(super) fn owner_path(&self) -> &Path {
        &self.owner_path
    }

    pub(super) fn recheck(&self) -> Result<()> {
        self.parent
            .recheck()
            .context("rechecking installed owner Git directory")?;
        let named = fstatat(&self.parent, INSTALL_ROOT, AtFlags::AT_SYMLINK_NOFOLLOW)
            .context("rechecking installed owner directory name")?;
        let held = fstat(&self.root)?;
        ensure!(
            named.st_dev == held.st_dev && named.st_ino == held.st_ino,
            "installed owner directory changed before hook activation"
        );
        ensure_file_identity(&self.root, INSTALL_LOCK, &self.lock, 0o600, 0)?;
        self.owner
            .recheck()
            .context("rechecking installed hook owner")
    }
}

pub(super) fn read_current_owner(
    path: &Path,
    running_image: &RunningImage,
    limit: u64,
) -> Result<Vec<u8>> {
    let before = fs::symlink_metadata(path).context("checking running owner executable")?;
    ensure!(
        before.is_file()
            && !before.file_type().is_symlink()
            && before.uid() == nix::unistd::geteuid().as_raw()
            && before.nlink() == 1
            && before.len() <= limit,
        "running owner executable has unsafe custody or exceeds its bound"
    );
    let parent = CheckedSourceRoot::from_absolute_directory(
        path.parent()
            .context("running owner executable has no parent")?,
    )
    .context("validating running owner executable directory")?;
    let checked = parent
        .open_file(Path::new(
            path.file_name()
                .context("running owner executable has no leaf")?,
        ))
        .context("validating running owner executable permissions and ACL")?;
    let mut file = checked.file().try_clone()?;
    let opened = file.metadata()?;
    ensure!(
        opened.dev() == before.dev() && opened.ino() == before.ino() && opened.nlink() == 1,
        "running owner executable changed while opening"
    );
    running_image
        .verify_file_identity(&file)
        .context("verifying running owner executable image")?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "running owner executable exceeds its bound"
    );
    let after = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    ensure!(
        after.dev() == opened.dev()
            && after.ino() == opened.ino()
            && after.len() == opened.len()
            && named.dev() == opened.dev()
            && named.ino() == opened.ino()
            && named.len() == opened.len(),
        "running owner executable changed while reading"
    );
    checked
        .recheck()
        .context("rechecking running owner executable permissions and ACL")?;
    Ok(bytes)
}

pub(super) fn install_owner(
    git_dir: &Path,
    owner: &[u8],
    signals: &super::config::InstallSignals,
) -> Result<OwnerInstallation> {
    signals.check()?;
    let parent = CheckedSourceRoot::from_absolute_directory(git_dir)
        .context("validating worktree Git directory")?;
    let parent_fd = File::from(parent.as_fd().try_clone_to_owned()?);
    parent
        .recheck()
        .context("rechecking worktree Git directory")?;
    let created = match mkdirat(&parent_fd, INSTALL_ROOT, Mode::from_bits_truncate(0o700)) {
        Ok(()) => {
            let created = fstatat(&parent_fd, INSTALL_ROOT, AtFlags::AT_SYMLINK_NOFOLLOW)
                .context("checking newly created worktree hook owner directory")?;
            ensure!(
                created.st_mode & SFlag::S_IFMT.bits() == SFlag::S_IFDIR.bits()
                    && created.st_uid == nix::unistd::geteuid().as_raw(),
                "new worktree hook owner directory has unsafe custody"
            );
            fchmodat(
                &parent_fd,
                INSTALL_ROOT,
                Mode::from_bits_truncate(0o700),
                FchmodatFlags::NoFollowSymlink,
            )
            .context("setting newly created worktree owner directory permissions")?;
            Some(created)
        }
        Err(Errno::EEXIST) => None,
        Err(error) => return Err(error).context("creating worktree hook owner directory"),
    };
    let root_fd = openat(
        &parent_fd,
        INSTALL_ROOT,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .context("opening private worktree hook owner directory")?;
    let root = File::from(root_fd);
    let checked_root = CheckedInstallRoot::from_pinned_directory(root.try_clone()?)
        .context("validating private worktree hook owner directory")?;
    let named_root = fstatat(&parent_fd, INSTALL_ROOT, AtFlags::AT_SYMLINK_NOFOLLOW)
        .context("checking private worktree hook owner directory name")?;
    let opened_root = root.metadata()?;
    let opened_root_stat = fstat(&root)?;
    if let Some(created) = &created {
        ensure!(
            created.st_dev == opened_root_stat.st_dev && created.st_ino == opened_root_stat.st_ino,
            "new worktree hook owner directory changed before opening"
        );
    }
    ensure!(
        named_root.st_dev == opened_root_stat.st_dev
            && named_root.st_ino == opened_root.ino()
            && named_root.st_uid == opened_root.uid()
            && opened_root.permissions().mode() & 0o777 == 0o700,
        "worktree hook owner directory changed while opening"
    );
    if created.is_some() {
        fsync(&parent_fd).context("syncing worktree Git directory after owner-root creation")?;
        fsync(&root).context("syncing private worktree owner directory")?;
    }

    let lock = match openat(
        &root,
        INSTALL_LOCK,
        OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    ) {
        Ok(lock) => {
            let lock = File::from(lock);
            fchmod(&lock, Mode::from_bits_truncate(0o600))
                .context("setting newly created worktree installer lock permissions")?;
            lock
        }
        Err(Errno::EEXIST) => File::from(
            openat(
                &root,
                INSTALL_LOCK,
                OFlag::O_RDWR | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .context("opening existing persistent worktree hook installer lock")?,
        ),
        Err(error) => {
            return Err(error).context("creating persistent worktree hook installer lock");
        }
    };
    ensure_file_identity(&root, INSTALL_LOCK, &lock, 0o600, 0)?;
    checked_root
        .open_file(Path::new(INSTALL_LOCK), InstallFileKind::Receipt)
        .context("checking native ACLs on installer lock")?;
    lock.lock()
        .context("locking worktree hook owner installation")?;

    if let Some(installed_owner) = matching_existing_owner(&checked_root, owner)
        .context("checking stable owner while holding installer lock")?
    {
        let installation = OwnerInstallation {
            lock,
            owner_path: git_dir.join(INSTALL_ROOT).join(OWNER_FILE),
            parent,
            root,
            owner: installed_owner,
        };
        installation.recheck()?;
        return Ok(installation);
    }

    let temporary = format!(".owner.{}", uuid::Uuid::new_v4());
    let result = publish_owner_entry(&root, &checked_root, &temporary, owner, signals)
        .context("publishing stable owner while holding installer lock");
    let installed_owner = match result {
        Ok(installed_owner) => installed_owner,
        Err(error) => {
            return match remove_if_same(&root, &temporary) {
                Ok(()) => Err(error),
                Err(cleanup) => {
                    Err(error.context(format!("temporary owner cleanup also failed: {cleanup:#}")))
                }
            };
        }
    };
    let installation = OwnerInstallation {
        lock,
        owner_path: git_dir.join(INSTALL_ROOT).join(OWNER_FILE),
        parent,
        root,
        owner: installed_owner,
    };
    installation.recheck()?;
    Ok(installation)
}

fn matching_existing_owner(
    root: &CheckedInstallRoot,
    expected: &[u8],
) -> Result<Option<CheckedInstallFile>> {
    let existing = match root.open_file(Path::new(OWNER_FILE), InstallFileKind::SealedCopy) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("checking existing stable hook owner"),
    };
    let metadata = existing.file().metadata()?;
    ensure!(
        metadata.len() <= 256 * 1024 * 1024,
        "existing hook owner exceeds its bound"
    );
    let mut actual = Vec::new();
    existing
        .file()
        .take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut actual)?;
    ensure!(
        actual.len() as u64 == metadata.len(),
        "existing hook owner changed while reading"
    );
    existing
        .recheck()
        .context("rechecking existing stable hook owner")?;
    Ok((actual == expected).then_some(existing))
}

fn publish_owner_entry(
    root: &File,
    checked_root: &CheckedInstallRoot,
    temporary: &str,
    owner: &[u8],
    signals: &super::config::InstallSignals,
) -> Result<CheckedInstallFile> {
    signals.check()?;
    let descriptor = openat(
        root,
        temporary,
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .context("creating staged worktree hook owner")?;
    let mut file = File::from(descriptor);
    (|| {
        file.write_all(owner)?;
        signals.check()?;
        fchmod(&file, Mode::from_bits_truncate(0o500))?;
        file.sync_all().context("syncing staged hook owner")?;
        signals.check()?;
        ensure_file_identity(root, temporary, &file, 0o500, owner.len() as u64)
            .context("validating staged worktree hook owner identity")?;
        let checked = checked_root
            .open_file(Path::new(temporary), InstallFileKind::SealedCopy)
            .context("validating staged hook owner")?;
        checked.recheck().context("rechecking staged hook owner")?;
        signals.check()?;
        renameat(root, temporary, root, OWNER_FILE)
            .context("atomically replacing stable worktree hook owner")?;
        ensure_file_identity(root, OWNER_FILE, &file, 0o500, owner.len() as u64)
            .context("checking published worktree hook owner identity")?;
        let installed = checked_root
            .open_file(Path::new(OWNER_FILE), InstallFileKind::SealedCopy)
            .context("validating published stable hook owner")?;
        installed
            .recheck()
            .context("rechecking published stable hook owner")?;
        fsync(root).context("syncing worktree hook owner directory")?;
        Ok(installed)
    })()
}

fn ensure_file_identity(
    parent: &File,
    name: &str,
    file: &File,
    mode: u32,
    length: u64,
) -> Result<()> {
    let opened = file.metadata()?;
    let opened_stat = fstat(file)?;
    let named = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .with_context(|| format!("checking private hook owner entry {name}"))?;
    ensure!(
        opened.is_file()
            && opened.uid() == nix::unistd::geteuid().as_raw()
            && opened.nlink() == 1
            && opened.permissions().mode() & 0o777 == mode
            && opened.len() == length
            && named.st_dev == opened_stat.st_dev
            && named.st_ino == opened.ino()
            && named.st_uid == opened.uid(),
        "private hook owner file has unsafe or changed custody"
    );
    Ok(())
}

fn remove_if_same(parent: &File, name: &str) -> Result<()> {
    let descriptor = openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    );
    let file = match descriptor {
        Ok(file) => File::from(file),
        Err(Errno::ENOENT) => return Ok(()),
        Err(error) => return Err(error).context("opening temporary owner for cleanup"),
    };
    let opened = file.metadata()?;
    let opened_stat = fstat(&file)?;
    let named = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .with_context(|| format!("checking temporary owner entry {name} before cleanup"))?;
    if named.st_dev == opened_stat.st_dev && named.st_ino == opened.ino() {
        unlinkat(parent, name, UnlinkatFlags::NoRemoveDir)?;
        fsync(parent)?;
    }
    Ok(())
}
