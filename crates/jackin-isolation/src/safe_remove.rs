// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Owned, validated, file-descriptor-pinned recursive directory removal.
//!
//! [`std::fs::remove_dir_all`] re-resolves the full path at every level, so a
//! path component swapped for a symlink mid-traversal (parent-directory
//! replacement race) redirects deletion outside the intended tree. These
//! helpers instead pin every level with an `O_NOFOLLOW` file descriptor
//! opened via `openat`: once a directory is pinned, renames above it cannot
//! redirect the deletion below it. Symlinks are never followed — a symlink
//! where a directory is expected is refused loudly rather than unlinked
//! silently, and containment roots bound record-driven paths (a stale or
//! hostile `isolation.json` entry cannot point cleanup at `$HOME`).

use std::ffi::{CStr, CString};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Component, Path};

use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::{OFlag, open, openat};
use nix::sys::stat::{Mode, fstat};
use nix::unistd::{UnlinkatFlags, unlinkat};

fn dir_oflags() -> OFlag {
    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC
}

/// Recursively delete `path`, pinning every level with `O_NOFOLLOW` fds.
///
/// Missing paths are a no-op. A symlink, a non-directory, a path that
/// changes identity during validation, or a top-level directory not owned by
/// the effective UID (unless root) is refused with an error — never silently
/// reinterpreted.
pub fn safe_remove_dir_all(path: &Path) -> std::io::Result<()> {
    let expected = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if expected.file_type().is_symlink() {
        return refuse(format!(
            "refusing to remove {}: path is a symlink",
            path.display()
        ));
    }
    if !expected.is_dir() {
        return refuse(format!(
            "refusing to remove {}: path is not a directory",
            path.display()
        ));
    }
    let Some(name) = path
        .file_name()
        .map(|name| CString::new(name.as_bytes()))
        .transpose()
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "refusing to remove {}: path component is not valid",
                    path.display()
                ),
            )
        })?
    else {
        return refuse(format!(
            "refusing to remove {}: path has no final component",
            path.display()
        ));
    };
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let parent_fd = match parent {
        Some(parent) => match open(parent, dir_oflags(), Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::ENOENT) => return Ok(()),
            Err(error) => return Err(refuse_io(path, error)),
        },
        None => open(Path::new("."), dir_oflags(), Mode::empty())
            .map_err(|error| refuse_io(path, error))?,
    };
    remove_child_dir(parent_fd.as_fd(), &name, path, &expected)
}

/// Recursively delete `path`, requiring it to live under `root`.
///
/// In addition to [`safe_remove_dir_all`]'s guarantees, `path` must be an
/// absolute path strictly below the canonicalized `root`, every component
/// between root and target must resolve without symlinks, and `..` segments
/// are rejected outright. A missing target is a no-op; a missing root is a
/// refusal, since containment cannot be verified without it.
pub fn safe_remove_dir_contained(root: &Path, path: &Path) -> std::io::Result<()> {
    let expected = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if expected.file_type().is_symlink() {
        return refuse(format!(
            "refusing to remove {}: path is a symlink",
            path.display()
        ));
    }
    if !expected.is_dir() {
        return refuse(format!(
            "refusing to remove {}: path is not a directory",
            path.display()
        ));
    }
    if !path.is_absolute() {
        return refuse(format!(
            "refusing to remove {}: contained path must be absolute",
            path.display()
        ));
    }
    let canonical_root = std::fs::canonicalize(root).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!(
                "refusing to remove {}: cannot verify containment under {}: {error}",
                path.display(),
                root.display()
            ),
        )
    })?;
    // Canonicalize only to compute the component suffix below the root; the
    // fd walk below re-validates every component without following symlinks.
    let canonical_path = std::fs::canonicalize(path).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!(
                "refusing to remove {}: cannot resolve path for containment: {error}",
                path.display()
            ),
        )
    })?;
    let suffix = canonical_path.strip_prefix(&canonical_root).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "refusing to remove {}: path escapes containment root {}",
                path.display(),
                root.display()
            ),
        )
    })?;
    let mut segments: Vec<CString> = Vec::new();
    for component in suffix.components() {
        let Component::Normal(name) = component else {
            return refuse(format!(
                "refusing to remove {}: path escapes containment root {}",
                path.display(),
                root.display()
            ));
        };
        segments.push(CString::new(name.as_bytes()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "refusing to remove {}: path component is not valid",
                    path.display()
                ),
            )
        })?);
    }
    if segments.is_empty() {
        return refuse(format!(
            "refusing to remove {}: path is the containment root itself",
            path.display()
        ));
    }
    let mut dir_fd = open(&canonical_root, dir_oflags(), Mode::empty())
        .map_err(|error| refuse_io(&canonical_root, error))?;
    for segment in &segments[..segments.len() - 1] {
        dir_fd = match openat(
            dir_fd.as_fd(),
            segment.as_c_str(),
            dir_oflags(),
            Mode::empty(),
        ) {
            Ok(child) => child,
            Err(Errno::ENOENT) => return Ok(()),
            Err(error) => return Err(refuse_io(path, error)),
        };
    }
    let name = segments.last().map_or_else(
        || {
            refuse(format!(
                "refusing to remove {}: path has no final component",
                path.display()
            ))
        },
        |name| Ok(name.clone()),
    )?;
    remove_child_dir(dir_fd.as_fd(), &name, path, &expected)
}

/// Device id of an `fstat` result as the `u64` that `Metadata::dev`
/// reports. `st_dev` is `i32` on macOS but already `u64` on Linux, so the
/// fallible conversion exists only where the types differ.
#[cfg(target_os = "macos")]
fn dev_id(stat: &nix::sys::stat::FileStat) -> u64 {
    u64::try_from(stat.st_dev).unwrap_or(u64::MAX)
}

/// Device id of an `fstat` result as the `u64` that `Metadata::dev`
/// reports. `st_dev` is already `u64` here; see the macOS variant.
#[cfg(not(target_os = "macos"))]
fn dev_id(stat: &nix::sys::stat::FileStat) -> u64 {
    stat.st_dev
}

/// Delete one child `name` of the pinned parent `dir_fd`, verifying it is
/// still the validated object before recursing.
fn remove_child_dir(
    dir_fd: BorrowedFd<'_>,
    name: &CStr,
    path: &Path,
    expected: &std::fs::Metadata,
) -> std::io::Result<()> {
    let child = match openat(dir_fd, name, dir_oflags(), Mode::empty()) {
        Ok(child) => child,
        Err(Errno::ENOENT) => return Ok(()),
        Err(error) => return Err(refuse_io(path, error)),
    };
    // The fd pins the exact object being deleted: if its identity differs
    // from the pre-validation metadata, the path was swapped underneath us.
    let actual = fstat(child.as_fd()).map_err(|error| refuse_io(path, error))?;
    let actual_dev = dev_id(&actual);
    if actual_dev != expected.dev() || actual.st_ino != expected.ino() {
        return refuse(format!(
            "refusing to remove {}: path changed during removal",
            path.display()
        ));
    }
    if nix::unistd::geteuid().as_raw() != 0 && actual.st_uid != nix::unistd::geteuid().as_raw() {
        return refuse(format!(
            "refusing to remove {}: directory is not owned by the current user",
            path.display()
        ));
    }
    remove_dir_contents(child.as_fd(), path)?;
    unlinkat(dir_fd, name, UnlinkatFlags::RemoveDir).map_err(|error| refuse_io(path, error))?;
    Ok(())
}

/// Delete every entry directly under pinned `dir_fd`, recursing into
/// subdirectories through newly pinned fds. Each entry is classified by an
/// atomic `openat(O_DIRECTORY|O_NOFOLLOW)`: success means directory,
/// `ENOTDIR`/`ELOOP` means unlink-without-descent, anything else aborts
/// loudly so a half-removed tree is never mistaken for a clean one.
fn remove_dir_contents(dir_fd: BorrowedFd<'_>, path: &Path) -> std::io::Result<()> {
    let owned = dir_fd.try_clone_to_owned().map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!("refusing to remove {}: {error}", path.display()),
        )
    })?;
    let names = {
        let mut dir = Dir::from_fd(owned).map_err(|error| refuse_io(path, error))?;
        let mut names = Vec::new();
        for entry in dir.iter() {
            let entry = entry.map_err(|error| refuse_io(path, error))?;
            let name = entry.file_name().to_owned();
            if name.as_bytes() == b"." || name.as_bytes() == b".." {
                continue;
            }
            names.push(name);
        }
        names
    };
    for name in &names {
        match openat(dir_fd, name.as_c_str(), dir_oflags(), Mode::empty()) {
            Ok(child) => {
                remove_dir_contents(child.as_fd(), path)?;
                unlinkat(dir_fd, name.as_c_str(), UnlinkatFlags::RemoveDir)
                    .map_err(|error| refuse_io(path, error))?;
            }
            Err(Errno::ENOTDIR | Errno::ELOOP) => {
                unlinkat(dir_fd, name.as_c_str(), UnlinkatFlags::NoRemoveDir)
                    .map_err(|error| refuse_io(path, error))?;
            }
            Err(Errno::ENOENT) => {}
            Err(error) => return Err(refuse_io(path, error)),
        }
    }
    Ok(())
}

fn refuse<T>(message: String) -> std::io::Result<T> {
    Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        message,
    ))
}

fn refuse_io(path: &Path, error: Errno) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        format!("refusing to remove {}: {error}", path.display()),
    )
}

#[cfg(test)]
mod tests;
