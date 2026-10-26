// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::secure_install::{self, AbsoluteSourceRoot};
use rustix::fs::Stat;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;

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
    pub fn file(&self) -> &File {
        &self.file
    }

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
