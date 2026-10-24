// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Read-only descriptor-relative inspection of a selected source tree.
//!
//! This capability checks current filesystem objects. It retains the
//! selected root's original parent/name edge and each relative directory edge
//! so callers can recheck those names while retaining the original content
//! descriptor. It does not authenticate source, establish a complete build
//! input closure, attest a build or running image, or defend against every
//! same-user or mount-level change.

#[cfg(test)]
mod tests;

use super::secure_install::{self, AbsoluteSourceRoot};
use rustix::fs::Stat;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;

/// An owned, checked descriptor for a selected source directory.
///
/// The final directory may have a safe mode such as `0755` or `0700`; unlike
/// [`super::CheckedInstallRoot`], it is not an installation root and is not
/// required to have exact mode `0700`. Construction checks the physical
/// absolute path from the OS root and retains the selected root's immediate
/// parent descriptor, name, and identity for later rechecks. It does not retain
/// every absolute ancestor name forever.
pub struct CheckedSourceRoot {
    inner: Arc<SourceRootInner>,
}

struct SourceRootInner {
    directory: File,
    parent: File,
    name: OsString,
    identity: Stat,
}

struct CheckedSourceDirectory {
    directory: File,
    name: OsString,
    identity: Stat,
}

/// A read-only source-file descriptor and the original root-relative names
/// required to recheck its custody.
pub struct CheckedSourceFile {
    root: Arc<SourceRootInner>,
    directories: Vec<CheckedSourceDirectory>,
    file: File,
    leaf_name: OsString,
    identity: Stat,
}

impl fmt::Debug for CheckedSourceRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CheckedSourceRoot")
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for CheckedSourceFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CheckedSourceFile")
            .finish_non_exhaustive()
    }
}

impl CheckedSourceRoot {
    /// Open and validate a source directory from an absolute path.
    ///
    /// The selected directory must belong to the effective user, must not be
    /// writable by group or other, and must pass the platform's native ACL
    /// inspection. Safe source-checkout modes such as `0755` and private
    /// fixture modes such as `0700` are accepted. This does not select or
    /// authenticate source contents.
    pub fn from_absolute_directory(path: &Path) -> io::Result<Self> {
        let AbsoluteSourceRoot {
            directory,
            parent,
            name,
            identity,
        } = secure_install::open_absolute_source_root(path)?;
        let root = Self {
            inner: Arc::new(SourceRootInner {
                directory,
                parent,
                name,
                identity,
            }),
        };
        root.recheck()?;
        Ok(root)
    }

    /// Open a regular source file beneath this root without following links.
    ///
    /// The returned descriptor is the original read-only, close-on-exec file
    /// object. The capability retains and rechecks the root parent/name edge,
    /// every traversed relative directory edge, and the original leaf name.
    /// It never substitutes a candidate descriptor opened during recheck.
    pub fn open_file(&self, relative: &Path) -> io::Result<CheckedSourceFile> {
        self.recheck()?;
        let mut components = secure_install::source_relative_components(relative)?;
        let leaf_name = components
            .pop()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "source path is empty"))?;

        let mut directories = Vec::with_capacity(components.len());
        for name in components {
            let parent = directories.last().map_or(
                &self.inner.directory,
                |directory: &CheckedSourceDirectory| &directory.directory,
            );
            let (directory, identity) = secure_install::open_source_directory(parent, &name)?;
            directories.push(CheckedSourceDirectory {
                directory,
                name,
                identity,
            });
        }

        let parent = directories
            .last()
            .map_or(&self.inner.directory, |directory| &directory.directory);
        let (file, identity) = secure_install::open_source_file(parent, &leaf_name)?;
        let checked = CheckedSourceFile {
            root: Arc::clone(&self.inner),
            directories,
            file,
            leaf_name,
            identity,
        };
        checked.recheck()?;
        Ok(checked)
    }

    /// Recheck the original root descriptor and its retained parent/name edge.
    ///
    /// Only the selected root's immediate absolute parent/name edge is retained
    /// after acquisition; this is not a continuous check of every absolute
    /// ancestor or mount namespace.
    pub fn recheck(&self) -> io::Result<()> {
        secure_install::recheck_absolute_source_root(
            &self.inner.parent,
            &self.inner.name,
            &self.inner.directory,
            &self.inner.identity,
        )
    }
}

impl CheckedSourceFile {
    /// Borrow the original read-only descriptor for content inspection.
    pub fn file(&self) -> &File {
        &self.file
    }

    /// Recheck the original descriptor and all retained names from the source
    /// root through the leaf.
    ///
    /// This checks names, object identities, metadata policy, and native ACLs;
    /// it does not hash contents or detect every in-place write by the same
    /// user. A higher-level bounded measurement must perform its own content
    /// and closure checks.
    pub fn recheck(&self) -> io::Result<()> {
        secure_install::recheck_absolute_source_root(
            &self.root.parent,
            &self.root.name,
            &self.root.directory,
            &self.root.identity,
        )?;

        let mut parent = &self.root.directory;
        for directory in &self.directories {
            secure_install::recheck_source_directory(
                parent,
                &directory.name,
                &directory.directory,
                &directory.identity,
            )?;
            parent = &directory.directory;
        }

        secure_install::recheck_source_file(parent, &self.leaf_name, &self.file, &self.identity)
    }
}
