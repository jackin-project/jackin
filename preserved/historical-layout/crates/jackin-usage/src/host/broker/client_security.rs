// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Validate the broker rendezvous path before sending host authority over it.

use std::fs;
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};
use std::path::{Component, Path, PathBuf};

use nix::unistd::geteuid;

use super::{
    BROKER_DIR, BROKER_RUN_DIR, BROKER_SOCKET_ALIAS_DIR_PREFIX, UsageBrokerClient, unavailable,
};
use jackin_protocol::usage_broker::UsageCoordinationError;

impl UsageBrokerClient {
    pub(super) fn validate_socket_path(&self) -> Result<(), UsageCoordinationError> {
        if self
            .socket_path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(unavailable());
        }
        let socket = fs::symlink_metadata(&self.socket_path).map_err(|_| unavailable())?;
        if socket.file_type().is_symlink()
            || !socket.file_type().is_socket()
            || socket.uid() != geteuid().as_raw()
            || socket.mode() & 0o777 != 0o600
        {
            return Err(unavailable());
        }
        if let Some(data_dir) = &self.host_data_dir {
            validate_broker_data_tree(data_dir)?;
        }
        let parent = self.socket_path.parent().ok_or_else(unavailable)?;
        let parent_name = parent.file_name().and_then(|name| name.to_str());
        if parent_name == Some(BROKER_RUN_DIR)
            && parent
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                == Some(BROKER_DIR)
        {
            validate_private_directory(parent)?;
            let broker_dir = parent.parent().ok_or_else(unavailable)?;
            validate_private_directory(broker_dir)?;
            let data_dir = broker_dir.parent().ok_or_else(unavailable)?;
            validate_base_directory(data_dir)?;
            validate_broker_data_ancestors(data_dir)?;
            return Ok(());
        }
        let alias_dir = format!("{BROKER_SOCKET_ALIAS_DIR_PREFIX}{}", geteuid().as_raw());
        if parent_name == Some(alias_dir.as_str()) {
            validate_private_directory(parent)?;
            validate_trusted_ancestors(parent.parent().ok_or_else(unavailable)?)?;
            return Ok(());
        }
        if self.socket_path.as_path() == Path::new(jackin_core::container_paths::USAGE_SOCK) {
            validate_runtime_relay_directory(parent)?;
            validate_trusted_ancestors(parent.parent().ok_or_else(unavailable)?)?;
            return Ok(());
        }
        validate_private_directory(parent)?;
        validate_trusted_ancestors(parent.parent().ok_or_else(unavailable)?)
    }
}

pub(super) fn validate_broker_data_tree(data_dir: &Path) -> Result<(), UsageCoordinationError> {
    validate_broker_data_ancestors(data_dir)?;
    validate_base_directory(data_dir)?;
    let broker_dir = data_dir.join(BROKER_DIR);
    validate_private_directory(&broker_dir)?;
    validate_private_directory(&broker_dir.join(BROKER_RUN_DIR))
}

pub(super) fn validate_broker_data_ancestors(
    data_dir: &Path,
) -> Result<(), UsageCoordinationError> {
    if data_dir
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(unavailable());
    }
    let absolute = if data_dir.is_absolute() {
        data_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| unavailable())?
            .join(data_dir)
    };
    validate_trusted_ancestors(absolute.parent().ok_or_else(unavailable)?)
}

fn validate_runtime_relay_directory(path: &Path) -> Result<(), UsageCoordinationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() || metadata.mode() & 0o777 != 0o700 {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_private_directory(path: &Path) -> Result<(), UsageCoordinationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_base_directory(path: &Path) -> Result<(), UsageCoordinationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
    {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_trusted_ancestors(path: &Path) -> Result<(), UsageCoordinationError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| unavailable())?
            .join(path)
    };
    let mut prefix = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::RootDir => prefix.push(component.as_os_str()),
            Component::CurDir => continue,
            Component::ParentDir => return Err(unavailable()),
            Component::Normal(_) | Component::Prefix(_) => prefix.push(component.as_os_str()),
        }
        let link_metadata = fs::symlink_metadata(&prefix).map_err(|_| unavailable())?;
        let (owner, mode, is_dir) = if link_metadata.file_type().is_symlink() {
            if link_metadata.uid() != 0 && link_metadata.uid() != geteuid().as_raw() {
                return Err(unavailable());
            }
            let target = fs::metadata(&prefix).map_err(|_| unavailable())?;
            (target.uid(), target.mode(), target.is_dir())
        } else {
            (
                link_metadata.uid(),
                link_metadata.mode(),
                link_metadata.is_dir(),
            )
        };
        let root_sticky = owner == 0 && mode & 0o1000 != 0;
        if !is_dir
            || (owner != 0 && owner != geteuid().as_raw())
            || (mode & 0o022 != 0 && !root_sticky)
        {
            return Err(unavailable());
        }
    }
    Ok(())
}
