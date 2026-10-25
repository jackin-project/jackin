// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::{CStr, CString};
use std::fs::File;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};
use std::time::Instant;

use nix::errno::Errno;
use nix::fcntl::{AtFlags, OFlag, open, openat};
use nix::sys::stat::{FileStat, Mode, SFlag, fstat, fstatat};

use super::{FileIdentity, OmpError, check_deadline, secure_directory_identity};

pub(super) fn open_source_directory(
    path: &Path,
    deadline: Instant,
) -> Result<Option<File>, OmpError> {
    open_source_directory_with_hook(path, deadline, |_| {})
}

pub(super) fn open_source_directory_with_hook(
    path: &Path,
    deadline: Instant,
    mut after_entry: impl FnMut(&Path),
) -> Result<Option<File>, OmpError> {
    let path = match std::fs::canonicalize(path) {
        Ok(resolved) => resolved,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(OmpError::Unavailable),
    };
    let start = if path.is_absolute() { "/" } else { "." };
    let start_fd = open(
        start,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| OmpError::Unavailable)?;
    let mut directory = File::from(start_fd);
    let mut traversed = Path::new(start).to_path_buf();
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        check_deadline(deadline)?;
        let Component::Normal(name) = component else {
            match component {
                Component::RootDir | Component::CurDir => continue,
                Component::ParentDir | Component::Prefix(_) => return Err(OmpError::Unavailable),
                Component::Normal(_) => unreachable!(),
            }
        };
        traversed.push(name);
        let name = CString::new(name.as_bytes()).map_err(|_| OmpError::Unavailable)?;
        let entry = match fstatat(&directory, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(Errno::ENOENT) => return Ok(None),
            Err(_) => return Err(OmpError::Unavailable),
        };
        if !SFlag::from_bits_truncate(entry.st_mode).contains(SFlag::S_IFDIR) {
            return Err(OmpError::Unavailable);
        }
        after_entry(&traversed);
        directory = open_entry(
            &directory,
            name.as_c_str(),
            &entry,
            components.peek().is_none(),
        )?;
    }
    check_deadline(deadline)?;
    secure_directory_identity(&directory, true)?;
    Ok(Some(directory))
}

fn open_entry(
    directory: &File,
    name: &CStr,
    expected: &FileStat,
    final_component: bool,
) -> Result<File, OmpError> {
    let opened = openat(
        directory,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| OmpError::Unavailable)?;
    let next = File::from(opened);
    let actual = fstat(&next).map_err(|_| OmpError::Unavailable)?;

    if !same_object_and_security(expected, &actual)
        || (final_component
            && FileIdentity::from_stat(expected) != FileIdentity::from_stat(&actual))
    {
        return Err(OmpError::Unavailable);
    }
    Ok(next)
}

pub(super) fn same_object_and_security(expected: &FileStat, actual: &FileStat) -> bool {
    expected.st_dev == actual.st_dev
        && expected.st_ino == actual.st_ino
        && expected.st_uid == actual.st_uid
        && expected.st_gid == actual.st_gid
        && expected.st_mode == actual.st_mode
}
