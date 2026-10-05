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
pub(crate) fn root(paths: &JackinPaths) -> io::Result<PathBuf> {
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
pub(crate) fn ensure_prunable(paths: &JackinPaths, pruned: &Path) -> io::Result<()> {
    reject_overlap(&namespace(paths)?, pruned)
}

/// Async callers dispatch the whole filesystem preflight off runtime threads.
pub(crate) async fn ensure_prunable_async(paths: &JackinPaths, pruned: &Path) -> io::Result<()> {
    let paths = paths.clone();
    let pruned = pruned.to_owned();
    jackin_telemetry::spawn::joined_blocking(move || ensure_prunable(&paths, &pruned))
        .await
        .map_err(io::Error::other)?
}

/// Preserve per-data-root universe authority across data deletion/recreation.
/// The key hashes the resolved absolute pathname, never its transient inode.
pub(crate) fn universe_dir(paths: &JackinPaths) -> io::Result<PathBuf> {
    let data = resolve(&paths.data_dir)?;
    let mut hash = Sha256::new();
    hash.update(b"jackin-universe-path-v1\0");
    hash.update(data.as_os_str().as_encoded_bytes());
    Ok(root(paths)?
        .join("universes")
        .join(hex::encode(hash.finalize())))
}

/// File ownership is only flock ownership. Neither contention nor Drop unlinks.
pub(crate) fn open_lock(paths: &JackinPaths, key: &str) -> io::Result<std::fs::File> {
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
pub(crate) fn open_in_namespace(directory: &Path, key: &str) -> io::Result<std::fs::File> {
    validate_key(key)?;
    let parent = open_directory_in_namespace(directory, true)?;
    open_state_at(&parent, &format!("{key}.lock"), true)
}

/// Pin every namespace path component; existing-only opens create nothing.
pub(crate) fn open_directory_in_namespace(
    directory: &Path,
    create: bool,
) -> io::Result<std::fs::File> {
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
pub(crate) fn open_state_in_namespace(
    directory: &Path,
    key: &str,
    create: bool,
) -> io::Result<std::fs::File> {
    validate_key(key)?;
    let parent = open_directory_in_namespace(directory, false)?;
    open_state_at(&parent, key, create)
}

/// Open a validated state leaf relative to an already pinned namespace
/// descriptor. Callers that also remove the entry can keep validation and
/// unlinking bound to the same directory inode.
pub(crate) fn open_state_at(
    parent: &std::fs::File,
    key: &str,
    create: bool,
) -> io::Result<std::fs::File> {
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt as _, symlink};

    #[expect(
        clippy::unwrap_used,
        reason = "filesystem race fixture must fail immediately if the expected syscall boundary is absent"
    )]
    fn canonicalize_with_appearing_directory(
        ancestor: &Path,
        raced: &Path,
        retry_calls: &mut usize,
    ) -> io::Result<PathBuf> {
        let result = std::fs::canonicalize(ancestor);
        if ancestor == raced {
            *retry_calls += 1;
            if *retry_calls == 1 {
                assert_eq!(result.as_ref().unwrap_err().kind(), io::ErrorKind::NotFound);
                // Actual filesystem creation at the vulnerable syscall boundary.
                std::fs::create_dir(raced).unwrap();
            }
        }
        result
    }

    fn open_after_barrier(
        barrier: &std::sync::Barrier,
        directory: &Path,
        key: &str,
    ) -> io::Result<std::fs::File> {
        barrier.wait();
        open_in_namespace(directory, key)
    }

    #[test]
    fn resolution_accepts_real_directory_created_after_canonicalization_miss() {
        for missing_tail in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let raced = temp.path().join("created-between-syscalls");
            let selected = if missing_tail {
                raced.join("still-missing")
            } else {
                raced.clone()
            };
            let mut retry_calls = 0;
            let resolved = resolve_with(&selected, |ancestor| {
                canonicalize_with_appearing_directory(ancestor, &raced, &mut retry_calls)
            })
            .unwrap();
            let expected = std::fs::canonicalize(&raced).unwrap();
            assert_eq!(
                resolved,
                if missing_tail {
                    expected.join("still-missing")
                } else {
                    expected
                }
            );
            assert_eq!(
                retry_calls, 2,
                "one bounded retry for the appeared real inode"
            );
            assert!(!raced.join("still-missing").exists());
        }
    }

    #[test]
    fn resolution_rejects_dangling_symlink_without_retry_or_creation() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("absent-target");
        let link = temp.path().join("dangling-link");
        symlink(&target, &link).unwrap();
        let mut calls = 0;
        let error = resolve_with(&link, |ancestor| {
            calls += 1;
            std::fs::canonicalize(ancestor)
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(calls, 1);
        assert!(!target.exists());
        assert!(
            std::fs::symlink_metadata(link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn concurrent_first_lock_opens_all_share_one_inode() {
        use std::sync::{Arc, Barrier};
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let directory = universe_dir(&paths).unwrap();
        drop(open_directory_in_namespace(&directory, true).unwrap());
        for round in 0..40 {
            let barrier = Arc::new(Barrier::new(8));
            let key = format!("concurrent-first-{round}");
            let files = std::thread::scope(|scope| {
                let workers: Vec<_> = (0..8)
                    .map(|_| {
                        let barrier = Arc::clone(&barrier);
                        let directory = &directory;
                        let key = &key;
                        scope.spawn(move || open_after_barrier(&barrier, directory, key))
                    })
                    .collect();
                workers
                    .into_iter()
                    .map(|worker| worker.join().unwrap().unwrap())
                    .collect::<Vec<_>>()
            });
            let first = files[0].metadata().unwrap();
            for file in &files {
                let metadata = file.metadata().unwrap();
                assert_eq!((metadata.dev(), metadata.ino()), (first.dev(), first.ino()));
            }
            let pathname = directory.join(format!("{key}.lock"));
            let metadata = std::fs::symlink_metadata(pathname).unwrap();
            assert_eq!((metadata.dev(), metadata.ino()), (first.dev(), first.ino()));
        }
    }

    #[test]
    fn existing_only_namespace_and_state_opens_create_nothing_and_never_truncate() {
        use std::io::{Read as _, Write as _};
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let directory = universe_dir(&paths).unwrap();
        assert_eq!(
            open_directory_in_namespace(&directory, false)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            open_state_in_namespace(&directory, "generation", true)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(
            !paths.home_dir.exists(),
            "existing-only parent open must not create fixture state"
        );
        drop(open_in_namespace(&directory, "universe-lock").unwrap());
        let mut file = open_state_in_namespace(&directory, "generation", true).unwrap();
        file.write_all(b"retained generation").unwrap();
        let first = file.metadata().unwrap();
        drop(file);
        let mut reopened = open_state_in_namespace(&directory, "generation", false).unwrap();
        let second = reopened.metadata().unwrap();
        let mut contents = String::new();
        reopened.read_to_string(&mut contents).unwrap();
        assert_eq!(
            contents, "retained generation",
            "opening validated state must not truncate it"
        );
        assert_eq!((first.dev(), first.ino()), (second.dev(), second.ino()));
        assert_eq!(
            open_state_in_namespace(&directory, "missing", false)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!directory.join("missing").exists());
    }

    #[test]
    fn literal_auxiliary_state_keys_refuse_symlinks_before_any_write() {
        use std::io::Write as _;
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let directory = universe_dir(&paths).unwrap();
        drop(open_in_namespace(&directory, "universe-lock").unwrap());
        let outside = temp.path().join("outside-state");
        std::fs::write(&outside, b"must stay intact").unwrap();
        for key in ["universe-generation", "universe-since"] {
            symlink(&outside, directory.join(key)).unwrap();
            for create in [false, true] {
                assert_eq!(
                    open_state_in_namespace(&directory, key, create)
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::PermissionDenied
                );
            }
        }
        assert_eq!(std::fs::read(&outside).unwrap(), b"must stay intact");
        let mut regular = open_state_in_namespace(&directory, "private-state", true).unwrap();
        regular.write_all(b"approved state").unwrap();
        assert_eq!(regular.metadata().unwrap().mode() & 0o077, 0);
        for key in ["", ".", "..", "../outside-state", "nested/state"] {
            assert_eq!(
                open_state_in_namespace(&directory, key, true)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn universe_identity_survives_deleted_data_and_isolates_distinct_roots() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let missing = universe_dir(&paths).unwrap();
        assert!(
            !paths.home_dir.exists(),
            "path derivation must not create state"
        );
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        assert_eq!(universe_dir(&paths).unwrap(), missing);
        std::fs::remove_dir_all(&paths.data_dir).unwrap();
        assert_eq!(universe_dir(&paths).unwrap(), missing);
        let mut other = paths.clone();
        other.data_dir = paths.home_dir.join("other-data");
        assert_ne!(universe_dir(&other).unwrap(), missing);
    }

    #[test]
    fn permanent_lock_rejects_symlinks_and_escaping_keys() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let file = open_lock(&paths, "original").unwrap();
        let identity = file.metadata().unwrap();
        drop(file);
        let file = open_lock(&paths, "original").unwrap();
        let reopened = file.metadata().unwrap();
        assert_eq!(
            (identity.dev(), identity.ino()),
            (reopened.dev(), reopened.ino())
        );
        for key in ["", ".", "..", "../outside", "nested/key"] {
            assert_eq!(
                open_lock(&paths, key).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        let outside = temp.path().join("outside");
        std::fs::write(&outside, b"retain").unwrap();
        symlink(&outside, root(&paths).unwrap().join("symlink.lock")).unwrap();
        open_lock(&paths, "symlink").unwrap_err();
        assert_eq!(std::fs::read(outside).unwrap(), b"retain");
    }

    #[test]
    fn prune_overlap_rejects_namespace_descendants_and_custom_runtime_layouts() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        drop(open_lock(&paths, "retained").unwrap());
        let namespace = root(&paths).unwrap();
        for target in [
            namespace.clone(),
            namespace.join("universes"),
            namespace.join("universes/custom-root"),
        ] {
            assert_eq!(
                ensure_prunable(&paths, &target).unwrap_err().kind(),
                io::ErrorKind::PermissionDenied,
                "namespace descendant cannot be a deletion target"
            );
        }
        for field in ["data", "home", "roles", "cache"] {
            let mut custom = paths.clone();
            let target = namespace.join(field);
            match field {
                "data" => custom.data_dir = target,
                "home" => custom.jackin_home = target,
                "roles" => custom.roles_dir = target,
                "cache" => custom.cache_dir = target,
                _ => unreachable!(),
            }
            assert_eq!(
                root(&custom).unwrap_err().kind(),
                io::ErrorKind::PermissionDenied,
                "custom {field} layout must not enter the coordination namespace"
            );
        }
        assert!(namespace.join("retained.lock").exists());
    }

    #[test]
    fn prune_overlap_rejects_canonical_aliases_and_ambiguous_missing_tails() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(&paths.home_dir).unwrap();
        let alias = temp.path().join("home-alias");
        symlink(&paths.home_dir, &alias).unwrap();
        assert_eq!(
            ensure_prunable(&paths, &alias).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        let dangling = temp.path().join("dangling");
        symlink(temp.path().join("absent"), &dangling).unwrap();
        ensure_prunable(&paths, &dangling).unwrap_err();
        ensure_prunable(&paths, &temp.path().join("missing/../home")).unwrap_err();
        ensure_prunable(&paths, &paths.jackin_home).unwrap();
    }
}

#[cfg(all(test, unix))]
mod security_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    #[test]
    fn namespace_open_rejects_writable_ancestors_and_fifo_before_opening() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let scope = universe_dir(&paths).unwrap();
        drop(open_in_namespace(&scope, "owner").unwrap());
        let universe_parent = scope.parent().unwrap();
        std::fs::set_permissions(universe_parent, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(
            open_in_namespace(&scope, "owner").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        std::fs::set_permissions(universe_parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        let namespace = root(&paths).unwrap();
        std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(
            open_in_namespace(&scope, "owner").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o700)).unwrap();
        let fifo = namespace.join("fifo.lock");
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();
        assert_eq!(
            open_lock(&paths, "fifo").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(
            fifo.exists(),
            "failed preflight must preserve the rejected inode"
        );
    }
    #[test]
    fn namespace_boundary_ignores_matching_basename_above_operator_home() {
        let temp = tempfile::tempdir().unwrap();
        let ancestor = temp.path().join(DIRECTORY);
        std::fs::create_dir_all(&ancestor).unwrap();
        std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o777)).unwrap();
        let mut paths = JackinPaths::for_tests(temp.path());
        paths.home_dir = ancestor.join("users/operator-home");
        let file = open_lock(&paths, "owner").unwrap();
        assert!(file.metadata().unwrap().is_file());
        assert!(
            root(&paths)
                .unwrap()
                .starts_with(resolve(&paths.home_dir).unwrap())
        );
    }
}
