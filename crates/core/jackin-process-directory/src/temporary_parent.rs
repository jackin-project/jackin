// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use rustix::fs::{AtFlags, FileType, Mode, OFlags, Stat, fstat, open, openat, statat};
use rustix::process::{RawUid, geteuid};
use std::fs::File;
use std::io;
use std::path::{Component, Path};

const DIRECTORY_OPEN_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK)
    .union(OFlags::CLOEXEC);
const NOFOLLOW_STAT_FLAGS: AtFlags = AtFlags::SYMLINK_NOFOLLOW;

pub fn validate_temporary_parent(path: &Path) -> io::Result<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let canonical = std::fs::canonicalize(absolute)?;

    let mut components = canonical.components();
    if components.next() != Some(Component::RootDir) {
        return Err(invalid_parent(
            "temporary parent must begin at the filesystem root",
        ));
    }

    let components = components.collect::<Vec<_>>();
    let mut directory = File::from(open("/", DIRECTORY_OPEN_FLAGS, Mode::empty())?);
    validate_directory(&directory, components.is_empty())?;
    let component_count = components.len();
    for (index, component) in components.into_iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(invalid_parent(
                "temporary parent contains a non-physical path component",
            ));
        };
        let before = statat(&directory, name, NOFOLLOW_STAT_FLAGS)?;
        validate_stat(&before)?;
        let opened = File::from(openat(
            &directory,
            name,
            DIRECTORY_OPEN_FLAGS,
            Mode::empty(),
        )?);
        let after = fstat(&opened)?;
        validate_stat(&after)?;
        if !same_identity(&before, &after) {
            return Err(permission_denied(
                "temporary parent changed while opening a path component",
            ));
        }
        super::secure_install::ensure_temporary_parent_acl(
            &opened,
            after.st_uid,
            index + 1 == component_count,
        )?;
        directory = opened;
    }
    Ok(())
}

fn validate_directory(directory: &File, selected_parent: bool) -> io::Result<()> {
    let metadata = fstat(directory)?;
    validate_stat(&metadata)?;
    super::secure_install::ensure_temporary_parent_acl(directory, metadata.st_uid, selected_parent)
}

fn validate_stat(metadata: &Stat) -> io::Result<()> {
    validate_temporary_parent_facts(
        FileType::from_raw_mode(metadata.st_mode),
        metadata.st_uid,
        u64::from(metadata.st_mode & 0o7777),
        geteuid().as_raw(),
    )
}

fn validate_temporary_parent_facts(
    file_type: FileType,
    owner: RawUid,
    mode: u64,
    effective_uid: RawUid,
) -> io::Result<()> {
    if file_type != FileType::Directory {
        return Err(permission_denied(
            "temporary parent path component is not a directory",
        ));
    }
    if owner != 0 && owner != effective_uid {
        return Err(permission_denied(
            "temporary parent path component has foreign ownership",
        ));
    }
    if mode & 0o022 != 0 && !(mode == 0o1777 && (owner == 0 || owner == effective_uid)) {
        return Err(permission_denied(
            "temporary parent path component is writable by group or other",
        ));
    }
    Ok(())
}

fn same_identity(left: &Stat, right: &Stat) -> bool {
    left.st_dev == right.st_dev
        && left.st_ino == right.st_ino
        && FileType::from_raw_mode(left.st_mode) == FileType::from_raw_mode(right.st_mode)
}

fn invalid_parent(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn permission_denied(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
