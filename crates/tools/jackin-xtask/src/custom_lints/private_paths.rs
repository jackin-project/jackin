use std::{
    fs::{self, File},
    io,
    os::unix::fs::MetadataExt,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use jackin_process_directory::CheckedInstallRoot;
use nix::{
    fcntl::{AtFlags, OFlag, open, openat},
    sys::stat::{FchmodatFlags, FileStat, Mode, SFlag, fchmod, fchmodat, fstat, fstatat},
    unistd::{geteuid, symlinkat},
};

pub(super) struct PrivateDirectory {
    temp_dir: tempfile::TempDir,
    descriptor: File,
}

impl PrivateDirectory {
    pub(super) fn path(&self) -> &Path {
        self.temp_dir.path()
    }

    pub(super) fn descriptor(&self) -> &File {
        &self.descriptor
    }

    pub(super) fn close(self) -> io::Result<()> {
        let Self {
            temp_dir,
            descriptor,
        } = self;
        drop(descriptor);
        temp_dir.close()
    }
}

pub(super) fn open_directory(path: &Path) -> Result<File> {
    let canonical = fs::canonicalize(path).context("resolving private directory parent")?;
    ensure!(
        canonical == path,
        "private directory parent is not canonical"
    );
    let before = fs::symlink_metadata(path).context("checking private directory parent")?;
    ensure!(
        before.is_dir() && !before.file_type().is_symlink(),
        "private directory parent is not a physical directory"
    );
    let descriptor = open(
        path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .context("opening private directory parent")?;
    let directory = File::from(descriptor);
    let opened = directory
        .metadata()
        .context("checking private directory parent descriptor")?;
    ensure!(
        opened.is_dir() && before.dev() == opened.dev() && before.ino() == opened.ino(),
        "private directory parent changed while opening"
    );
    recheck_path_identity(path, &directory, "private directory parent")?;
    Ok(directory)
}

pub(super) fn normalize_directory(
    parent: &File,
    temp_dir: tempfile::TempDir,
) -> Result<PrivateDirectory> {
    let path = temp_dir.path().to_path_buf();
    let parent_path = path
        .parent()
        .context("private directory has no parent path")?;
    let name = path
        .file_name()
        .context("private directory has no final path component")?;
    recheck_path_identity(parent_path, parent, "private directory parent")?;

    let created = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .context("checking new private directory entry")?;
    ensure_directory_owner(&created, "new private directory")?;
    fchmodat(
        parent,
        name,
        Mode::from_bits_truncate(0o700),
        FchmodatFlags::NoFollowSymlink,
    )
    .context("setting new private directory permissions without following links")?;
    let normalized = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .context("rechecking new private directory after normalization")?;
    ensure_same_directory(
        &created,
        &normalized,
        "new private directory changed during normalization",
    )?;
    ensure!(
        normalized.st_mode & 0o777 == 0o700,
        "new private directory permissions are not 0700"
    );
    let descriptor = openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .context("opening new private directory without following links")?;
    let directory = File::from(descriptor);
    let opened = fstat(&directory).context("checking new private directory descriptor")?;
    ensure_same_directory(
        &normalized,
        &opened,
        "new private directory changed while opening",
    )?;
    fchmod(&directory, Mode::from_bits_truncate(0o700))
        .context("setting new private directory permissions")?;
    CheckedInstallRoot::validate_confidential_directory(&directory)
        .context("validating new private directory ownership, mode, and ACL")?;

    let named = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .context("rechecking new private directory entry")?;
    ensure_same_directory(
        &opened,
        &named,
        "new private directory changed during normalization",
    )?;
    ensure!(
        named.st_mode & 0o777 == 0o700,
        "new private directory permissions are not 0700"
    );
    recheck_path_identity(parent_path, parent, "private directory parent")?;
    recheck_path_identity(&path, &directory, "new private directory")?;

    Ok(PrivateDirectory {
        temp_dir,
        descriptor: directory,
    })
}

pub(super) fn create_symlink(parent: &File, target: &Path, name: &std::ffi::OsStr) -> Result<()> {
    symlinkat(target, parent, name).context("creating new custom lint symlink")?;
    let created = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)?;
    ensure!(
        created.st_mode & SFlag::S_IFMT.bits() == SFlag::S_IFLNK.bits()
            && created.st_uid == geteuid().as_raw(),
        "new custom lint symlink has unsafe identity"
    );
    #[cfg(target_os = "macos")]
    fchmodat(
        parent,
        name,
        Mode::from_bits_truncate(0o700),
        FchmodatFlags::NoFollowSymlink,
    )
    .context("setting new custom lint symlink permissions")?;
    let named = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)?;
    ensure!(
        named.st_dev == created.st_dev
            && named.st_ino == created.st_ino
            && named.st_mode & SFlag::S_IFMT.bits() == SFlag::S_IFLNK.bits()
            && named.st_uid == created.st_uid,
        "new custom lint symlink changed while setting permissions"
    );
    #[cfg(target_os = "macos")]
    ensure!(
        named.st_mode & 0o777 == 0o700,
        "new custom lint symlink permissions were not normalized"
    );
    Ok(())
}

fn ensure_directory_owner(stat: &FileStat, message: &str) -> Result<()> {
    ensure!(
        stat.st_mode & SFlag::S_IFMT.bits() == SFlag::S_IFDIR.bits()
            && stat.st_uid == geteuid().as_raw(),
        "{message} has unsafe identity"
    );
    Ok(())
}

fn ensure_same_directory(left: &FileStat, right: &FileStat, message: &str) -> Result<()> {
    ensure!(
        left.st_dev == right.st_dev
            && left.st_ino == right.st_ino
            && right.st_mode & SFlag::S_IFMT.bits() == SFlag::S_IFDIR.bits()
            && right.st_uid == geteuid().as_raw(),
        "{message}"
    );
    Ok(())
}

fn recheck_path_identity(path: &Path, descriptor: &File, description: &str) -> Result<()> {
    let path_metadata =
        fs::symlink_metadata(path).with_context(|| format!("checking {description} path"))?;
    let descriptor_metadata = descriptor
        .metadata()
        .with_context(|| format!("checking {description} descriptor"))?;
    ensure!(
        path_metadata.is_dir()
            && !path_metadata.file_type().is_symlink()
            && path_metadata.dev() == descriptor_metadata.dev()
            && path_metadata.ino() == descriptor_metadata.ino(),
        "{description} path changed while validating its descriptor"
    );
    Ok(())
}
