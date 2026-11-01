// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod select;
mod store;

use std::collections::BTreeMap;

use thiserror::Error;

use crate::agent::Agent;
use crate::boundary::FsError;
use crate::docker::EngineConfig;
use crate::ids::{
    AccountName, BuilderName, ChatgptAccountId, EnvName, GatewayName, OpReference,
    OpenShellWorkspace, RoleName, WorkspaceName,
};
use crate::paths::HostPath;
use crate::platform::Platform;
use crate::provider::Provider;

pub use select::*;
pub use store::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayEntry {
    pub openshell_workspace: OpenShellWorkspace,
    pub builder: BuilderName,
    pub platform: Platform,
    pub docker_engine: Option<EngineConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialSource {
    Env {
        var: EnvName,
    },
    OnePassword {
        reference: OpReference,
    },
    File {
        path: HostPath,
    },
    CodexLogin {
        home: HostPath,
        chatgpt_account_id: ChatgptAccountId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleEntry {
    pub source: RoleSource,
    pub bindings: BTreeMap<Agent, AccountName>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleSource {
    Path(HostPath),
    Git { url: String, rev: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub path: HostPath,
    pub docker: bool,
    pub default_role: Option<RoleName>,
    pub bindings: BTreeMap<Provider, AccountName>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigEdit {
    AddGateway {
        name: GatewayName,
        entry: GatewayEntry,
    },
    RemoveGateway(GatewayName),
    SetDefaultGateway(Option<GatewayName>),
    AddAccount(Account),
    RemoveAccount(AccountName),
    SetDefaultAccount {
        account: AccountName,
    },
    ClearDefaultAccount {
        provider: Provider,
    },
    BindWorkspaceAccount {
        workspace: WorkspaceName,
        account: AccountName,
    },
    UnbindWorkspaceAccount {
        workspace: WorkspaceName,
        provider: Provider,
    },
    BindRoleAccount {
        role: RoleName,
        agent: Agent,
        account: AccountName,
    },
    UnbindRoleAccount {
        role: RoleName,
        agent: Agent,
    },
    AddRole {
        name: RoleName,
        source: RoleSource,
    },
    RemoveRole(RoleName),
    AddWorkspace {
        name: WorkspaceName,
        workspace: Workspace,
    },
    RemoveWorkspace(WorkspaceName),
    SetWorkspaceDocker {
        workspace: WorkspaceName,
        docker: bool,
    },
    SetWorkspaceDefaultRole {
        workspace: WorkspaceName,
        role: Option<RoleName>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigReference {
    Default(Provider),
    WorkspaceBinding(WorkspaceName),
    RoleBinding(RoleName),
    WorkspaceDefaultRole(WorkspaceName),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("syntax error at line {line}, column {column}: {message}")]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },
    #[error("unknown field {key}")]
    UnknownField { key: String },
    #[error("duplicate {kind} name {name}")]
    DuplicateName { kind: &'static str, name: String },
    #[error("unknown {kind} {name}")]
    MissingReference { kind: &'static str, name: String },
    #[error("account {account:?} has provider {found:?} but {expected:?} is required")]
    ProviderMismatch {
        account: AccountName,
        expected: Provider,
        found: Provider,
    },
    #[error("a credential of this kind is not allowed for provider {provider:?}")]
    CredentialNotAllowed { provider: Provider },
    #[error("account {account:?} is still referenced: {by:?}")]
    AccountInUse {
        account: AccountName,
        by: Vec<ConfigReference>,
    },
    #[error("role {role:?} is still referenced: {by:?}")]
    RoleInUse {
        role: RoleName,
        by: Vec<ConfigReference>,
    },
    #[error("no gateway is configured; run jackin gateway add")]
    NoGateway,
    #[error("several gateways are configured and none is the default: {names:?}")]
    AmbiguousGateway { names: Vec<GatewayName> },
    #[error("unknown gateway {0:?}")]
    UnknownGateway(GatewayName),
    #[error("no workspace for {0:?}; run jackin workspace add")]
    NoWorkspaceForPath(HostPath),
    #[error(transparent)]
    Fs(#[from] FsError),
}
