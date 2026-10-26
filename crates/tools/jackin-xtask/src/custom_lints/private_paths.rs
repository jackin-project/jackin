use std::fs::{self, File};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use jackin_process_directory::CheckedInstallRoot;
use nix::fcntl::{OFlag, open};
use nix::sys::stat::Mode;
use nix::unistd::geteuid;

pub(super) fn normalize_directory(path: &Path) -> Result<File> {
    let created = fs::symlink_metadata(path).context("inspecting new custom lint directory")?;
    ensure!(
        created.is_dir() && created.uid() == geteuid().as_raw(),
        "new custom lint directory has unsafe identity"
    );
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .context("protecting new custom lint directory")?;
    let descriptor = open(
        path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .context("opening new custom lint directory")?;
    let directory = File::from(descriptor);
    let opened = directory.metadata()?;
    ensure!(
        created.dev() == opened.dev() && created.ino() == opened.ino(),
        "new custom lint directory changed while opening"
    );
    CheckedInstallRoot::validate_confidential_directory(&directory)
        .context("validating new custom lint directory ownership, mode, and ACL")?;
    let named = fs::symlink_metadata(path)?;
    ensure!(
        named.is_dir() && named.dev() == opened.dev() && named.ino() == opened.ino(),
        "new custom lint directory changed during validation"
    );
    Ok(directory)
}

pub(super) fn create_symlink(parent: &File, target: &Path, name: &std::ffi::OsStr) -> Result<()> {
    use nix::fcntl::AtFlags;
    use nix::sys::stat::{SFlag, fstatat};
    use nix::unistd::symlinkat;

    symlinkat(target, parent, name).context("creating new custom lint symlink")?;
    let created = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)?;
    ensure!(
        created.st_mode & SFlag::S_IFMT.bits() == SFlag::S_IFLNK.bits()
            && created.st_uid == geteuid().as_raw(),
        "new custom lint symlink has unsafe identity"
    );
    #[cfg(target_os = "macos")]
    nix::sys::stat::fchmodat(
        parent,
        name,
        Mode::from_bits_truncate(0o700),
        nix::sys::stat::FchmodatFlags::NoFollowSymlink,
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
