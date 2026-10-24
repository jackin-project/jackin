// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Absolute-path acquisition for a checked private installation root.

#[cfg(test)]
mod tests;

use super::{CheckedInstallRoot, DIRECTORY_OPEN_FLAGS, NodeRole};
use rustix::fd::AsFd;
use rustix::fs::{Mode, fstat, open, openat, statat};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::path::{Component, Path};

pub(super) fn open_absolute_directory(path: &Path) -> io::Result<CheckedInstallRoot> {
    let components = absolute_components(path)?;
    let mut directory = open_filesystem_root()?;

    for (index, component) in components.iter().enumerate() {
        let role = if index + 1 == components.len() {
            NodeRole::Root
        } else {
            NodeRole::Ancestor
        };
        directory = open_directory_component(&directory, component, role)?;
    }

    Ok(CheckedInstallRoot { directory })
}

fn absolute_components(path: &Path) -> io::Result<Vec<OsString>> {
    if !path.is_absolute() {
        return Err(super::invalid_input(
            "installation root path must be absolute",
        ));
    }

    let mut root_seen = false;
    let mut names = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir => root_seen = true,
            Component::Normal(name) => names.push(name.to_os_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => {
                return Err(super::invalid_input(
                    "installation root path must not contain parent traversal",
                ));
            }
        }
    }

    if !root_seen || names.is_empty() {
        return Err(super::invalid_input(
            "installation root path must name a directory below the OS root",
        ));
    }
    Ok(names)
}

fn open_filesystem_root() -> io::Result<File> {
    let opened = File::from(open("/", DIRECTORY_OPEN_FLAGS, Mode::empty())?);
    let root = super::duplicate_file(&opened)?;
    super::validate_directory_descriptor(&root, NodeRole::Ancestor)?;
    Ok(root)
}

fn open_directory_component(parent: &File, name: &OsStr, role: NodeRole) -> io::Result<File> {
    let before = statat(parent, name, super::NOFOLLOW_STAT_FLAGS)?;
    super::validate_stat(&before, role)?;

    let opened = File::from(openat(parent, name, DIRECTORY_OPEN_FLAGS, Mode::empty())?);
    let opened = super::duplicate_file(&opened)?;
    let after = fstat(&opened)?;
    super::validate_stat(&after, role)?;
    if !super::same_identity(&before, &after) {
        return Err(super::permission_denied(
            "installation directory changed while opening a path component",
        ));
    }
    super::ensure_native_acl(opened.as_fd(), true)?;
    Ok(opened)
}
