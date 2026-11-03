// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Config, ConfigError, HostPath, Workspace, WorkspaceName};

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
