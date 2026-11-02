// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{
    Account, AccountName, BuilderName, Config, CredentialSource, GatewayName, OpenShellWorkspace,
    RoleName, RoleRev, RoleSource, RoleUrl, WorkspaceName,
};
use serde::{Deserialize, Serialize};

pub const VIEW_SCHEMA: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayLs {
    pub schema: u32,
    pub gateways: Vec<GatewayRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRow {
    pub name: GatewayName,
    pub default: bool,
    pub workspace: OpenShellWorkspace,
    pub builder: BuilderName,
    pub platform: String,
    pub engine: Option<EngineRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineRow {
    pub port: u16,
    pub certs: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountLs {
    pub schema: u32,
    pub accounts: Vec<AccountRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRow {
    pub name: AccountName,
    pub provider: String,
    pub credential: String,
    pub default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleLs {
    pub schema: u32,
    pub roles: Vec<RoleRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleRow {
    pub name: RoleName,
    pub source: RoleSourceRow,
    pub bindings: BTreeMap<String, AccountName>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RoleSourceRow {
    Path { path: String },
    Git { url: RoleUrl, rev: Option<RoleRev> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceLs {
    pub schema: u32,
    pub workspaces: Vec<WorkspaceRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRow {
    pub name: WorkspaceName,
    pub path: String,
    pub docker: bool,
    pub default_role: Option<RoleName>,
    pub bindings: BTreeMap<String, AccountName>,
}

pub(crate) fn credential_kind(account: &Account) -> &'static str {
    match account.credential() {
        CredentialSource::Env { .. } => "env",
        CredentialSource::OnePassword { .. } => "op",
        CredentialSource::File { .. } => "file",
        CredentialSource::CodexLogin { .. } => "codex-login",
    }
}

impl GatewayLs {
    pub(crate) fn of(config: &Config) -> GatewayLs {
        GatewayLs {
            schema: VIEW_SCHEMA,
            gateways: config
                .gateways()
                .map(|(name, entry)| GatewayRow {
                    name: name.clone(),
                    default: config.default_gateway() == Some(name),
                    workspace: entry.openshell_workspace.clone(),
                    builder: entry.builder.clone(),
                    platform: entry.platform.docker_arg().to_owned(),
                    engine: entry.docker_engine.as_ref().map(|engine| EngineRow {
                        port: engine.port.get(),
                        certs: engine.client_certs.as_path().display().to_string(),
                    }),
                })
                .collect(),
        }
    }
}

impl AccountLs {
    pub(crate) fn of(config: &Config) -> AccountLs {
        AccountLs {
            schema: VIEW_SCHEMA,
            accounts: config
                .accounts()
                .map(|account| AccountRow {
                    name: account.name().clone(),
                    provider: account.provider().as_str().to_owned(),
                    credential: credential_kind(account).to_owned(),
                    default: config.default_account(account.provider()) == Some(account.name()),
                })
                .collect(),
        }
    }
}

impl RoleLs {
    pub(crate) fn of(config: &Config) -> RoleLs {
        RoleLs {
            schema: VIEW_SCHEMA,
            roles: config
                .roles()
                .map(|(name, entry)| RoleRow {
                    name: name.clone(),
                    source: match &entry.source {
                        RoleSource::Path(path) => RoleSourceRow::Path {
                            path: path.as_path().display().to_string(),
                        },
                        RoleSource::Git { url, rev } => RoleSourceRow::Git {
                            url: url.clone(),
                            rev: rev.clone(),
                        },
                    },
                    bindings: entry
                        .bindings
                        .iter()
                        .map(|(agent, account)| (agent.as_str().to_owned(), account.clone()))
                        .collect(),
                })
                .collect(),
        }
    }
}

impl WorkspaceLs {
    pub(crate) fn of(config: &Config) -> WorkspaceLs {
        WorkspaceLs {
            schema: VIEW_SCHEMA,
            workspaces: config
                .workspaces()
                .map(|(name, workspace)| WorkspaceRow {
                    name: name.clone(),
                    path: workspace.path.as_path().display().to_string(),
                    docker: workspace.docker,
                    default_role: workspace.default_role.clone(),
                    bindings: workspace
                        .bindings
                        .iter()
                        .map(|(provider, account)| (provider.as_str().to_owned(), account.clone()))
                        .collect(),
                })
                .collect(),
        }
    }
}
