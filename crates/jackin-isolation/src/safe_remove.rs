// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Owned, validated, file-descriptor-pinned recursive directory removal.
//!
//! Every directory level is pinned with an `O_NOFOLLOW` descriptor opened
//! through `openat`. Once pinned, renames above it cannot redirect traversal.
//! Symlinks where directories are expected are refused; links within removed
//! contents are unlinked without descent. Record-driven paths are bounded by
//! a trusted containment root before missing targets are accepted.

use std::ffi::{CStr, CString, OsString};
use std::io::Read as _;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
use std::os::unix::fs::MetadataExt as _;
use std::path::{Component, Path, PathBuf};

use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::{AtFlags, Flock, FlockArg, OFlag, open, openat};
use nix::sys::stat::{FileStat, Mode, SFlag, fstat, fstatat, mkdirat};
use nix::unistd::{UnlinkatFlags, unlinkat};

const WORKTREE_REGISTRY_LOCK: &str = ".jackin-worktree-registry.lock";

fn dir_oflags() -> OFlag {
    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC
}

/// A directory pinned together with the parent entry that authorized it.
/// No operation re-resolves its display path.
#[derive(Debug)]
pub(crate) struct PinnedDir {
    fd: OwnedFd,
    parent: Option<OwnedFd>,
    name: Option<CString>,
    path: PathBuf,
    identity: FileStat,
}

/// Process-shared admission lease for Jackin worktree-registry mutations.
/// The lock file is persistent: unlinking it could split waiters across inodes.
pub(crate) struct WorktreeRegistryLock {
    _lock: Flock<std::fs::File>,
    file_identity: (u64, u64),
}

impl PinnedDir {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn directory_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    pub(crate) fn identity(&self) -> (u64, u64) {
        (dev_id(&self.identity), self.identity.st_ino)
    }

    /// Serialize Jackin worktree additions and cleanup for this common Git
    /// directory. Git itself does not honor this lock; callers must also
    /// validate checkout inventory while holding the lease.
    pub(crate) async fn lock_worktree_registry(&self) -> std::io::Result<WorktreeRegistryLock> {
        self.verify_entry()?;
        verify_directory_owner(&self.identity, &self.path)?;
        if self.identity.st_mode & 0o022 != 0 && self.identity.st_mode & 0o1000 == 0 {
            return refuse(format!(
                "refusing to lock worktree registry for {}: common Git directory is writable by other users",
                self.path.display()
            ));
        }
        let parent = self.fd.as_fd().try_clone_to_owned()?;
        let path = self.path.clone();
        let runtime = tokio::runtime::Handle::try_current().map_err(|error| {
            std::io::Error::other(format!(
                "worktree registry lock requires a Tokio runtime: {error}"
            ))
        })?;
        let lock = runtime
            .spawn_blocking(move || acquire_worktree_registry_lock(parent, path))
            .await
            .map_err(|error| {
                std::io::Error::other(format!("worktree registry lock task failed: {error}"))
            })??;
        self.verify_entry()?;
        let current = fstatat(
            self.fd.as_fd(),
            WORKTREE_REGISTRY_LOCK,
            AtFlags::AT_SYMLINK_NOFOLLOW,
        )
        .map_err(|error| refuse_io(&self.path, error))?;
        if (dev_id(&current), current.st_ino) != lock.file_identity {
            return refuse(format!(
                "refusing to use worktree registry lock for {}: lock path changed",
                self.path.display()
            ));
        }
        Ok(lock)
    }

    /// Confirm the pinned directory is still registered at its original
    /// parent/name edge. Directory descriptors remain valid after a rename,
    /// so transactions must check this before relying on their inventory.
    pub(crate) fn verify_entry(&self) -> std::io::Result<()> {
        let (Some(parent), Some(name)) = (&self.parent, &self.name) else {
            return refuse(format!(
                "refusing to verify {}: path has no final component",
                self.path.display()
            ));
        };
        verify_entry(parent.as_fd(), name, &self.identity, &self.path)
    }

    pub(crate) fn verify_parent(&self, expected_parent: &PinnedDir) -> std::io::Result<()> {
        let Some(parent) = &self.parent else {
            return refuse(format!(
                "refusing to verify {}: path has no parent",
                self.path.display()
            ));
        };
        let actual = fstat(parent.as_fd()).map_err(|error| refuse_io(&self.path, error))?;
        if dev_id(&actual) != dev_id(&expected_parent.identity)
            || actual.st_ino != expected_parent.identity.st_ino
        {
            return refuse(format!(
                "refusing to use {}: parent identity changed",
                self.path.display()
            ));
        }
        Ok(())
    }

    /// Open one child directory without following symlinks.
    pub(crate) fn open_child_dir(&self, name: &str) -> std::io::Result<Option<Self>> {
        let name = child_name(name.as_bytes(), &self.path)?;
        pin_child(
            self.fd.as_fd(),
            &name,
            &self.path.join(std::ffi::OsStr::from_bytes(name.to_bytes())),
        )
    }

    pub(crate) fn create_child_dir(&self, name: &str) -> std::io::Result<Self> {
        self.verify_entry()?;
        let name = child_name(name.as_bytes(), &self.path)?;
        match mkdirat(
            self.fd.as_fd(),
            Path::new(std::ffi::OsStr::from_bytes(name.to_bytes())),
            Mode::S_IRWXU,
        ) {
            Ok(()) | Err(Errno::EEXIST) => {}
            Err(error) => return Err(refuse_io(&self.path, error)),
        }
        nix::unistd::fsync(self.fd.as_fd()).map_err(|error| refuse_io(&self.path, error))?;
        let child_path = self.path.join(std::ffi::OsStr::from_bytes(name.to_bytes()));
        pin_child(self.fd.as_fd(), &name, &child_path)?
            .ok_or_else(|| std::io::Error::other("created directory disappeared while pinning"))
    }

    /// Read bounded UTF-8 metadata; reject links and nonregular files before reading.
    pub(crate) fn read_file(&self, name: &str) -> std::io::Result<Option<String>> {
        let name = child_name(name.as_bytes(), &self.path)?;
        let path = self.path.join(std::ffi::OsStr::from_bytes(name.to_bytes()));
        let expected = match fstatat(
            self.fd.as_fd(),
            name.as_c_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ) {
            Ok(stat) => stat,
            Err(Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(refuse_io(&path, error)),
        };
        if SFlag::from_bits_truncate(expected.st_mode) != SFlag::S_IFREG {
            return refuse(format!(
                "refusing to read {}: metadata is not a regular file",
                path.display()
            ));
        }
        let fd = match openat(
            self.fd.as_fd(),
            name.as_c_str(),
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(refuse_io(&path, error)),
        };
        let stat = fstat(fd.as_fd()).map_err(|error| refuse_io(&path, error))?;
        if SFlag::from_bits_truncate(stat.st_mode) != SFlag::S_IFREG {
            return refuse(format!(
                "refusing to read {}: metadata is not a regular file",
                path.display()
            ));
        }
        if dev_id(&stat) != dev_id(&expected) || stat.st_ino != expected.st_ino {
            return refuse(format!(
                "refusing to read {}: metadata changed during validation",
                path.display()
            ));
        }
        let mut bytes = Vec::new();
        std::fs::File::from(fd)
            .take(65_537)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 65_536 {
            return refuse(format!(
                "refusing to read {}: metadata exceeds 64 KiB",
                path.display()
            ));
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    pub(crate) fn entry_names(&self) -> std::io::Result<Vec<OsString>> {
        list_names(self.fd.as_fd(), &self.path).map(|names| {
            names
                .into_iter()
                .map(|name| OsString::from_vec(name.into_bytes()))
                .collect()
        })
    }

    /// Delete the pinned contents, refusing any replacement parent entry.
    pub(crate) fn remove(self) -> std::io::Result<()> {
        let (Some(parent), Some(name)) = (&self.parent, &self.name) else {
            return refuse(format!(
                "refusing to remove {}: path has no final component",
                self.path.display()
            ));
        };
        verify_entry(parent.as_fd(), name, &self.identity, &self.path)?;
        remove_dir_contents(self.fd.as_fd(), &self.path)?;
        verify_entry(parent.as_fd(), name, &self.identity, &self.path)?;
        unlinkat(parent.as_fd(), name.as_c_str(), UnlinkatFlags::RemoveDir)
            .map_err(|error| refuse_io(&self.path, error))?;
        nix::unistd::fsync(parent.as_fd()).map_err(|error| refuse_io(&self.path, error))
    }

    /// Atomically move this exact directory under a pinned destination parent.
    /// The destination must not exist; the returned pin follows the moved inode.
    pub(crate) fn rename_into(
        self,
        destination_parent: &PinnedDir,
        destination_name: &str,
    ) -> std::io::Result<PinnedDir> {
        let (Some(source_parent), Some(source_name)) = (&self.parent, &self.name) else {
            return refuse(format!(
                "refusing to quarantine {}: path has no final component",
                self.path.display()
            ));
        };
        let destination_name = child_name(destination_name.as_bytes(), &destination_parent.path)?;
        destination_parent.verify_entry()?;
        let destination_path = destination_parent
            .path
            .join(std::ffi::OsStr::from_bytes(destination_name.to_bytes()));
        verify_entry(
            source_parent.as_fd(),
            source_name,
            &self.identity,
            &self.path,
        )?;
        match fstatat(
            destination_parent.fd.as_fd(),
            destination_name.as_c_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ) {
            Err(Errno::ENOENT) => {}
            Err(error) => return Err(refuse_io(&destination_path, error)),
            Ok(_) => {
                return refuse(format!(
                    "refusing to quarantine {}: destination {} already exists",
                    self.path.display(),
                    destination_path.display()
                ));
            }
        }
        rustix::fs::renameat_with(
            source_parent.as_fd(),
            source_name,
            destination_parent.fd.as_fd(),
            &destination_name,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "refusing to quarantine {}: rename failed: {error}",
                    self.path.display()
                ),
            )
        })?;
        nix::unistd::fsync(source_parent.as_fd()).map_err(|error| refuse_io(&self.path, error))?;
        nix::unistd::fsync(destination_parent.fd.as_fd())
            .map_err(|error| refuse_io(&destination_path, error))?;

        match fstatat(
            source_parent.as_fd(),
            source_name.as_c_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        ) {
            Ok(_) => {
                return refuse(format!(
                    "refusing to quarantine {}: source entry remains after rename",
                    self.path.display()
                ));
            }
            Err(Errno::ENOENT) => {}
            Err(error) => return Err(refuse_io(&self.path, error)),
        }
        let moved = fstatat(
            destination_parent.fd.as_fd(),
            destination_name.as_c_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        )
        .map_err(|error| refuse_io(&destination_path, error))?;
        if dev_id(&moved) != dev_id(&self.identity) || moved.st_ino != self.identity.st_ino {
            return refuse(format!(
                "refusing to quarantine {}: destination identity changed",
                destination_path.display()
            ));
        }
        let PinnedDir { fd, identity, .. } = self;
        let parent = destination_parent
            .fd
            .as_fd()
            .try_clone_to_owned()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        Ok(PinnedDir {
            fd,
            parent: Some(parent),
            name: Some(destination_name),
            path: destination_path,
            identity,
        })
    }
}

fn acquire_worktree_registry_lock(
    parent: OwnedFd,
    path: PathBuf,
) -> std::io::Result<WorktreeRegistryLock> {
    let fd = openat(
        parent.as_fd(),
        WORKTREE_REGISTRY_LOCK,
        OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::S_IRUSR | Mode::S_IWUSR,
    )
    .map_err(|error| refuse_io(&path, error))?;
    let expected = fstat(fd.as_fd()).map_err(|error| refuse_io(&path, error))?;
    if SFlag::from_bits_truncate(expected.st_mode) != SFlag::S_IFREG {
        return refuse(format!(
            "refusing to lock worktree registry for {}: lock path is not a regular file",
            path.display()
        ));
    }
    verify_owned_file(&expected, &path)?;
    if expected.st_mode & 0o022 != 0 {
        return refuse(format!(
            "refusing to lock worktree registry for {}: lock file is writable by other users",
            path.display()
        ));
    }
    let entry = fstatat(
        parent.as_fd(),
        WORKTREE_REGISTRY_LOCK,
        AtFlags::AT_SYMLINK_NOFOLLOW,
    )
    .map_err(|error| refuse_io(&path, error))?;
    if !same_identity(&expected, &entry) {
        return refuse(format!(
            "refusing to lock worktree registry for {}: lock path changed",
            path.display()
        ));
    }
    let file = std::fs::File::from(fd);
    let lock =
        Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, error)| refuse_io(&path, error))?;
    let current = fstatat(
        parent.as_fd(),
        WORKTREE_REGISTRY_LOCK,
        AtFlags::AT_SYMLINK_NOFOLLOW,
    )
    .map_err(|error| refuse_io(&path, error))?;
    if !same_identity(&expected, &current) {
        return refuse(format!(
            "refusing to use worktree registry lock for {}: lock path changed",
            path.display()
        ));
    }
    Ok(WorktreeRegistryLock {
        _lock: lock,
        file_identity: (dev_id(&expected), expected.st_ino),
    })
}

/// Pin an absolute path by walking every component from `/` without links.
/// Invalid syntax is refused even if an earlier component is missing.
pub(crate) fn pin_dir(path: &Path) -> std::io::Result<Option<PinnedDir>> {
    let segments = absolute_segments(path)?;
    let mut fd = open(Path::new("/"), dir_oflags(), Mode::empty())
        .map_err(|error| refuse_io(path, error))?;
    if segments.is_empty() {
        return refuse(format!(
            "refusing to pin {}: path has no final component",
            path.display()
        ));
    }
    for segment in &segments[..segments.len() - 1] {
        fd = match openat(fd.as_fd(), segment.as_c_str(), dir_oflags(), Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(refuse_io(path, error)),
        };
    }
    pin_child(fd.as_fd(), &segments[segments.len() - 1], path)
}

/// Pin a target strictly below the caller's trusted containment root.
/// Validate the original lexical suffix before accepting any missing target.
/// Only the trusted root is canonicalized; target symlinks are never followed.
pub(crate) fn pin_dir_contained(root: &Path, path: &Path) -> std::io::Result<Option<PinnedDir>> {
    let (pinned_root, segments, pinned_path) = contained_root(root, path)?;
    let mut fd = pinned_root.fd;
    for segment in &segments[..segments.len() - 1] {
        fd = match openat(fd.as_fd(), segment.as_c_str(), dir_oflags(), Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(refuse_io(path, error)),
        };
    }
    pin_child(fd.as_fd(), &segments[segments.len() - 1], &pinned_path)
}

fn contained_root(root: &Path, path: &Path) -> std::io::Result<(PinnedDir, Vec<CString>, PathBuf)> {
    absolute_segments(path)?;
    let absolute_root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    absolute_segments(&absolute_root)?;
    let expected = std::fs::symlink_metadata(&absolute_root)?;
    if expected.file_type().is_symlink() || !expected.is_dir() {
        return refuse(format!(
            "refusing to remove {}: containment root is a symlink or not a directory",
            path.display()
        ));
    }
    let canonical_root = std::fs::canonicalize(&absolute_root)?;
    let suffix = path
        .strip_prefix(&absolute_root)
        .or_else(|_| path.strip_prefix(&canonical_root))
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "refusing to remove {}: path escapes containment root {}",
                    path.display(),
                    root.display()
                ),
            )
        })?;
    let segments = normal_segments(suffix)?;
    if segments.is_empty() {
        return refuse(format!(
            "refusing to remove {}: path is the containment root itself",
            path.display()
        ));
    }
    let Some(pinned_root) = pin_dir(&canonical_root)? else {
        return refuse(format!(
            "refusing to remove {}: containment root disappeared",
            path.display()
        ));
    };
    if dev_id(&pinned_root.identity) != expected.dev()
        || pinned_root.identity.st_ino != expected.ino()
    {
        return refuse(format!(
            "refusing to remove {}: containment root changed during validation",
            path.display()
        ));
    }
    let pinned_path = canonical_root.join(suffix);
    Ok((pinned_root, segments, pinned_path))
}

/// Admission for one contained directory removal, including a pinned absence
/// witness. Admission never mutates the filesystem. The consuming effect
/// refuses replacement directories and any target that appeared after admission.
#[derive(Debug)]
pub struct OwnedRemoval {
    ancestors: Vec<PinnedDir>,
    state: AdmittedState,
}

#[derive(Debug)]
enum AdmittedState {
    Present(PinnedDir),
    Absent(CString),
}

impl OwnedRemoval {
    pub fn admit_contained(root: &Path, target: &Path) -> std::io::Result<Self> {
        let (pinned_root, segments, pinned_path) = contained_root(root, target)?;
        let mut ancestors = vec![pinned_root];
        for (index, segment) in segments.iter().enumerate() {
            let parent = ancestors
                .last()
                .ok_or_else(|| std::io::Error::other("missing containment root"))?;
            let child_path = parent
                .path
                .join(std::ffi::OsStr::from_bytes(segment.to_bytes()));
            match pin_child(parent.fd.as_fd(), segment, &child_path)? {
                Some(mut child) if index + 1 == segments.len() => {
                    child.path = pinned_path;
                    return Ok(Self {
                        ancestors,
                        state: AdmittedState::Present(child),
                    });
                }
                Some(child) => ancestors.push(child),
                None => {
                    return Ok(Self {
                        ancestors,
                        state: AdmittedState::Absent(segment.clone()),
                    });
                }
            }
        }
        refuse("refusing admission without a final target component".to_owned())
    }

    pub fn remove(self) -> std::io::Result<()> {
        let root = self
            .ancestors
            .first()
            .ok_or_else(|| std::io::Error::other("missing containment root"))?;
        // Rewalk the trusted canonical root without links. This only observes;
        // destructive operations still use the originally admitted descriptors.
        let current_root = pin_dir(&root.path)?
            .ok_or_else(|| std::io::Error::other("containment root disappeared after admission"))?;
        if dev_id(&current_root.identity) != dev_id(&root.identity)
            || current_root.identity.st_ino != root.identity.st_ino
        {
            return refuse(format!(
                "refusing to remove {}: containment root changed after admission",
                root.path.display()
            ));
        }
        for ancestor in &self.ancestors {
            let (Some(parent), Some(name)) = (&ancestor.parent, &ancestor.name) else {
                return refuse("refusing removal with an unbound ancestor".to_owned());
            };
            verify_entry(parent.as_fd(), name, &ancestor.identity, &ancestor.path)?;
        }
        match self.state {
            AdmittedState::Present(target) => target.remove(),
            AdmittedState::Absent(name) => {
                let parent = self
                    .ancestors
                    .last()
                    .ok_or_else(|| std::io::Error::other("missing absence parent"))?;
                match fstatat(
                    parent.fd.as_fd(),
                    name.as_c_str(),
                    AtFlags::AT_SYMLINK_NOFOLLOW,
                ) {
                    Err(Errno::ENOENT) => Ok(()),
                    Err(error) => Err(refuse_io(&parent.path, error)),
                    Ok(_) => refuse(format!(
                        "refusing to remove {}: entry appeared after admission",
                        parent
                            .path
                            .join(std::ffi::OsStr::from_bytes(name.to_bytes()))
                            .display()
                    )),
                }
            }
        }
    }
}

/// Recursively remove a directory through a pinned lexical path.
/// Missing paths are a no-op; links and non-directories are refused.
pub fn safe_remove_dir_all(path: &Path) -> std::io::Result<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    if let Some(dir) = pin_dir(&absolute)? {
        dir.remove()?;
    }
    Ok(())
}

/// Recursively remove a strictly contained directory through pinned fds.
pub fn safe_remove_dir_contained(root: &Path, path: &Path) -> std::io::Result<()> {
    if let Some(dir) = pin_dir_contained(root, path)? {
        dir.remove()?;
    }
    Ok(())
}

fn absolute_segments(path: &Path) -> std::io::Result<Vec<CString>> {
    if !path.is_absolute() {
        return refuse(format!(
            "refusing to remove {}: contained path must be absolute",
            path.display()
        ));
    }
    // `Path::components` normalizes interior `.`; inspect bytes first.
    if path
        .as_os_str()
        .as_bytes()
        .split(|byte| *byte == b'/')
        .any(|part| part == b"." || part == b"..")
    {
        return refuse(format!(
            "refusing to remove {}: path escapes containment root or has dot components",
            path.display()
        ));
    }
    normal_segments(path.strip_prefix("/").map_err(std::io::Error::other)?)
}

fn normal_segments(path: &Path) -> std::io::Result<Vec<CString>> {
    path.components()
        .map(|component| {
            let Component::Normal(name) = component else {
                return refuse(format!(
                    "refusing to remove {}: path escapes containment root",
                    path.display()
                ));
            };
            child_name(name.as_bytes(), path)
        })
        .collect()
}

fn child_name(name: &[u8], path: &Path) -> std::io::Result<CString> {
    if name.is_empty() || name == b"." || name == b".." || name.contains(&b'/') {
        return refuse(format!(
            "refusing to access {}: expected one normal path component",
            path.display()
        ));
    }
    CString::new(name).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "refusing to access {}: invalid path component",
                path.display()
            ),
        )
    })
}

fn pin_child(
    parent: BorrowedFd<'_>,
    name: &CStr,
    path: &Path,
) -> std::io::Result<Option<PinnedDir>> {
    let expected = match fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::ENOENT) => return Ok(None),
        Err(error) => return Err(refuse_io(path, error)),
    };
    let kind = SFlag::from_bits_truncate(expected.st_mode);
    if kind == SFlag::S_IFLNK {
        return refuse(format!(
            "refusing to remove {}: path is a symlink",
            path.display()
        ));
    }
    if kind != SFlag::S_IFDIR {
        return refuse(format!(
            "refusing to remove {}: path is not a directory",
            path.display()
        ));
    }
    let fd = openat(parent, name, dir_oflags(), Mode::empty())
        .map_err(|error| refuse_io(path, error))?;
    let identity = fstat(fd.as_fd()).map_err(|error| refuse_io(path, error))?;
    if dev_id(&identity) != dev_id(&expected) || identity.st_ino != expected.st_ino {
        return refuse(format!(
            "refusing to remove {}: path changed during validation",
            path.display()
        ));
    }
    verify_directory_owner(&identity, path)?;
    Ok(Some(PinnedDir {
        fd,
        parent: Some(parent.try_clone_to_owned()?),
        name: Some(name.to_owned()),
        path: path.to_path_buf(),
        identity,
    }))
}

fn verify_directory_owner(identity: &FileStat, path: &Path) -> std::io::Result<()> {
    let effective_uid = nix::unistd::geteuid().as_raw();
    if effective_uid != 0 && identity.st_uid != effective_uid {
        return refuse(format!(
            "refusing to remove {}: directory is not owned by the current user",
            path.display()
        ));
    }
    Ok(())
}

fn verify_owned_file(identity: &FileStat, path: &Path) -> std::io::Result<()> {
    let effective_uid = nix::unistd::geteuid().as_raw();
    if effective_uid != 0 && identity.st_uid != effective_uid {
        return refuse(format!(
            "refusing to use {}: file is not owned by the current user",
            path.display()
        ));
    }
    Ok(())
}

fn same_identity(left: &FileStat, right: &FileStat) -> bool {
    dev_id(left) == dev_id(right) && left.st_ino == right.st_ino
}

fn verify_entry(
    parent: BorrowedFd<'_>,
    name: &CStr,
    expected: &FileStat,
    path: &Path,
) -> std::io::Result<()> {
    let actual = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .map_err(|error| refuse_io(path, error))?;
    if dev_id(&actual) != dev_id(expected)
        || actual.st_ino != expected.st_ino
        || SFlag::from_bits_truncate(actual.st_mode) != SFlag::S_IFDIR
    {
        return refuse(format!(
            "refusing to remove {}: path changed during removal",
            path.display()
        ));
    }
    Ok(())
}

/// Device id of an `fstat` result as the `u64` that `Metadata::dev`
/// reports. `st_dev` is already `u64` on Linux, so no conversion exists
/// there; every other platform converts fallibly (macOS `st_dev` is `i32`).
#[cfg(target_os = "linux")]
fn dev_id(stat: &FileStat) -> u64 {
    stat.st_dev
}

/// Device id of an `fstat` result as the `u64` that `Metadata::dev`
/// reports. Portable fallible conversion for non-Linux Unix (macOS
/// `st_dev` is `i32`); see the Linux variant.
#[cfg(not(target_os = "linux"))]
fn dev_id(stat: &FileStat) -> u64 {
    u64::try_from(stat.st_dev).unwrap_or(u64::MAX)
}

/// Delete every entry directly under pinned `dir_fd`, recursing into
/// subdirectories through newly pinned fds. Each entry is classified by an
/// atomic `openat(O_DIRECTORY|O_NOFOLLOW)`: success means directory,
/// `ENOTDIR`/`ELOOP` means unlink-without-descent, anything else aborts
/// loudly so a half-removed tree is never mistaken for a clean one.
fn remove_dir_contents(dir_fd: BorrowedFd<'_>, path: &Path) -> std::io::Result<()> {
    let names = list_names(dir_fd, path)?;
    for name in &names {
        match openat(dir_fd, name.as_c_str(), dir_oflags(), Mode::empty()) {
            Ok(child) => {
                let identity = fstat(child.as_fd()).map_err(|error| refuse_io(path, error))?;
                verify_directory_owner(&identity, path)?;
                remove_dir_contents(child.as_fd(), path)?;
                verify_entry(dir_fd, name, &identity, path)?;
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
    nix::unistd::fsync(dir_fd).map_err(|error| refuse_io(path, error))
}

fn list_names(dir_fd: BorrowedFd<'_>, path: &Path) -> std::io::Result<Vec<CString>> {
    // A new open description avoids sharing directory offsets with the pin.
    let fd = openat(dir_fd, Path::new("."), dir_oflags(), Mode::empty())
        .map_err(|error| refuse_io(path, error))?;
    let mut dir = Dir::from_fd(fd).map_err(|error| refuse_io(path, error))?;
    let mut names = Vec::new();
    for entry in dir.iter() {
        let entry = entry.map_err(|error| refuse_io(path, error))?;
        let name = entry.file_name();
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            names.push(name.to_owned());
        }
    }
    Ok(names)
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
