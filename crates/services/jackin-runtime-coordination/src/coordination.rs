// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::JackinPaths;
use sha2::{Digest as _, Sha256};
use std::io;
use std::path::{Component, Path, PathBuf};

const DIRECTORY: &str = ".jackin-coordination";

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
                match std::fs::symlink_metadata(ancestor) {
                    Ok(metadata) if !metadata.file_type().is_symlink() => {
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

pub fn ensure_prunable(paths: &JackinPaths, pruned: &Path) -> io::Result<()> {
    reject_overlap(&namespace(paths)?, pruned)
}

pub async fn ensure_prunable_async(paths: &JackinPaths, pruned: &Path) -> io::Result<()> {
    let paths = paths.clone();
    let pruned = pruned.to_owned();
    jackin_telemetry::spawn::joined_blocking(move || ensure_prunable(&paths, &pruned))
        .await
        .map_err(io::Error::other)?
}

pub fn universe_dir(paths: &JackinPaths) -> io::Result<PathBuf> {
    let data = resolve(&paths.data_dir)?;
    let mut hash = Sha256::new();
    hash.update(b"jackin-universe-path-v1\0");
    hash.update(data.as_os_str().as_encoded_bytes());
    Ok(root(paths)?
        .join("universes")
        .join(hex::encode(hash.finalize())))
}

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

pub fn open_in_namespace(directory: &Path, key: &str) -> io::Result<std::fs::File> {
    validate_key(key)?;
    let parent = open_directory_in_namespace(directory, true)?;
    open_state_at(&parent, &format!("{key}.lock"), true)
}

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
#[derive(Debug)]
pub enum StatePublishError {
    NotPublished(io::Error),
    VisibleButUncertain(io::Error),
}

impl std::fmt::Display for StatePublishError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPublished(error) => write!(formatter, "state was not published: {error}"),
            Self::VisibleButUncertain(error) => {
                write!(
                    formatter,
                    "state is visible but durability is uncertain: {error}"
                )
            }
        }
    }
}

impl std::error::Error for StatePublishError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotPublished(error) | Self::VisibleButUncertain(error) => Some(error),
        }
    }
}

pub fn read_state_bytes_in_namespace(
    directory: &Path,
    key: &str,
    maximum_bytes: usize,
) -> io::Result<Option<Vec<u8>>> {
    validate_key(key)?;
    let parent = match open_directory_in_namespace(directory, false) {
        Ok(parent) => parent,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let leaf = format!("{key}.json");
    let file = match open_state_at(&parent, &leaf, false) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    read_bounded_state_file(file, maximum_bytes).map(Some)
}

pub fn publish_state_bytes_in_namespace(
    directory: &Path,
    key: &str,
    bytes: &[u8],
    maximum_bytes: usize,
) -> Result<(), StatePublishError> {
    publish_state_bytes_with_faults(
        directory,
        key,
        bytes,
        maximum_bytes,
        PublishTestFaults::default(),
    )
}

#[cfg(unix)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum PublishTestFailure {
    TempSync,
    Rename,
    DirectorySync,
    FinalReadback,
}

#[derive(Default)]
struct PublishTestFaults {
    #[cfg(unix)]
    failure: Option<PublishTestFailure>,
}

fn publish_state_bytes_with_faults(
    directory: &Path,
    key: &str,
    bytes: &[u8],
    maximum_bytes: usize,
    faults: PublishTestFaults,
) -> Result<(), StatePublishError> {
    validate_key(key).map_err(StatePublishError::NotPublished)?;
    if bytes.len() > maximum_bytes {
        return Err(StatePublishError::NotPublished(io::Error::new(
            io::ErrorKind::InvalidInput,
            "coordination state exceeds the configured size limit",
        )));
    }
    let parent =
        open_directory_in_namespace(directory, true).map_err(StatePublishError::NotPublished)?;
    let leaf = format!("{key}.json");
    #[cfg(unix)]
    {
        publish_state_at(&parent, &leaf, bytes, maximum_bytes, faults)
    }
    #[cfg(not(unix))]
    {
        let _ = (parent, leaf, bytes, maximum_bytes, faults);
        Err(StatePublishError::NotPublished(io::Error::new(
            io::ErrorKind::Unsupported,
            "coordination state publication requires no-follow Unix access",
        )))
    }
}

fn read_bounded_state_file(mut file: std::fs::File, maximum_bytes: usize) -> io::Result<Vec<u8>> {
    use std::io::Read as _;

    let maximum_u64 = u64::try_from(maximum_bytes).unwrap_or(u64::MAX);
    let metadata = file.metadata()?;
    if metadata.len() > maximum_u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "coordination state exceeds the configured size limit",
        ));
    }
    let capacity = usize::try_from(metadata.len())
        .unwrap_or(maximum_bytes)
        .min(maximum_bytes);
    let mut bytes = Vec::with_capacity(capacity);
    let read_limit = maximum_u64.saturating_add(1);
    {
        let mut limited = (&mut file).take(read_limit);
        limited.read_to_end(&mut bytes)?;
    }
    if bytes.len() > maximum_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "coordination state grew beyond the configured size limit",
        ));
    }
    let observed_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let final_len = file.metadata()?.len();
    if final_len > maximum_u64 || final_len != observed_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "coordination state changed while being read",
        ));
    }
    Ok(bytes)
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StateInodeIdentity {
    device: nix::libc::dev_t,
    inode: nix::libc::ino_t,
}

#[cfg(unix)]
fn state_inode_identity(file: &std::fs::File) -> io::Result<StateInodeIdentity> {
    use nix::sys::stat::fstat;

    let stat = fstat(file)?;
    Ok(StateInodeIdentity {
        device: stat.st_dev,
        inode: stat.st_ino,
    })
}

#[cfg(unix)]
fn state_leaf_identity(
    parent: &std::fs::File,
    leaf: &str,
) -> io::Result<Option<StateInodeIdentity>> {
    match open_state_at(parent, leaf, false) {
        Ok(file) => state_inode_identity(&file).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn create_staged_state_file(
    parent: &std::fs::File,
) -> io::Result<(String, std::fs::File, StateInodeIdentity)> {
    use nix::errno::Errno;
    use nix::fcntl::{OFlag, openat};
    use nix::sys::stat::{Mode, fstat};
    use std::os::unix::fs::MetadataExt as _;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_LEAF: AtomicU64 = AtomicU64::new(0);
    let flags =
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let mode = Mode::from_bits_truncate(0o600);
    for _ in 0..128 {
        let sequence = NEXT_TEMP_LEAF.fetch_add(1, Ordering::Relaxed);
        let leaf = format!(".jackin-state-{}-{sequence}", std::process::id());
        match openat(parent, leaf.as_str(), flags, mode) {
            Ok(descriptor) => {
                let file = std::fs::File::from(descriptor);
                let metadata = file.metadata()?;
                let stat = fstat(&file)?;
                if !metadata.is_file()
                    || metadata.uid() != nix::unistd::geteuid().as_raw()
                    || metadata.mode() & 0o077 != 0
                    || metadata.nlink() != 1
                    || stat.st_nlink != 1
                {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "staged coordination state must be a private owned regular inode",
                    ));
                }
                let identity = StateInodeIdentity {
                    device: stat.st_dev,
                    inode: stat.st_ino,
                };
                return Ok((leaf, file, identity));
            }
            Err(Errno::EEXIST) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique staged coordination state leaf",
    ))
}

#[cfg(unix)]
fn remove_staged_state_file(
    parent: &std::fs::File,
    leaf: &str,
    expected: StateInodeIdentity,
) -> io::Result<()> {
    use nix::errno::Errno;
    use nix::fcntl::AtFlags;
    use nix::sys::stat::{SFlag, fstat, fstatat};
    use nix::unistd::{UnlinkatFlags, unlinkat};

    let stat = match fstatat(parent, leaf, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::ENOENT) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let identity = StateInodeIdentity {
        device: stat.st_dev,
        inode: stat.st_ino,
    };
    if identity != expected
        || SFlag::from_bits_truncate(stat.st_mode) != SFlag::S_IFREG
        || stat.st_uid != nix::unistd::geteuid().as_raw()
        || stat.st_mode & 0o077 != 0
        || stat.st_nlink != 1
    {
        return Ok(());
    }
    let opened_file = std::fs::File::from(nix::fcntl::openat(
        parent,
        leaf,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC
            | nix::fcntl::OFlag::O_NONBLOCK,
        nix::sys::stat::Mode::empty(),
    )?);
    let opened_stat = fstat(&opened_file)?;
    if opened_stat.st_dev != expected.device || opened_stat.st_ino != expected.inode {
        return Ok(());
    }
    let final_stat = match fstatat(parent, leaf, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::ENOENT) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if final_stat.st_dev != expected.device
        || final_stat.st_ino != expected.inode
        || SFlag::from_bits_truncate(final_stat.st_mode) != SFlag::S_IFREG
        || final_stat.st_uid != nix::unistd::geteuid().as_raw()
        || final_stat.st_mode & 0o077 != 0
        || final_stat.st_nlink != 1
    {
        return Ok(());
    }
    unlinkat(parent, leaf, UnlinkatFlags::NoRemoveDir).map_err(io::Error::from)
}

#[cfg(unix)]
fn publish_state_at(
    parent: &std::fs::File,
    leaf: &str,
    bytes: &[u8],
    maximum_bytes: usize,
    faults: PublishTestFaults,
) -> Result<(), StatePublishError> {
    use nix::fcntl::renameat;
    use std::io::Write as _;

    let previous_identity =
        state_leaf_identity(parent, leaf).map_err(StatePublishError::NotPublished)?;
    let mut staged_leaf = None;
    let mut staged_inode = None;
    let staged_identity_result = (|| -> io::Result<StateInodeIdentity> {
        let (temporary_leaf, mut temporary_file, identity) = create_staged_state_file(parent)?;
        staged_leaf = Some(temporary_leaf.clone());
        staged_inode = Some(identity);
        temporary_file.write_all(bytes)?;
        if faults.failure == Some(PublishTestFailure::TempSync) {
            return Err(io::Error::other("injected staged-file sync failure"));
        }
        temporary_file.sync_all()?;

        let current_identity = state_leaf_identity(parent, leaf)?;
        if current_identity != previous_identity {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "coordination state destination changed before replacement",
            ));
        }
        if faults.failure == Some(PublishTestFailure::Rename) {
            return Err(io::Error::other("injected state rename failure"));
        }
        renameat(parent, temporary_leaf.as_str(), parent, leaf).map_err(io::Error::from)?;
        staged_leaf = None;
        Ok(identity)
    })();
    let staged_identity = match staged_identity_result {
        Ok(identity) => identity,
        Err(error) => {
            if let (Some(temporary_leaf), Some(temporary_identity)) = (staged_leaf, staged_inode)
                && let Err(cleanup_error) =
                    remove_staged_state_file(parent, &temporary_leaf, temporary_identity)
            {
                return Err(StatePublishError::NotPublished(io::Error::new(
                    error.kind(),
                    format!("{error}; staged-file cleanup failed: {cleanup_error}"),
                )));
            }
            return Err(StatePublishError::NotPublished(error));
        }
    };

    if faults.failure == Some(PublishTestFailure::DirectorySync) {
        return Err(StatePublishError::VisibleButUncertain(io::Error::other(
            "injected coordination-directory sync failure after replacement",
        )));
    }
    parent
        .sync_all()
        .map_err(StatePublishError::VisibleButUncertain)?;
    if faults.failure == Some(PublishTestFailure::FinalReadback) {
        return Err(StatePublishError::VisibleButUncertain(io::Error::other(
            "injected final state readback failure after replacement",
        )));
    }
    let final_file =
        open_state_at(parent, leaf, false).map_err(StatePublishError::VisibleButUncertain)?;
    let final_identity =
        state_inode_identity(&final_file).map_err(StatePublishError::VisibleButUncertain)?;
    if final_identity != staged_identity {
        return Err(StatePublishError::VisibleButUncertain(io::Error::new(
            io::ErrorKind::InvalidData,
            "published coordination state inode differs from staged inode",
        )));
    }
    let final_bytes = read_bounded_state_file(final_file, maximum_bytes)
        .map_err(StatePublishError::VisibleButUncertain)?;
    if final_bytes != bytes {
        return Err(StatePublishError::VisibleButUncertain(io::Error::new(
            io::ErrorKind::InvalidData,
            "published coordination state bytes differ from staged bytes",
        )));
    }
    Ok(())
}
