// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs::{self, File};
use std::io;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Component, Path, PathBuf};

use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};
use nix::fcntl::{AtFlags, OFlag, open, openat};
use nix::sys::stat::{FchmodatFlags, Mode, SFlag, fchmodat, fstat, fstatat, mkdirat};
use nix::unistd::geteuid;
use sha2::{Digest as _, Sha256};

use crate::{
    BROKER_DIR, BROKER_RUN_DIR, BROKER_SOCKET, BROKER_SOCKET_ALIAS_DIR_PREFIX,
    UNIX_SOCKET_PATH_LIMIT,
};

#[cfg(target_os = "macos")]
const DEFAULT_SOCKET_ALIAS_ROOT: &str = "/private/tmp";
#[cfg(not(target_os = "macos"))]
const DEFAULT_SOCKET_ALIAS_ROOT: &str = "/tmp";

/// Return a deterministic, securely provisioned alias when `full` exceeds
/// the platform Unix socket path limit.
pub fn short_socket_alias(full: &Path) -> Result<Option<PathBuf>, UsageCoordinationError> {
    short_socket_alias_with_root(full, None)
}

pub(crate) fn socket_path_for_data_dir(
    data_dir: &Path,
    alias_root: Option<&Path>,
) -> Result<PathBuf, UsageCoordinationError> {
    let current_dir = if data_dir.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().map_err(|error| {
            alias_error(format!(
                "unable to resolve relative broker data directory: {error}"
            ))
        })?
    };
    let full = broker_socket_path_for_data_dir(data_dir, &current_dir).map_err(|error| {
        alias_error(format!(
            "unable to resolve broker socket path for data directory {}: {error}",
            data_dir.display()
        ))
    })?;
    Ok(short_socket_alias_with_root(&full, alias_root)?.unwrap_or(full))
}

pub(crate) fn broker_socket_path_for_data_dir(
    data_dir: &Path,
    current_dir: &Path,
) -> io::Result<PathBuf> {
    let absolute = if data_dir.is_absolute() {
        data_dir.to_path_buf()
    } else {
        if !current_dir.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "relative broker data path requires an absolute working directory",
            ));
        }
        current_dir.join(data_dir)
    };
    let mut full = PathBuf::from("/");
    let mut has_missing_tail = false;
    for component in absolute.components() {
        match component {
            Component::RootDir => {}
            Component::CurDir if !has_missing_tail => {}
            Component::ParentDir if !has_missing_tail => {
                let _popped = full.pop();
            }
            Component::Normal(name) if has_missing_tail => full.push(name),
            Component::Normal(name) => {
                let candidate = full.join(name);
                match fs::canonicalize(&candidate) {
                    Ok(canonical) => full = canonical,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        match fs::symlink_metadata(&candidate) {
                            Ok(_) => return Err(error),
                            Err(metadata_error)
                                if metadata_error.kind() == io::ErrorKind::NotFound =>
                            {
                                full.push(name);
                                has_missing_tail = true;
                            }
                            Err(metadata_error) => return Err(metadata_error),
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
            Component::CurDir | Component::ParentDir => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "broker data path traverses a missing component",
                ));
            }
            Component::Prefix(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "broker data path has a non-Unix prefix",
                ));
            }
        }
    }
    full.push(BROKER_DIR);
    full.push(BROKER_RUN_DIR);
    full.push(BROKER_SOCKET);
    Ok(full)
}

pub(crate) fn socket_alias_name(full: &Path) -> String {
    let digest = Sha256::digest(full.as_os_str().as_encoded_bytes());
    let mut name = String::with_capacity(3 + 64 + 5);
    name.push_str("jk-");
    for byte in digest {
        use std::fmt::Write as _;
        let _written = write!(name, "{byte:02x}");
    }
    name.push_str(".sock");
    name
}

pub(crate) fn default_socket_alias_root() -> &'static Path {
    Path::new(DEFAULT_SOCKET_ALIAS_ROOT)
}

pub(crate) fn short_socket_alias_with_root(
    full: &Path,
    alias_root: Option<&Path>,
) -> Result<Option<PathBuf>, UsageCoordinationError> {
    if !full.is_absolute() {
        return Err(alias_error(
            "broker socket path must be absolute before aliasing".to_owned(),
        ));
    }
    let full_bytes = full.as_os_str().as_encoded_bytes();
    if full_bytes.contains(&0) {
        return Err(alias_error(
            "broker socket path contains a NUL byte".to_owned(),
        ));
    }
    if full_bytes.len() < UNIX_SOCKET_PATH_LIMIT {
        return Ok(None);
    }

    let requested_root = alias_root.unwrap_or(default_socket_alias_root());
    if !requested_root.is_absolute() {
        return Err(alias_error(format!(
            "socket alias root {} must be absolute",
            requested_root.display()
        )));
    }
    let root = fs::canonicalize(requested_root).map_err(|error| {
        alias_error(format!(
            "cannot resolve socket alias root {}: {error}",
            requested_root.display()
        ))
    })?;
    let child_name = format!("{BROKER_SOCKET_ALIAS_DIR_PREFIX}{}", geteuid().as_raw());
    let alias = root.join(&child_name).join(socket_alias_name(full));
    if alias.as_os_str().as_encoded_bytes().len() >= UNIX_SOCKET_PATH_LIMIT {
        return Err(alias_error(format!(
            "secure alias path exceeds the {UNIX_SOCKET_PATH_LIMIT}-byte Unix socket limit"
        )));
    }

    let root_directory = open_trusted_socket_alias_root(&root)?;
    let _child_directory = private_socket_alias_child(&root_directory, &child_name)?;
    Ok(Some(alias))
}

pub(crate) fn open_trusted_socket_alias_root(path: &Path) -> Result<File, UsageCoordinationError> {
    if !path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        })
    {
        return Err(alias_error(format!(
            "resolved socket alias root {} must be absolute and normalized",
            path.display()
        )));
    }

    let root_fd = open(
        "/",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| alias_error(format!("cannot open filesystem root: {error}")))?;
    let mut directory = File::from(root_fd);
    validate_trusted_socket_alias_ancestor(&directory, Path::new("/"))?;
    let mut current_path = PathBuf::from("/");
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        let name = match component {
            Component::RootDir => continue,
            Component::Normal(name) => name,
            _ => {
                return Err(alias_error(format!(
                    "resolved socket alias root {} must be absolute and normalized",
                    path.display()
                )));
            }
        };
        let fd = openat(
            &directory,
            name,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| {
            alias_error(format!(
                "cannot open socket alias root {} without following links: {error}",
                path.display()
            ))
        })?;
        let next = File::from(fd);
        current_path.push(name);
        if components.peek().is_some() {
            validate_trusted_socket_alias_ancestor(&next, &current_path)?;
        }
        directory = next;
    }
    validate_trusted_socket_alias_root(&directory, path)?;
    Ok(directory)
}

pub(crate) fn validate_trusted_socket_alias_ancestor(
    directory: &File,
    path: &Path,
) -> Result<(), UsageCoordinationError> {
    let metadata = directory.metadata().map_err(|error| {
        alias_error(format!(
            "cannot inspect socket alias root ancestor {}: {error}",
            path.display()
        ))
    })?;
    let owner = metadata.uid();
    let mode = metadata.mode() & 0o7777;
    let root_sticky = owner == 0 && mode & 0o1000 != 0;
    if !metadata.is_dir()
        || (owner != 0 && owner != geteuid().as_raw())
        || (mode & 0o022 != 0 && !root_sticky)
    {
        return Err(alias_error(format!(
            "socket alias root ancestor {} is not trusted",
            path.display()
        )));
    }
    Ok(())
}

pub(crate) fn validate_trusted_socket_alias_root(
    directory: &File,
    path: &Path,
) -> Result<(), UsageCoordinationError> {
    let metadata = directory.metadata().map_err(|error| {
        alias_error(format!(
            "cannot inspect socket alias root {}: {error}",
            path.display()
        ))
    })?;
    let mode = metadata.mode() & 0o7777;
    let shared_root = metadata.uid() == 0 && mode == 0o1777;
    let private_root = metadata.uid() == geteuid().as_raw() && mode == 0o700;
    if !metadata.is_dir() || !(shared_root || private_root) {
        return Err(alias_error(format!(
            "socket alias root {} must be root-owned mode 01777 or effective-uid-owned mode 0700",
            path.display()
        )));
    }
    Ok(())
}

pub(crate) fn private_socket_alias_child(
    parent: &File,
    name: &str,
) -> Result<File, UsageCoordinationError> {
    private_socket_alias_child_with_normalizer(parent, name, |parent, name| {
        fchmodat(
            parent,
            name,
            Mode::from_bits_truncate(0o700),
            FchmodatFlags::NoFollowSymlink,
        )
    })
}

fn private_socket_alias_child_with_normalizer(
    parent: &File,
    name: &str,
    mut normalize: impl FnMut(&File, &str) -> nix::Result<()>,
) -> Result<File, UsageCoordinationError> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err(alias_error(
            "per-user alias directory name must be one path component".to_owned(),
        ));
    }
    let created = match mkdirat(parent, name, Mode::from_bits_truncate(0o700)) {
        Ok(()) => true,
        Err(nix::errno::Errno::EEXIST) => false,
        Err(error) => {
            return Err(alias_error(format!(
                "cannot create per-user alias directory {name}: {error}"
            )));
        }
    };

    let created_identity = if created {
        // The parent FD anchors both no-follow checks and the one permitted chmod.
        // This does not defend against a hostile process running as the same UID:
        // fstatat/fchmodat are separate operations, and the root trust model assumes
        // same-UID processes are trusted.
        let created_stat =
            fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW).map_err(|error| {
                alias_error(format!(
                    "cannot inspect newly created per-user alias directory {name}: {error}"
                ))
            })?;
        let identity = validate_created_socket_alias_child_stat(name, &created_stat, false)?;
        if created_stat.st_mode & 0o7777 != 0o700 {
            normalize(parent, name).map_err(|error| {
                alias_error(format!(
                    "cannot normalize newly created per-user alias directory {name} to mode 0700 without following links: {error}"
                ))
            })?;
        }

        let normalized_stat =
            fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW).map_err(|error| {
                alias_error(format!(
                    "cannot re-inspect newly created per-user alias directory {name}: {error}"
                ))
            })?;
        let normalized_identity =
            validate_created_socket_alias_child_stat(name, &normalized_stat, true)?;
        ensure_socket_alias_child_identity(name, identity, normalized_identity)?;
        Some(identity)
    } else {
        None
    };

    let fd = openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| {
        alias_error(format!(
            "cannot open per-user alias directory {name} without following links: {error}"
        ))
    })?;
    let directory = File::from(fd);
    let metadata = directory.metadata().map_err(|error| {
        alias_error(format!(
            "cannot inspect per-user alias directory {name}: {error}"
        ))
    })?;
    if !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o7777 != 0o700
    {
        return Err(alias_error(format!(
            "per-user alias directory {name} must be owned by effective uid {} with mode 0700",
            geteuid().as_raw()
        )));
    }
    if let Some(identity) = created_identity {
        let opened_stat = fstat(&directory).map_err(|error| {
            alias_error(format!(
                "cannot inspect opened per-user alias directory {name} identity: {error}"
            ))
        })?;
        ensure_socket_alias_child_identity(
            name,
            identity,
            (opened_stat.st_dev, opened_stat.st_ino),
        )?;
    }
    Ok(directory)
}

type SocketAliasChildIdentity = (nix::libc::dev_t, nix::libc::ino_t);

fn validate_created_socket_alias_child_stat(
    name: &str,
    stat: &nix::sys::stat::FileStat,
    require_private_mode: bool,
) -> Result<SocketAliasChildIdentity, UsageCoordinationError> {
    let mode = stat.st_mode & 0o7777;
    let file_type = stat.st_mode & SFlag::S_IFMT.bits();
    if file_type != SFlag::S_IFDIR.bits()
        || stat.st_uid != geteuid().as_raw()
        || mode & !0o700 != 0
        || (require_private_mode && mode != 0o700)
    {
        return Err(alias_error(format!(
            "newly created per-user alias directory {name} must remain an effective-uid-owned directory with permissions no broader than mode 0700{}",
            if require_private_mode {
                " and exact mode 0700"
            } else {
                " and no special bits"
            }
        )));
    }
    Ok((stat.st_dev, stat.st_ino))
}

fn ensure_socket_alias_child_identity(
    name: &str,
    expected: SocketAliasChildIdentity,
    actual: SocketAliasChildIdentity,
) -> Result<(), UsageCoordinationError> {
    if expected != actual {
        return Err(alias_error(format!(
            "newly created per-user alias directory {name} changed device/inode during validation"
        )));
    }
    Ok(())
}

fn alias_error(message: String) -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unavailable,
        message: format!("cannot provision usage broker socket alias: {message}"),
    }
}

#[cfg(test)]
mod tests;
