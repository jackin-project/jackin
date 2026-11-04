// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Config, ConfigError, HostPath, IdError, Workspace, WorkspaceName};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceLookup {
    Saved {
        name: WorkspaceName,
        workspace: Workspace,
    },
    Unsaved {
        path: HostPath,
    },
}

pub fn lookup_workspace(config: &Config, path: &HostPath) -> WorkspaceLookup {
    match config.workspace_for(path) {
        Ok((name, workspace)) => WorkspaceLookup::Saved {
            name: name.clone(),
            workspace: workspace.clone(),
        },
        Err(_) => WorkspaceLookup::Unsaved { path: path.clone() },
    }
}

impl WorkspaceLookup {
    pub fn workspace(self) -> Workspace {
        match self {
            WorkspaceLookup::Saved { workspace, .. } => workspace,
            WorkspaceLookup::Unsaved { path } => Workspace::new(path, false),
        }
    }

    pub fn require_saved(self) -> Result<(WorkspaceName, Workspace), ConfigError> {
        match self {
            WorkspaceLookup::Saved { name, workspace } => Ok((name, workspace)),
            WorkspaceLookup::Unsaved { path } => Err(ConfigError::NoWorkspaceForPath(path)),
        }
    }
}

fn suffix(path: &HostPath) -> String {
    let digest = Sha256::digest(path.as_path().as_os_str().as_encoded_bytes());
    format!(
        "{:06x}",
        u32::from_be_bytes([0, digest[0], digest[1], digest[2]])
    )
}

pub fn adhoc_workspace_name(config: &Config, path: &HostPath) -> Result<WorkspaceName, IdError> {
    let base = path
        .as_path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = WorkspaceName::parse(&base).or_else(|_| WorkspaceName::sanitized(&base))?;
    if config.workspace(&name).is_none() {
        return Ok(name);
    }
    let suffix = suffix(path);
    let keep = WorkspaceName::MAX_LEN - suffix.len() - 1;
    let head: String = name.as_str().chars().take(keep).collect();
    let head = head.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
    WorkspaceName::parse(&format!("{head}-{suffix}"))
}
