// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs::{DirBuilder, File, symlink_metadata};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::Path;

use super::ConnectionError;

pub(super) fn directory(path: &Path) -> Result<(), ConnectionError> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    check_directory(path)
}

pub(super) fn check_directory(path: &Path) -> Result<(), ConnectionError> {
    let meta = symlink_metadata(path)?;
    if !meta.is_dir()
        || meta.mode() & 0o077 != 0
        || meta.uid() != rustix::process::geteuid().as_raw()
    {
        return Err(ConnectionError::Private);
    }
    Ok(())
}

pub(super) fn file(path: &Path) -> Result<File, ConnectionError> {
    use rustix::fs::{Mode, OFlags};
    let flags =
        OFlags::RDWR | OFlags::CREATE | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
    let fd =
        rustix::fs::open(path, flags, Mode::RUSR | Mode::WUSR).map_err(std::io::Error::from)?;
    let file = File::from(fd);
    check_metadata(&file.metadata()?)?;
    Ok(file)
}

pub(super) fn check_metadata(meta: &std::fs::Metadata) -> Result<(), ConnectionError> {
    if !meta.is_file()
        || meta.mode() & 0o077 != 0
        || meta.nlink() != 1
        || meta.uid() != rustix::process::geteuid().as_raw()
    {
        return Err(ConnectionError::Private);
    }
    Ok(())
}
