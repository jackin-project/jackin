// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Descriptor-relative inspection of a caller-pinned private installation root.
//!
//! The root is a capability supplied by the caller. This module does not
//! authenticate the pathname or ancestors from which that descriptor came.

#[cfg(target_os = "macos")]
mod darwin_acl;
#[cfg(target_os = "linux")]
mod linux_acl;
#[cfg(test)]
mod tests;

use rustix::fd::AsFd;
use rustix::fs::{AtFlags, FileType, Mode, OFlags, Stat, fstat, openat, statat};
use rustix::process::{RawUid, geteuid};
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::{Component, Path};

const DIRECTORY_OPEN_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK)
    .union(OFlags::CLOEXEC);
const LEAF_OPEN_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK)
    .union(OFlags::CLOEXEC);
const NOFOLLOW_STAT_FLAGS: AtFlags = AtFlags::SYMLINK_NOFOLLOW;

/// The exact mode required for one supported installed artifact kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallFileKind {
    /// An installed executable with mode `0755`.
    Executable,
    /// A small receipt with mode `0600`.
    Receipt,
    /// An installed script with mode `0400`.
    Script,
    /// A sealed copy with mode `0500`.
    SealedCopy,
}

impl InstallFileKind {
    fn expected_mode(self) -> Mode {
        match self {
            Self::Executable => Mode::from_bits_truncate(0o755),
            Self::Receipt => Mode::from_bits_truncate(0o600),
            Self::Script => Mode::from_bits_truncate(0o400),
            Self::SealedCopy => Mode::from_bits_truncate(0o500),
        }
    }
}

/// An owned, checked descriptor for a caller-pinned installation directory.
///
/// Construction duplicates the supplied descriptor with close-on-exec set,
/// then requires a current-effective-UID directory with exact mode `0700` and
/// a supported native ACL policy. The external path and its ancestors are not
/// inspected by this type.
pub struct CheckedInstallRoot {
    directory: File,
}

/// A checked file descriptor and the pinned parent/name used to recheck it.
///
/// The file descriptor remains the original opened object. `recheck` reports
/// whether its name still refers to that object; it never substitutes a newly
/// opened descriptor.
pub struct CheckedInstallFile {
    file: File,
    parent: File,
    parent_role: NodeRole,
    leaf_name: OsString,
    kind: InstallFileKind,
    identity: Stat,
}

impl fmt::Debug for CheckedInstallRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CheckedInstallRoot")
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for CheckedInstallFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CheckedInstallFile")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
enum NodeRole {
    Root,
    Directory,
    Leaf(InstallFileKind),
}

#[derive(Clone, Copy)]
struct MetadataFacts {
    file_type: FileType,
    mode: Mode,
    uid: RawUid,
    single_link: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MetadataProblem {
    WrongType,
    WrongOwner,
    WrongMode,
    MultipleLinks,
    WritableByGroupOrOther,
}

struct ParentLookup {
    parent: File,
    role: NodeRole,
    leaf_name: OsString,
}

impl CheckedInstallRoot {
    /// Duplicate and validate an already-open installation directory.
    ///
    /// The caller must choose the correct physical root. This method checks
    /// the descriptor's object and does not resolve or canonicalize a path.
    pub fn from_pinned_directory(directory: File) -> io::Result<Self> {
        let descriptor = rustix::io::fcntl_dupfd_cloexec(&directory, 3)?;
        let directory = File::from(descriptor);
        validate_directory_descriptor(&directory, NodeRole::Root)?;
        Ok(Self { directory })
    }

    /// Open and validate a relative regular file beneath this pinned root.
    ///
    /// Absolute paths, parent traversal, symbolic links, writable ancestors,
    /// hard-linked leaves, unexpected modes, and unsupported ACL inspection
    /// are rejected. The returned descriptor is read-only and close-on-exec.
    pub fn open_file(
        &self,
        relative: &Path,
        kind: InstallFileKind,
    ) -> io::Result<CheckedInstallFile> {
        let lookup = open_parent_directory(&self.directory, relative)?;
        open_checked_leaf(lookup.parent, lookup.role, lookup.leaf_name, kind)
    }
}

impl CheckedInstallFile {
    /// Borrow the original checked descriptor for bounded content inspection.
    pub fn file(&self) -> &File {
        &self.file
    }

    /// Recheck the original object and its name in the same pinned parent.
    ///
    /// The method retains the original descriptor on both success and error.
    pub fn recheck(&self) -> io::Result<()> {
        validate_directory_descriptor(&self.parent, self.parent_role)?;
        let original = fstat(&self.file)?;
        validate_stat(&original, NodeRole::Leaf(self.kind))?;
        ensure_native_acl(self.file.as_fd(), false)?;
        if !same_identity(&original, &self.identity) {
            return Err(permission_denied(
                "checked file descriptor identity changed",
            ));
        }

        let named = statat(&self.parent, &self.leaf_name, NOFOLLOW_STAT_FLAGS)?;
        validate_stat(&named, NodeRole::Leaf(self.kind))?;
        if !same_identity(&named, &self.identity) {
            return Err(permission_denied(
                "checked file name now identifies another object",
            ));
        }

        let candidate = openat(
            &self.parent,
            &self.leaf_name,
            LEAF_OPEN_FLAGS,
            Mode::empty(),
        )?;
        let candidate_file = File::from(candidate);
        let candidate_stat = fstat(&candidate_file)?;
        validate_stat(&candidate_stat, NodeRole::Leaf(self.kind))?;
        ensure_native_acl(candidate_file.as_fd(), false)?;
        if !same_identity(&candidate_stat, &self.identity) {
            return Err(permission_denied("checked file name was rebound"));
        }

        let final_name = statat(&self.parent, &self.leaf_name, NOFOLLOW_STAT_FLAGS)?;
        validate_stat(&final_name, NodeRole::Leaf(self.kind))?;
        if !same_identity(&final_name, &self.identity) {
            return Err(permission_denied(
                "checked file name changed during recheck",
            ));
        }
        Ok(())
    }
}

fn open_parent_directory(root: &File, relative: &Path) -> io::Result<ParentLookup> {
    let mut components = relative_components(relative)?.peekable();
    validate_directory_descriptor(root, NodeRole::Root)?;
    let mut parent = duplicate_file(root)?;
    let mut role = NodeRole::Root;

    while let Some(component) = components.next() {
        if components.peek().is_none() {
            return Ok(ParentLookup {
                parent,
                role,
                leaf_name: component,
            });
        }
        let before = statat(&parent, &component, NOFOLLOW_STAT_FLAGS)?;
        validate_stat(&before, NodeRole::Directory)?;
        let opened = openat(&parent, &component, DIRECTORY_OPEN_FLAGS, Mode::empty())?;
        let opened = File::from(opened);
        let after = fstat(&opened)?;
        validate_stat(&after, NodeRole::Directory)?;
        if !same_identity(&before, &after) {
            return Err(permission_denied(
                "directory changed while opening a path component",
            ));
        }
        ensure_native_acl(opened.as_fd(), true)?;
        parent = opened;
        role = NodeRole::Directory;
    }

    Err(invalid_input("relative installation path has no file name"))
}

fn open_checked_leaf(
    parent: File,
    parent_role: NodeRole,
    leaf_name: OsString,
    kind: InstallFileKind,
) -> io::Result<CheckedInstallFile> {
    validate_directory_descriptor(&parent, parent_role)?;
    let before = statat(&parent, &leaf_name, NOFOLLOW_STAT_FLAGS)?;
    validate_stat(&before, NodeRole::Leaf(kind))?;

    let opened = openat(&parent, &leaf_name, LEAF_OPEN_FLAGS, Mode::empty())?;
    let file = File::from(opened);
    let identity = fstat(&file)?;
    validate_stat(&identity, NodeRole::Leaf(kind))?;
    if !same_identity(&before, &identity) {
        return Err(permission_denied(
            "file changed while opening a path component",
        ));
    }
    ensure_native_acl(file.as_fd(), false)?;

    let final_name = statat(&parent, &leaf_name, NOFOLLOW_STAT_FLAGS)?;
    validate_stat(&final_name, NodeRole::Leaf(kind))?;
    if !same_identity(&final_name, &identity) {
        return Err(permission_denied(
            "file name changed while opening the leaf",
        ));
    }
    ensure_native_acl(file.as_fd(), false)?;

    Ok(CheckedInstallFile {
        file,
        parent,
        parent_role,
        leaf_name,
        kind,
        identity,
    })
}

fn duplicate_file(file: &File) -> io::Result<File> {
    rustix::io::fcntl_dupfd_cloexec(file, 3)
        .map(File::from)
        .map_err(Into::into)
}

fn relative_components(path: &Path) -> io::Result<impl Iterator<Item = OsString> + '_> {
    let mut has_file_name = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_file_name = true,
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(invalid_input(
                    "installation path must stay relative to its pinned root",
                ));
            }
        }
    }
    if !has_file_name {
        return Err(invalid_input("relative installation path is empty"));
    }
    Ok(path.components().filter_map(|component| match component {
        Component::Normal(name) => Some(name.to_os_string()),
        Component::CurDir => None,
        Component::RootDir | Component::ParentDir | Component::Prefix(_) => None,
    }))
}

fn validate_directory_descriptor(file: &File, role: NodeRole) -> io::Result<()> {
    let metadata = fstat(file)?;
    validate_stat(&metadata, role)?;
    ensure_native_acl(file.as_fd(), true)
}

fn validate_stat(metadata: &Stat, role: NodeRole) -> io::Result<()> {
    let facts = MetadataFacts {
        file_type: FileType::from_raw_mode(metadata.st_mode),
        mode: Mode::from_raw_mode(metadata.st_mode),
        uid: metadata.st_uid,
        single_link: metadata.st_nlink == 1,
    };
    let effective_uid = geteuid().as_raw();
    classify_metadata(facts, effective_uid, role).map_err(metadata_problem_error)
}

fn classify_metadata(
    facts: MetadataFacts,
    effective_uid: RawUid,
    role: NodeRole,
) -> Result<(), MetadataProblem> {
    let (expected_type, expected_mode) = match role {
        NodeRole::Root | NodeRole::Directory => (FileType::Directory, None),
        NodeRole::Leaf(kind) => (FileType::RegularFile, Some(kind.expected_mode())),
    };
    if facts.file_type != expected_type {
        return Err(MetadataProblem::WrongType);
    }
    if facts.uid != effective_uid {
        return Err(MetadataProblem::WrongOwner);
    }
    match role {
        NodeRole::Root if facts.mode != Mode::from_bits_truncate(0o700) => {
            return Err(MetadataProblem::WrongMode);
        }
        NodeRole::Directory if facts.mode.intersects(Mode::WGRP | Mode::WOTH) => {
            return Err(MetadataProblem::WritableByGroupOrOther);
        }
        NodeRole::Leaf(_) => {
            if Some(facts.mode) != expected_mode {
                return Err(MetadataProblem::WrongMode);
            }
            if !facts.single_link {
                return Err(MetadataProblem::MultipleLinks);
            }
        }
        NodeRole::Root | NodeRole::Directory => {}
    }
    Ok(())
}

fn metadata_problem_error(problem: MetadataProblem) -> io::Error {
    let message = match problem {
        MetadataProblem::WrongType => "installation entry has an unexpected file type",
        MetadataProblem::WrongOwner => "installation entry is not owned by the effective user",
        MetadataProblem::WrongMode => "installation entry has an unexpected mode",
        MetadataProblem::MultipleLinks => "installation file must have exactly one hard link",
        MetadataProblem::WritableByGroupOrOther => {
            "installation directory is writable by group or other"
        }
    };
    permission_denied(message)
}

fn same_identity(left: &Stat, right: &Stat) -> bool {
    left.st_dev == right.st_dev
        && left.st_ino == right.st_ino
        && FileType::from_raw_mode(left.st_mode) == FileType::from_raw_mode(right.st_mode)
}

fn ensure_native_acl(descriptor: impl AsFd, is_directory: bool) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux_acl::ensure_no_extended_acl(descriptor, is_directory)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = is_directory;
        darwin_acl::ensure_no_mutating_acl(descriptor)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (descriptor, is_directory);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native installation ACL inspection is not implemented on this Unix platform",
        ))
    }
}

fn invalid_input(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn permission_denied(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
