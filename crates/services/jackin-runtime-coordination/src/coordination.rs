// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Permanent coordination inodes live outside every prunable runtime root.

use jackin_core::JackinPaths;
use sha2::{Digest as _, Sha256};
use std::io;
use std::path::{Component, Path, PathBuf};

const DIRECTORY: &str = ".jackin-coordination";

/// Resolve a pathname through its nearest existing ancestor. Never invent an
/// identity after permission/I/O errors or ambiguous missing symlink/`..` tails.
fn resolve(path: &Path) -> io::Result<PathBuf> {
    resolve_with(path, |ancestor| std::fs::canonicalize(ancestor))
}

fn resolve_with(
    path: &Path,
    mut canonicalize: impl FnMut(&Path) -> io::Result<PathBuf>,
) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut ancestor = absolute.as_path();
    let mut tail = Vec::new();
    loop {
        match canonicalize(ancestor) {
            Ok(mut resolved) => {
                for component in tail.into_iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // A dangling symlink has an inode but cannot be canonicalized.
                match std::fs::symlink_metadata(ancestor) {
                    Ok(metadata) if !metadata.file_type().is_symlink() => {
                        // Creation may have completed after the failed syscall.
                        // Retry once for this real inode; further errors remain errors.
                        let mut resolved = canonicalize(ancestor)?;
                        for component in tail.into_iter().rev() {
                            resolved.push(component);
                        }
                        return Ok(resolved);
                    }
                    Ok(_) => return Err(error),
                    Err(missing) if missing.kind() == io::ErrorKind::NotFound => {}
                    Err(other) => return Err(other),
                }
                if ancestor
                    .components()
                    .any(|part| part == Component::ParentDir)
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "cannot resolve missing coordination path containing `..`",
                    ));
                }
                tail.push(
                    ancestor
                        .file_name()
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "coordination path has no ancestor",
                            )
                        })?
                        .to_owned(),
                );
                ancestor = ancestor.parent().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "coordination path has no parent",
                    )
                })?;
            }
            Err(error) => return Err(error),
        }
    }
}

fn namespace(paths: &JackinPaths) -> io::Result<PathBuf> {
    let namespace = resolve(&paths.home_dir)?.join(DIRECTORY);
    match std::fs::symlink_metadata(&namespace) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "coordination namespace must be a real directory",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(namespace)
}

fn reject_overlap(namespace: &Path, pruned: &Path) -> io::Result<()> {
    let pruned = resolve(pruned)?;
    if namespace.starts_with(&pruned) || pruned.starts_with(namespace) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "prune root overlaps the permanent coordination namespace",
        ));
    }
    Ok(())
}

/// No creation: safe for preflight validation and fixture-owned path derivation.
pub fn root(paths: &JackinPaths) -> io::Result<PathBuf> {
    let namespace = namespace(paths)?;
    for pruned in [
        &paths.jackin_home,
        &paths.data_dir,
        &paths.roles_dir,
        &paths.cache_dir,
    ] {
        reject_overlap(&namespace, pruned)?;
    }
    Ok(namespace)
}

/// Check each actual deletion boundary, including non-default layouts.
pub fn ensure_prunable(paths: &JackinPaths, pruned: &Path) -> io::Result<()> {
    reject_overlap(&namespace(paths)?, pruned)
}

/// Async callers dispatch the whole filesystem preflight off runtime threads.
pub async fn ensure_prunable_async(paths: &JackinPaths, pruned: &Path) -> io::Result<()> {
    let paths = paths.clone();
    let pruned = pruned.to_owned();
    jackin_telemetry::spawn::joined_blocking(move || ensure_prunable(&paths, &pruned))
        .await
        .map_err(io::Error::other)?
}

/// Preserve per-data-root universe authority across data deletion/recreation.
/// The key hashes the resolved absolute pathname, never its transient inode.
pub fn universe_dir(paths: &JackinPaths) -> io::Result<PathBuf> {
    let data = resolve(&paths.data_dir)?;
    let mut hash = Sha256::new();
    hash.update(b"jackin-universe-path-v1\0");
    hash.update(data.as_os_str().as_encoded_bytes());
    Ok(root(paths)?
        .join("universes")
        .join(hex::encode(hash.finalize())))
}

/// File ownership is only flock ownership. Neither contention nor Drop unlinks.
pub fn open_lock(paths: &JackinPaths, key: &str) -> io::Result<std::fs::File> {
    open_in_namespace(&root(paths)?, key)
}

fn validate_key(key: &str) -> io::Result<()> {
    if key.is_empty()
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        || matches!(key, "." | "..")
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid coordination leaf key",
        ));
    }
    Ok(())
}

/// Reopen an already captured namespace without reselecting its authority.
/// Lock inodes are created once and never truncated or removed.
pub fn open_in_namespace(directory: &Path, key: &str) -> io::Result<std::fs::File> {
    validate_key(key)?;
    let parent = open_directory_in_namespace(directory, true)?;
    open_state_at(&parent, &format!("{key}.lock"), true)
}

/// Pin every namespace path component; existing-only opens create nothing.
pub fn open_directory_in_namespace(directory: &Path, create: bool) -> io::Result<std::fs::File> {
    if !directory.is_absolute()
        || !directory
            .components()
            .any(|part| part == Component::Normal(std::ffi::OsStr::new(DIRECTORY)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid coordination directory",
        ));
    }
    #[cfg(unix)]
    {
        use nix::errno::Errno;
        use nix::fcntl::{OFlag, open, openat};
        use nix::sys::stat::{Mode, mkdirat};
        use std::os::unix::fs::MetadataExt as _;
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
        let mut parent = std::fs::File::from(open(Path::new("/"), flags, Mode::empty())?);
        let components: Vec<_> = directory.components().collect();
        let boundary = components
            .iter()
            .rposition(|part| *part == Component::Normal(std::ffi::OsStr::new(DIRECTORY)))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "missing coordination namespace boundary",
                )
            })?;
        let mut private_namespace = false;
        for (index, component) in components.into_iter().enumerate() {
            let Component::Normal(name) = component else {
                if component == Component::RootDir {
                    continue;
                }
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "coordination path must be normalized",
                ));
            };
            if index == boundary {
                let metadata = parent.metadata()?;
                if metadata.uid() != nix::unistd::geteuid().as_raw() || metadata.mode() & 0o022 != 0
                {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "coordination parent must be owned and not writable by others",
                    ));
                }
                private_namespace = true;
            }
            let fd = match openat(&parent, name, flags, Mode::empty()) {
                Ok(fd) => fd,
                Err(Errno::ENOENT) if create => {
                    match mkdirat(&parent, name, Mode::from_bits_truncate(0o700)) {
                        Ok(()) | Err(Errno::EEXIST) => {}
                        Err(error) => return Err(error.into()),
                    }
                    openat(&parent, name, flags, Mode::empty())?
                }
                Err(error) => return Err(error.into()),
            };
            parent = std::fs::File::from(fd);
            if private_namespace {
                let metadata = parent.metadata()?;
                if metadata.uid() != nix::unistd::geteuid().as_raw() || metadata.mode() & 0o077 != 0
                {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "every coordination directory must be private and owned",
                    ));
                }
            }
        }

        Ok(parent)
    }
    #[cfg(not(unix))]
    {
        let _ = create;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "coordination directories require no-follow Unix access",
        ))
    }
}

/// Literal state key, with existing-only parent. Never truncate before validation.
pub fn open_state_in_namespace(
    directory: &Path,
    key: &str,
    create: bool,
) -> io::Result<std::fs::File> {
    validate_key(key)?;
    let parent = open_directory_in_namespace(directory, false)?;
    open_state_at(&parent, key, create)
}

fn open_state_at(parent: &std::fs::File, key: &str, create: bool) -> io::Result<std::fs::File> {
    validate_key(key)?;
    #[cfg(unix)]
    {
        use nix::errno::Errno;
        use nix::fcntl::{AtFlags, OFlag, openat};
        use nix::sys::stat::{Mode, SFlag, fstat, fstatat};
        use std::os::unix::fs::MetadataExt as _;
        let existing_identity = || match fstatat(parent, key, AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(stat) => {
                if SFlag::from_bits_truncate(stat.st_mode) != SFlag::S_IFREG
                    || stat.st_uid != nix::unistd::geteuid().as_raw()
                    || stat.st_mode & 0o077 != 0
                    || stat.st_nlink != 1
                {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "existing coordination state must be a private owned regular inode",
                    ));
                }
                Ok(Some((stat.st_dev, stat.st_ino)))
            }
            Err(Errno::ENOENT) => Ok(None),
            Err(error) => Err(io::Error::from(error)),
        };
        let mut before = existing_identity()?;
        let flags = OFlag::O_RDWR | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK;
        let mode = Mode::from_bits_truncate(0o600);
        let fd = if create && before.is_none() {
            // Concurrent nonexclusive O_CREAT opens can return ENOENT on macOS.
            // Elect exactly one creator; contenders validate and open its inode.
            match openat(parent, key, flags | OFlag::O_CREAT | OFlag::O_EXCL, mode) {
                Ok(fd) => fd,
                Err(Errno::EEXIST) => {
                    before = Some(existing_identity()?.ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, "coordination inode disappeared")
                    })?);
                    openat(parent, key, flags, Mode::empty())?
                }
                Err(error) => return Err(error.into()),
            }
        } else {
            openat(parent, key, flags, Mode::empty())?
        };
        let file = std::fs::File::from(fd);
        let metadata = file.metadata()?;
        let after = fstat(&file)?;
        if !metadata.is_file()
            || metadata.uid() != nix::unistd::geteuid().as_raw()
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
            || before.is_some_and(|identity| identity != (after.st_dev, after.st_ino))
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "coordination state must be a private owned regular inode",
            ));
        }
        Ok(file)
    }
    #[cfg(not(unix))]
    {
        let _ = (parent, create);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "coordination state requires no-follow Unix access",
        ))
    }
}

#[cfg(test)]
mod tests;
