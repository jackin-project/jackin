// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod edit;
mod parse;
mod render;
mod select;
mod store;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use thiserror::Error;

use crate::agent::{Agent, CodexAuthMode};
use crate::boundary::FsError;
use crate::display::plain;
use crate::docker::EngineConfig;
use crate::ids::{
    AccountName, BuilderName, ChatgptAccountId, EnvName, GatewayName, OpReference,
    OpenShellWorkspace, RoleName, RoleRev, RoleUrl, WorkspaceName,
};
use crate::paths::HostPath;
use crate::platform::Platform;
use crate::provider::Provider;
use crate::tomlkit::Finding;

pub use select::*;
pub use store::*;

pub(crate) const KIND_GATEWAY: &str = "gateway";
pub(crate) const KIND_ACCOUNT: &str = "account";
pub(crate) const KIND_ROLE: &str = "role";
pub(crate) const KIND_WORKSPACE: &str = "workspace";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    default_gateway: Option<GatewayName>,
    env: EnvTable,
    gateways: BTreeMap<GatewayName, GatewayEntry>,
    defaults: BTreeMap<Provider, AccountName>,
    accounts: BTreeMap<AccountName, Account>,
    roles: BTreeMap<RoleName, RoleEntry>,
    workspaces: BTreeMap<WorkspaceName, Workspace>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayEntry {
    pub openshell_workspace: OpenShellWorkspace,
    pub builder: BuilderName,
    pub platform: Platform,
    pub docker_engine: Option<EngineConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    name: AccountName,
    provider: Provider,
    credential: CredentialSource,
}

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
    Git { url: RoleUrl, rev: Option<RoleRev> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub path: HostPath,
    pub docker: bool,
    pub default_role: Option<RoleName>,
    pub bindings: BTreeMap<Provider, AccountName>,
    pub roles: AllowedRoles,
    pub env: EnvTable,
    pub role_env: BTreeMap<RoleName, EnvTable>,
}

pub type EnvTable = BTreeMap<EnvName, PlainEnvValue>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlainEnvValue(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum EnvValueError {
    #[error("a 1Password reference is not allowed; jackin keeps no secrets in env, use an account")]
    OnePassword,
    #[error("expected text without a newline or a NUL")]
    Control,
}

impl PlainEnvValue {
    pub fn parse(text: &str) -> Result<PlainEnvValue, EnvValueError> {
        if text.starts_with("op://") {
            Err(EnvValueError::OnePassword)
        } else if text.contains(['\n', '\0']) {
            Err(EnvValueError::Control)
        } else {
            Ok(PlainEnvValue(text.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PlainEnvValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleSet(BTreeSet<RoleName>);

impl RoleSet {
    pub fn new(roles: impl IntoIterator<Item = RoleName>) -> Option<RoleSet> {
        let roles: BTreeSet<RoleName> = roles.into_iter().collect();
        (!roles.is_empty()).then_some(RoleSet(roles))
    }

    pub fn contains(&self, role: &RoleName) -> bool {
        self.0.contains(role)
    }

    pub fn iter(&self) -> impl Iterator<Item = &RoleName> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AllowedRoles {
    #[default]
    All,
    Only(RoleSet),
}

impl AllowedRoles {
    pub fn allows(&self, role: &RoleName) -> bool {
        match self {
            AllowedRoles::All => true,
            AllowedRoles::Only(set) => set.contains(role),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvScope {
    Global,
    Workspace(WorkspaceName),
    WorkspaceRole(WorkspaceName, RoleName),
}

impl Workspace {
    pub fn new(path: HostPath, docker: bool) -> Workspace {
        Workspace {
            path,
            docker,
            default_role: None,
            bindings: BTreeMap::new(),
            roles: AllowedRoles::All,
            env: EnvTable::new(),
            role_env: BTreeMap::new(),
        }
    }
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
    RemoveAccountClearingBindings(AccountName),
    SetAccountCredential {
        account: AccountName,
        credential: CredentialSource,
    },
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
    SetWorkspaceRoles {
        workspace: WorkspaceName,
        roles: AllowedRoles,
    },
    SetEnv {
        scope: EnvScope,
        name: EnvName,
        value: Option<PlainEnvValue>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigReference {
    Default(Provider),
    WorkspaceBinding(WorkspaceName),
    RoleBinding(RoleName),
    WorkspaceDefaultRole(WorkspaceName),
    WorkspaceRoles(WorkspaceName),
    WorkspaceRoleEnv(WorkspaceName),
}

impl fmt::Display for ConfigReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigReference::Default(provider) => {
                write!(f, "the default {} account", provider.as_str())
            }
            ConfigReference::WorkspaceBinding(workspace) => {
                write!(f, "the binding of workspace {workspace}")
            }
            ConfigReference::RoleBinding(role) => write!(f, "the binding of role {role}"),
            ConfigReference::WorkspaceDefaultRole(workspace) => {
                write!(f, "the default role of workspace {workspace}")
            }
            ConfigReference::WorkspaceRoles(workspace) => {
                write!(f, "the allowed roles of workspace {workspace}")
            }
            ConfigReference::WorkspaceRoleEnv(workspace) => {
                write!(f, "the role variables of workspace {workspace}")
            }
        }
    }
}

pub(crate) fn join_display<T: fmt::Display>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("syntax error at line {line}, column {column}: {}", plain(.message))]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },
    #[error("unknown field {}", plain(.key))]
    UnknownField { key: String },
    #[error("missing field {}", plain(.key))]
    MissingField { key: String },
    #[error("invalid value for {}: {}", plain(.key), plain(.message))]
    InvalidValue { key: String, message: String },
    #[error("duplicate {kind} name {}", plain(.name))]
    DuplicateName { kind: &'static str, name: String },
    #[error("unknown {kind} {}", plain(.name))]
    MissingReference { kind: &'static str, name: String },
    #[error(
        "account {account} has provider {} but {} is required",
        .found.as_str(),
        .expected.as_str()
    )]
    ProviderMismatch {
        account: AccountName,
        expected: Provider,
        found: Provider,
    },
    #[error("a ChatGPT login credential is not allowed for the {} provider", .provider.as_str())]
    CredentialNotAllowed { provider: Provider },
    #[error("account {account} is still referenced by {}", join_display(.by))]
    AccountInUse {
        account: AccountName,
        by: Vec<ConfigReference>,
    },
    #[error("role {role} is still referenced by {}", join_display(.by))]
    RoleInUse {
        role: RoleName,
        by: Vec<ConfigReference>,
    },
    #[error(
        "workspaces {} share the path {}",
        join_display(.workspaces),
        .path.as_path().display()
    )]
    DuplicatePath {
        path: HostPath,
        workspaces: Vec<WorkspaceName>,
    },
    #[error("no gateway is configured; run jackin gateway add")]
    NoGateway,
    #[error(
        "several gateways are configured and none is the default ({}); pass --gateway",
        join_display(.names)
    )]
    AmbiguousGateway { names: Vec<GatewayName> },
    #[error("unknown gateway {0}")]
    UnknownGateway(GatewayName),
    #[error("no workspace for {}; run jackin workspace add", .0.as_path().display())]
    NoWorkspaceForPath(HostPath),
    #[error(transparent)]
    Fs(#[from] FsError),
}

impl From<Finding> for ConfigError {
    fn from(finding: Finding) -> ConfigError {
        match finding {
            Finding::Unknown { key } => ConfigError::UnknownField { key },
            Finding::Missing { key } => ConfigError::MissingField { key },
            Finding::Invalid { key, message } => ConfigError::InvalidValue { key, message },
        }
    }
}

pub(crate) fn missing(kind: &'static str, name: impl fmt::Display) -> ConfigError {
    ConfigError::MissingReference {
        kind,
        name: name.to_string(),
    }
}

pub(crate) fn reserved_env_name(name: &EnvName) -> Result<(), ConfigError> {
    if name.is_reserved() {
        Err(ConfigError::InvalidValue {
            key: format!("env.{name}"),
            message: "reserved by jackin".to_owned(),
        })
    } else {
        Ok(())
    }
}

pub(crate) fn default_role_not_allowed(workspace: &WorkspaceName, role: &RoleName) -> ConfigError {
    ConfigError::InvalidValue {
        key: format!("workspaces.{workspace}.default_role"),
        message: format!("role {role} is not in the allowed roles"),
    }
}

pub(crate) fn duplicate(kind: &'static str, name: impl fmt::Display) -> ConfigError {
    ConfigError::DuplicateName {
        kind,
        name: name.to_string(),
    }
}

impl Account {
    pub fn new(
        name: AccountName,
        provider: Provider,
        credential: CredentialSource,
    ) -> Result<Account, ConfigError> {
        if matches!(credential, CredentialSource::CodexLogin { .. }) && provider != Provider::Codex
        {
            return Err(ConfigError::CredentialNotAllowed { provider });
        }
        Ok(Account {
            name,
            provider,
            credential,
        })
    }

    pub fn name(&self) -> &AccountName {
        &self.name
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn credential(&self) -> &CredentialSource {
        &self.credential
    }

    pub fn codex_auth_mode(&self) -> Option<CodexAuthMode> {
        if self.provider != Provider::Codex {
            return None;
        }
        Some(match self.credential {
            CredentialSource::CodexLogin { .. } => CodexAuthMode::ChatgptLogin,
            CredentialSource::Env { .. }
            | CredentialSource::OnePassword { .. }
            | CredentialSource::File { .. } => CodexAuthMode::ApiKey,
        })
    }
}

impl Config {
    pub fn empty() -> Config {
        Config::default()
    }

    pub fn default_gateway(&self) -> Option<&GatewayName> {
        self.default_gateway.as_ref()
    }

    pub fn env(&self) -> &EnvTable {
        &self.env
    }

    pub fn gateway(&self, name: &GatewayName) -> Option<&GatewayEntry> {
        self.gateways.get(name)
    }

    pub fn gateways(&self) -> impl Iterator<Item = (&GatewayName, &GatewayEntry)> {
        self.gateways.iter()
    }

    pub fn account(&self, name: &AccountName) -> Option<&Account> {
        self.accounts.get(name)
    }

    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }

    pub fn default_account(&self, provider: Provider) -> Option<&AccountName> {
        self.defaults.get(&provider)
    }

    pub fn role(&self, name: &RoleName) -> Option<&RoleEntry> {
        self.roles.get(name)
    }

    pub fn roles(&self) -> impl Iterator<Item = (&RoleName, &RoleEntry)> {
        self.roles.iter()
    }

    pub fn workspace(&self, name: &WorkspaceName) -> Option<&Workspace> {
        self.workspaces.get(name)
    }

    pub fn workspaces(&self) -> impl Iterator<Item = (&WorkspaceName, &Workspace)> {
        self.workspaces.iter()
    }

    pub fn workspace_for(
        &self,
        path: &HostPath,
    ) -> Result<(&WorkspaceName, &Workspace), ConfigError> {
        self.workspaces
            .iter()
            .find(|(_, workspace)| workspace.path == *path)
            .ok_or_else(|| ConfigError::NoWorkspaceForPath(path.clone()))
    }

    pub fn workspace_accounts(&self, name: &WorkspaceName) -> Vec<&Account> {
        self.workspaces
            .get(name)
            .into_iter()
            .flat_map(|workspace| workspace.bindings.values())
            .filter_map(|account| self.accounts.get(account))
            .collect()
    }

    pub fn resolve_gateway(
        &self,
        requested: Option<&GatewayName>,
    ) -> Result<(&GatewayName, &GatewayEntry), ConfigError> {
        if let Some(name) = requested {
            return self
                .gateways
                .get_key_value(name)
                .ok_or_else(|| ConfigError::UnknownGateway(name.clone()));
        }
        if let Some(entry) = self
            .default_gateway
            .as_ref()
            .and_then(|name| self.gateways.get_key_value(name))
        {
            return Ok(entry);
        }
        let mut all = self.gateways.iter();
        match (all.next(), all.next()) {
            (None, _) => Err(ConfigError::NoGateway),
            (Some(only), None) => Ok(only),
            (Some(_), Some(_)) => Err(ConfigError::AmbiguousGateway {
                names: self.gateways.keys().cloned().collect(),
            }),
        }
    }

    pub(crate) fn references_to_account(&self, name: &AccountName) -> Vec<ConfigReference> {
        let defaults = self
            .defaults
            .iter()
            .filter(|(_, account)| *account == name)
            .map(|(provider, _)| ConfigReference::Default(*provider));
        let workspaces = self
            .workspaces
            .iter()
            .filter(|(_, workspace)| workspace.bindings.values().any(|account| account == name))
            .map(|(workspace, _)| ConfigReference::WorkspaceBinding(workspace.clone()));
        let roles = self
            .roles
            .iter()
            .filter(|(_, role)| role.bindings.values().any(|account| account == name))
            .map(|(role, _)| ConfigReference::RoleBinding(role.clone()));
        defaults.chain(workspaces).chain(roles).collect()
    }

    pub(crate) fn check_binding(
        &self,
        provider: Provider,
        account: &AccountName,
    ) -> Result<(), ConfigError> {
        let found = self
            .accounts
            .get(account)
            .ok_or_else(|| missing(KIND_ACCOUNT, account))?;
        if found.provider == provider {
            Ok(())
        } else {
            Err(ConfigError::ProviderMismatch {
                account: account.clone(),
                expected: provider,
                found: found.provider,
            })
        }
    }

    pub(crate) fn check_references(&self) -> Result<(), ConfigError> {
        if let Some(name) = &self.default_gateway
            && !self.gateways.contains_key(name)
        {
            return Err(missing(KIND_GATEWAY, name));
        }
        for (provider, account) in &self.defaults {
            self.check_binding(*provider, account)?;
        }
        for name in self.env.keys() {
            reserved_env_name(name)?;
        }
        for (name, workspace) in &self.workspaces {
            self.check_workspace(name, workspace)?;
        }
        for role in self.roles.values() {
            for (agent, account) in &role.bindings {
                self.check_binding(agent.provider(), account)?;
            }
        }
        self.check_paths_unique()
    }

    fn check_paths_unique(&self) -> Result<(), ConfigError> {
        let mut owners: BTreeMap<&HostPath, Vec<WorkspaceName>> = BTreeMap::new();
        for (name, workspace) in &self.workspaces {
            owners
                .entry(&workspace.path)
                .or_default()
                .push(name.clone());
        }
        match owners.into_iter().find(|(_, names)| names.len() > 1) {
            Some((path, workspaces)) => Err(ConfigError::DuplicatePath {
                path: path.clone(),
                workspaces,
            }),
            None => Ok(()),
        }
    }

    pub(crate) fn check_workspace(
        &self,
        name: &WorkspaceName,
        workspace: &Workspace,
    ) -> Result<(), ConfigError> {
        for (provider, account) in &workspace.bindings {
            self.check_binding(*provider, account)?;
        }
        if let Some(role) = &workspace.default_role {
            self.require_configured_role(role)?;
            if !workspace.roles.allows(role) {
                return Err(default_role_not_allowed(name, role));
            }
        }
        if let AllowedRoles::Only(set) = &workspace.roles {
            for role in set.iter() {
                self.require_configured_role(role)?;
            }
        }
        for (role, table) in &workspace.role_env {
            self.require_configured_role(role)?;
            for env_name in table.keys() {
                reserved_env_name(env_name)?;
            }
        }
        for env_name in workspace.env.keys() {
            reserved_env_name(env_name)?;
        }
        Ok(())
    }

    fn require_configured_role(&self, role: &RoleName) -> Result<(), ConfigError> {
        if self.roles.contains_key(role) {
            Ok(())
        } else {
            Err(missing(KIND_ROLE, role))
        }
    }
}
