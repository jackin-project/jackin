// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Config, ConfigError, FileLock, GatewayName, HostPath, IdError, SandboxName, Workspace,
    WorkspaceName,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::accounts::unreachable_text;
use crate::gateways::{Gateways, UnreachableGateway};
use crate::lifecycle::LifecycleError;
use crate::lock::{self, LockError};

pub(crate) fn workspace_lock(name: &WorkspaceName) -> String {
    format!("workspace-{name}.lock")
}

#[derive(Debug)]
pub struct WorkspaceHold {
    _lock: FileLock,
}

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
    let mut tail = suffix.clone();
    let mut attempt = 1_u32;
    loop {
        let keep = WorkspaceName::MAX_LEN - tail.len() - 1;
        let head: String = name.as_str().chars().take(keep).collect();
        let head = head.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
        let candidate = WorkspaceName::parse(&format!("{head}-{tail}"))?;
        if config.workspace(&candidate).is_none() {
            return Ok(candidate);
        }
        attempt += 1;
        tail = format!("{suffix}-{attempt}");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    pub gateway: GatewayName,
    pub sandbox: SandboxName,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceHolders {
    pub readable: Vec<Holder>,
    pub unreadable: Vec<Holder>,
    pub unattributable: Vec<Holder>,
    pub default_gateway: Option<GatewayName>,
}

impl WorkspaceHolders {
    fn is_empty(&self) -> bool {
        self.readable.is_empty() && self.unreadable.is_empty() && self.unattributable.is_empty()
    }

    fn command(&self, holder: &Holder, verb: &str, force: &str) -> String {
        let gateway = if self.default_gateway.as_ref() == Some(&holder.gateway) {
            String::new()
        } else {
            format!(" --gateway {}", holder.gateway)
        };
        format!("`jackin {verb} {}{gateway}{force}`", holder.sandbox)
    }

    fn lines(&self, workspace: &WorkspaceName) -> Vec<String> {
        let readable = self.readable.iter().map(|holder| {
            format!(
                "export and purge {} first (gateway {}, workspace {workspace}): run {}, then {}",
                holder.sandbox,
                holder.gateway,
                self.command(holder, "export", ""),
                self.command(holder, "purge", "")
            )
        });
        let unreadable = self.unreadable.iter().map(|holder| {
            format!(
                "purge {} first (gateway {}, workspace {workspace}): its labels are unreadable, so unexported work in it cannot be exported; run {} to delete it",
                holder.sandbox,
                holder.gateway,
                self.command(holder, "purge", " --force")
            )
        });
        let unattributable = self.unattributable.iter().map(|holder| {
            format!(
                "purge {} first (gateway {}): cannot tell which workspace it uses, and unexported work in it cannot be exported because its labels are unreadable; run {} to delete it",
                holder.sandbox,
                holder.gateway,
                self.command(holder, "purge", " --force")
            )
        });
        readable.chain(unreadable).chain(unattributable).collect()
    }
}

fn holders_text(workspace: &WorkspaceName, holders: &WorkspaceHolders) -> String {
    holders.lines(workspace).join("; ")
}

#[derive(Debug, Error)]
pub enum WorkspaceGuardError {
    #[error("{}", holders_text(.workspace, .holders))]
    InUse {
        workspace: WorkspaceName,
        holders: WorkspaceHolders,
    },
    #[error("{}", unreachable_text(.0))]
    Unreachable(Vec<UnreachableGateway>),
    #[error("cannot list the sandboxes of gateway {gateway}")]
    Listing {
        gateway: GatewayName,
        source: LifecycleError,
    },
    #[error(transparent)]
    Lock(#[from] LockError),
}

impl Gateways {
    pub async fn ensure_workspace_unused(
        &self,
        config: &Config,
        workspace: &WorkspaceName,
        cancel: &CancellationToken,
    ) -> Result<WorkspaceHold, WorkspaceGuardError> {
        let sessions = self.every().map_err(WorkspaceGuardError::Unreachable)?;
        let home = &self.home().deps;
        let lock = lock::hold(
            &home.host,
            &home.cache_dir,
            &workspace_lock(workspace),
            cancel,
        )
        .await?;
        let mut holders = WorkspaceHolders {
            readable: Vec::new(),
            unreadable: Vec::new(),
            unattributable: Vec::new(),
            default_gateway: config.default_gateway().cloned(),
        };
        for session in sessions {
            let gateway = session.gateway_name().clone();
            let listing = session
                .list()
                .await
                .map_err(|source| WorkspaceGuardError::Listing {
                    gateway: gateway.clone(),
                    source,
                })?;
            let holder = |sandbox: SandboxName| Holder {
                gateway: gateway.clone(),
                sandbox,
            };
            holders.readable.extend(
                listing
                    .sandboxes
                    .into_iter()
                    .filter(|sandbox| sandbox.labels.planned.workspace == *workspace)
                    .map(|sandbox| holder(sandbox.name)),
            );
            for broken in listing.unreadable {
                match broken.workspace {
                    Some(found) if found == *workspace => {
                        holders.unreadable.push(holder(broken.name));
                    }
                    Some(_) => {}
                    None => holders.unattributable.push(holder(broken.name)),
                }
            }
        }
        if holders.is_empty() {
            return Ok(WorkspaceHold { _lock: lock });
        }
        for group in [
            &mut holders.readable,
            &mut holders.unreadable,
            &mut holders.unattributable,
        ] {
            group.sort_by(|a, b| (&a.gateway, &a.sandbox).cmp(&(&b.gateway, &b.sandbox)));
        }
        Err(WorkspaceGuardError::InUse {
            workspace: workspace.clone(),
            holders,
        })
    }
}
