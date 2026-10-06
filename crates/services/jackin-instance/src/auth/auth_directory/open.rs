// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Descriptor-relative path traversal and open primitives.

use super::run_source_open_hook;

use anyhow::Context;

use nix::errno::Errno;
use nix::fcntl::{AtFlags, OFlag, open, openat};
use nix::sys::stat::{FileStat, Mode, SFlag, fstat, fstatat, mkdirat};
use nix::unistd::{fsync, geteuid};

use std::ffi::{CStr, CString};
use std::fs::File;

#[cfg(target_os = "macos")]
use std::ffi::OsString;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

pub(crate) fn owned_fd(fd: OwnedFd) -> File {
    fd.into()
}

pub(crate) fn nix_error(error: Errno, action: &str) -> anyhow::Error {
    if error == Errno::ELOOP {
        return anyhow::anyhow!("{action}: symlink traversal rejected");
    }
    if error == Errno::ENOTDIR {
        return anyhow::anyhow!("{action}: non-directory or symlink traversal rejected");
    }
    anyhow::Error::new(error).context(action.to_owned())
}

/// Normalize only lexical aliases. Accepted paths become absolute so the
/// lock identity is shared by relative and absolute spellings; symlinks
/// are deliberately not resolved here and are rejected by descriptor
/// traversal instead.
pub(crate) fn normalize_path(path: &Path) -> anyhow::Result<PathBuf> {
    let mut normalized = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().context("finding auth path base directory")?
    };
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(Path::new("/")),
            std::path::Component::CurDir => {}
            std::path::Component::Normal(component) => normalized.push(component),
            std::path::Component::ParentDir => {
                anyhow::bail!("auth path contains parent traversal: {}", path.display())
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let bytes = normalized.as_os_str().as_bytes();
        for alias in [b"/var".as_slice(), b"/tmp".as_slice(), b"/etc".as_slice()] {
            if bytes == alias
                || bytes
                    .strip_prefix(alias)
                    .is_some_and(|rest| rest.starts_with(b"/"))
            {
                let mut normalized = b"/private".to_vec();
                normalized.extend_from_slice(bytes);
                return Ok(PathBuf::from(OsString::from_vec(normalized)));
            }
        }
    }
    Ok(normalized)
}

pub(crate) fn path_key(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let normalized = normalize_path(path)?;
    let mut digest = Sha256::new();
    digest.update(normalized.as_os_str().as_bytes());
    Ok(hex::encode(digest.finalize()))
}

pub(crate) fn cstring_name(path: &Path) -> anyhow::Result<CString> {
    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("auth path has no final component: {}", path.display()))?;
    CString::new(name.as_bytes()).context("auth path contains NUL")
}

pub(crate) fn component_cstring(component: &std::path::Component<'_>) -> anyhow::Result<CString> {
    CString::new(component.as_os_str().as_bytes()).context("auth path contains NUL")
}

pub(crate) fn open_start(absolute: bool) -> anyhow::Result<File> {
    let path = if absolute {
        Path::new("/")
    } else {
        Path::new(".")
    };
    let fd = open(
        path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| nix_error(error, "opening auth traversal root"))?;
    let file = owned_fd(fd);
    validate_directory(&file, "auth traversal root", false)?;
    Ok(file)
}

pub(crate) fn validate_directory(
    file: &File,
    label: &str,
    exact_private: bool,
) -> anyhow::Result<()> {
    let stat = fstat(file).map_err(|error| nix_error(error, label))?;
    anyhow::ensure!(
        SFlag::from_bits_truncate(stat.st_mode).contains(SFlag::S_IFDIR),
        "{label} is not a directory"
    );
    let mode = stat.st_mode & 0o7777;
    if exact_private {
        anyhow::ensure!(
            stat.st_uid == geteuid().as_raw(),
            "{label} is not owned by the current user"
        );
        anyhow::ensure!(
            mode & 0o777 == 0o700,
            "{label} is not mode 0700 (mode {mode:o})"
        );
    } else {
        anyhow::ensure!(
            mode & 0o022 == 0 || mode & 0o1000 != 0,
            "{label} is writable by an untrusted group or other user"
        );
    }
    Ok(())
}

pub(crate) fn validate_owned_stat(
    stat: &FileStat,
    label: &str,
    expected: SFlag,
) -> anyhow::Result<()> {
    let actual = SFlag::from_bits_truncate(stat.st_mode);
    anyhow::ensure!(
        actual.contains(expected),
        "{} has an unexpected {}",
        label,
        if actual.contains(SFlag::S_IFLNK) {
            "symlink"
        } else if actual
            .intersects(SFlag::S_IFIFO | SFlag::S_IFCHR | SFlag::S_IFBLK | SFlag::S_IFSOCK,)
        {
            "special file"
        } else {
            "file type"
        }
    );
    anyhow::ensure!(
        stat.st_uid == geteuid().as_raw(),
        "{label} is not owned by the current user"
    );
    anyhow::ensure!(
        stat.st_mode & 0o022 == 0,
        "{label} is writable by an untrusted group or other user"
    );
    Ok(())
}

pub(crate) fn open_parent(path: &Path, create: bool) -> anyhow::Result<(File, CString, PathBuf)> {
    anyhow::ensure!(
        matches!(
            path.components().next_back(),
            Some(std::path::Component::Normal(_))
        ),
        "auth path must have a normal final component: {}",
        path.display()
    );
    let path = normalize_path(path)?;
    let target = cstring_name(&path)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut directory = open_start(path.is_absolute())?;
    for component in parent.components() {
        let std::path::Component::Normal(_) = component else {
            if matches!(component, std::path::Component::RootDir) {
                continue;
            }
            if matches!(component, std::path::Component::CurDir) {
                continue;
            }
            anyhow::bail!("auth path contains parent traversal: {}", path.display());
        };
        let component = component_cstring(&component)?;
        let next = match openat(
            &directory,
            component.as_c_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => owned_fd(fd),
            Err(Errno::ENOENT) if create => {
                mkdirat(
                    &directory,
                    component.as_c_str(),
                    Mode::from_bits_truncate(0o700),
                )
                .or_else(ignore_eexist)
                .map_err(|error| nix_error(error, "creating auth directory parent"))?;
                let fd = openat(
                    &directory,
                    component.as_c_str(),
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|error| nix_error(error, "opening created auth directory parent"))?;
                owned_fd(fd)
            }
            Err(error) => return Err(nix_error(error, "opening auth directory parent")),
        };
        validate_directory(&next, "auth directory parent", false)?;
        directory = next;
    }
    Ok((directory, target, path))
}

pub(crate) fn entry_stat(directory: &File, name: &CStr) -> anyhow::Result<Option<FileStat>> {
    match fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => Ok(Some(stat)),
        Err(Errno::ENOENT) => Ok(None),
        Err(error) => Err(nix_error(error, "lstat auth directory entry")),
    }
}

pub(crate) fn open_private_file(
    directory: &File,
    name: &CStr,
    flags: OFlag,
    mode: Mode,
    label: &str,
) -> anyhow::Result<File> {
    let fd = openat(
        directory,
        name,
        flags | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        mode,
    )
    .map_err(|error| nix_error(error, label))?;
    let file = owned_fd(fd);
    let stat = fstat(&file).map_err(|error| nix_error(error, label))?;
    validate_owned_stat(&stat, label, SFlag::S_IFREG)?;
    Ok(file)
}

pub(crate) fn ensure_same_source_identity(
    expected: &FileStat,
    actual: &FileStat,
    label: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        expected.st_dev == actual.st_dev
            && expected.st_ino == actual.st_ino
            && expected.st_mode & SFlag::S_IFMT.bits() == actual.st_mode & SFlag::S_IFMT.bits(),
        "{label} was replaced during secure open"
    );
    Ok(())
}

pub(crate) fn open_source_file_with_hook(
    directory: &File,
    name: &CStr,
    expected: &FileStat,
    label: &str,
    invoke_hook: bool,
) -> anyhow::Result<File> {
    if invoke_hook {
        run_source_open_hook();
    }
    let fd = openat(
        directory,
        name,
        OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| nix_error(error, label))?;
    let file = owned_fd(fd);
    let actual = fstat(&file).map_err(|error| nix_error(error, label))?;
    validate_owned_stat(&actual, label, SFlag::S_IFREG)?;
    ensure_same_source_identity(expected, &actual, label)?;
    Ok(file)
}

pub(crate) fn open_source_file(
    directory: &File,
    name: &CStr,
    expected: &FileStat,
    label: &str,
) -> anyhow::Result<File> {
    open_source_file_with_hook(directory, name, expected, label, true)
}

pub(crate) fn open_directory_at(
    directory: &File,
    name: &CStr,
    label: &str,
) -> anyhow::Result<File> {
    let fd = openat(
        directory,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| nix_error(error, label))?;
    let file = owned_fd(fd);
    validate_directory(&file, label, false)?;
    let stat = fstat(&file).map_err(|error| nix_error(error, label))?;
    anyhow::ensure!(
        stat.st_uid == geteuid().as_raw(),
        "{label} is not owned by the current user"
    );
    Ok(file)
}

pub(crate) fn open_source_directory_at_with_hook(
    directory: &File,
    name: &CStr,
    expected: &FileStat,
    label: &str,
    invoke_hook: bool,
) -> anyhow::Result<File> {
    if invoke_hook {
        run_source_open_hook();
    }
    let fd = openat(
        directory,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| nix_error(error, label))?;
    let file = owned_fd(fd);
    validate_directory(&file, label, false)?;
    let actual = fstat(&file).map_err(|error| nix_error(error, label))?;
    anyhow::ensure!(
        actual.st_uid == geteuid().as_raw(),
        "{label} is not owned by the current user"
    );
    ensure_same_source_identity(expected, &actual, label)?;
    Ok(file)
}

pub(crate) fn open_source_directory_at(
    directory: &File,
    name: &CStr,
    expected: &FileStat,
    label: &str,
) -> anyhow::Result<File> {
    open_source_directory_at_with_hook(directory, name, expected, label, true)
}

pub(crate) fn fsync_directory(directory: &File) -> anyhow::Result<()> {
    fsync(directory).map_err(|error| nix_error(error, "syncing auth directory"))
}

pub(crate) fn ignore_eexist(error: Errno) -> Result<(), Errno> {
    if error == Errno::EEXIST {
        Ok(())
    } else {
        Err(error)
    }
}
