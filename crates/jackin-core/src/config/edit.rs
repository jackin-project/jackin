// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::btree_map::Entry;

use super::{
    Account, AllowedRoles, Config, ConfigEdit, ConfigError, ConfigReference, CredentialSource,
    EnvScope, EnvTable, GatewayEntry, KIND_ACCOUNT, KIND_GATEWAY, KIND_ROLE, KIND_WORKSPACE,
    PlainEnvValue, RoleEntry, RoleSource, ScopedEnvName, Workspace, default_role_not_allowed,
    duplicate, missing,
};
use crate::agent::Agent;
use crate::docker::EngineConfig;
use crate::ids::{AccountName, EnvName, GatewayName, RoleName, WorkspaceName};
use crate::provider::Provider;
use crate::tomlkit;

impl Config {
    pub fn apply(&mut self, edit: ConfigEdit) -> Result<(), ConfigError> {
        match edit {
            ConfigEdit::AddGateway { name, entry } => self.add_gateway(name, entry),
            ConfigEdit::RemoveGateway(name) => self.remove_gateway(&name),
            ConfigEdit::SetDefaultGateway(name) => self.set_default_gateway(name),
            ConfigEdit::AddAccount(account) => self.add_account(account),
            ConfigEdit::RemoveAccount(name) => self.remove_account(&name),
            ConfigEdit::RemoveAccountClearingBindings(name) => {
                self.remove_account_clearing_bindings(&name)
            }
            ConfigEdit::SetAccountCredential {
                account,
                credential,
            } => self.set_account_credential(&account, credential),
            ConfigEdit::SetDefaultAccount { account } => self.set_default_account(account),
            ConfigEdit::ClearDefaultAccount { provider } => {
                self.defaults.remove(&provider);
                Ok(())
            }
            ConfigEdit::BindWorkspaceAccount { workspace, account } => {
                self.bind_workspace_account(&workspace, account)
            }
            ConfigEdit::UnbindWorkspaceAccount {
                workspace,
                provider,
            } => self.unbind_workspace_account(&workspace, provider),
            ConfigEdit::BindRoleAccount {
                role,
                agent,
                account,
            } => self.bind_role_account(&role, agent, account),
            ConfigEdit::UnbindRoleAccount { role, agent } => self.unbind_role_account(&role, agent),
            ConfigEdit::AddRole { name, source } => self.add_role(name, source),
            ConfigEdit::RemoveRole(name) => self.remove_role(&name),
            ConfigEdit::AddWorkspace { name, workspace } => self.add_workspace(name, workspace),
            ConfigEdit::RemoveWorkspace(name) => self.remove_workspace(&name),
            ConfigEdit::SetWorkspaceDocker { workspace, docker } => {
                self.workspace_mut(&workspace)?.docker = docker;
                Ok(())
            }
            ConfigEdit::SetWorkspaceDefaultRole { workspace, role } => {
                self.set_workspace_default_role(&workspace, role)
            }
            ConfigEdit::SetWorkspaceRoles { workspace, roles } => {
                self.set_workspace_roles(&workspace, roles)
            }
            ConfigEdit::SetEnv { scope, name, value } => self.set_env(&scope, name, value),
        }
    }

    pub fn require_account(&self, name: &AccountName) -> Result<&Account, ConfigError> {
        self.accounts
            .get(name)
            .ok_or_else(|| missing(KIND_ACCOUNT, name))
    }

    fn require_role(&self, name: &RoleName) -> Result<(), ConfigError> {
        if self.roles.contains_key(name) {
            Ok(())
        } else {
            Err(missing(KIND_ROLE, name))
        }
    }

    fn workspace_mut(&mut self, name: &WorkspaceName) -> Result<&mut Workspace, ConfigError> {
        self.workspaces
            .get_mut(name)
            .ok_or_else(|| missing(KIND_WORKSPACE, name))
    }

    fn role_mut(&mut self, name: &RoleName) -> Result<&mut RoleEntry, ConfigError> {
        self.roles
            .get_mut(name)
            .ok_or_else(|| missing(KIND_ROLE, name))
    }

    fn add_gateway(&mut self, name: GatewayName, entry: GatewayEntry) -> Result<(), ConfigError> {
        if let Some(message) = entry
            .docker_engine
            .as_ref()
            .and_then(|engine| EngineConfig::port_refusal(engine.port))
        {
            return Err(ConfigError::InvalidValue {
                key: format!(
                    "gateways.{}.docker_engine.port",
                    tomlkit::key(name.as_str())
                ),
                message,
            });
        }
        match self.gateways.entry(name) {
            Entry::Occupied(taken) => Err(duplicate(KIND_GATEWAY, taken.key())),
            Entry::Vacant(slot) => {
                slot.insert(entry);
                Ok(())
            }
        }
    }

    fn remove_gateway(&mut self, name: &GatewayName) -> Result<(), ConfigError> {
        self.gateways
            .remove(name)
            .ok_or_else(|| missing(KIND_GATEWAY, name))?;
        if self.default_gateway.as_ref() == Some(name) {
            self.default_gateway = None;
        }
        Ok(())
    }

    fn set_default_gateway(&mut self, name: Option<GatewayName>) -> Result<(), ConfigError> {
        if let Some(name) = &name
            && !self.gateways.contains_key(name)
        {
            return Err(missing(KIND_GATEWAY, name));
        }
        self.default_gateway = name;
        Ok(())
    }

    fn add_account(&mut self, account: Account) -> Result<(), ConfigError> {
        match self.accounts.entry(account.name.clone()) {
            Entry::Occupied(taken) => Err(duplicate(KIND_ACCOUNT, taken.key())),
            Entry::Vacant(slot) => {
                slot.insert(account);
                Ok(())
            }
        }
    }

    fn remove_account(&mut self, name: &AccountName) -> Result<(), ConfigError> {
        self.require_account(name)?;
        let by = self.references_to_account(name);
        if !by.is_empty() {
            return Err(ConfigError::AccountInUse {
                account: name.clone(),
                by,
            });
        }
        self.accounts.remove(name);
        Ok(())
    }

    fn remove_account_clearing_bindings(&mut self, name: &AccountName) -> Result<(), ConfigError> {
        self.require_account(name)?;
        self.defaults.retain(|_, account| account != name);
        for workspace in self.workspaces.values_mut() {
            workspace.bindings.retain(|_, account| account != name);
        }
        for role in self.roles.values_mut() {
            role.bindings.retain(|_, account| account != name);
        }
        self.accounts.remove(name);
        Ok(())
    }

    fn set_account_credential(
        &mut self,
        name: &AccountName,
        credential: CredentialSource,
    ) -> Result<(), ConfigError> {
        let provider = self.require_account(name)?.provider;
        self.accounts.insert(
            name.clone(),
            Account::new(name.clone(), provider, credential)?,
        );
        Ok(())
    }

    fn set_default_account(&mut self, account: AccountName) -> Result<(), ConfigError> {
        let provider = self.require_account(&account)?.provider;
        self.defaults.insert(provider, account);
        Ok(())
    }

    fn bind_workspace_account(
        &mut self,
        workspace: &WorkspaceName,
        account: AccountName,
    ) -> Result<(), ConfigError> {
        let provider = self.require_account(&account)?.provider;
        self.workspace_mut(workspace)?
            .bindings
            .insert(provider, account);
        Ok(())
    }

    fn unbind_workspace_account(
        &mut self,
        workspace: &WorkspaceName,
        provider: Provider,
    ) -> Result<(), ConfigError> {
        self.workspace_mut(workspace)?.bindings.remove(&provider);
        Ok(())
    }

    fn bind_role_account(
        &mut self,
        role: &RoleName,
        agent: Agent,
        account: AccountName,
    ) -> Result<(), ConfigError> {
        self.require_role(role)?;
        self.check_binding(agent.provider(), &account)?;
        self.role_mut(role)?.bindings.insert(agent, account);
        Ok(())
    }

    fn unbind_role_account(&mut self, role: &RoleName, agent: Agent) -> Result<(), ConfigError> {
        self.role_mut(role)?.bindings.remove(&agent);
        Ok(())
    }

    fn add_role(&mut self, name: RoleName, source: RoleSource) -> Result<(), ConfigError> {
        match self.roles.entry(name) {
            Entry::Occupied(taken) => Err(duplicate(KIND_ROLE, taken.key())),
            Entry::Vacant(slot) => {
                slot.insert(RoleEntry {
                    source,
                    bindings: std::collections::BTreeMap::new(),
                });
                Ok(())
            }
        }
    }

    fn remove_role(&mut self, name: &RoleName) -> Result<(), ConfigError> {
        self.require_role(name)?;
        let mut by: Vec<ConfigReference> = Vec::new();
        for (workspace, entry) in &self.workspaces {
            if entry.default_role.as_ref() == Some(name) {
                by.push(ConfigReference::WorkspaceDefaultRole(workspace.clone()));
            }
            if matches!(&entry.roles, AllowedRoles::Only(set) if set.contains(name)) {
                by.push(ConfigReference::WorkspaceRoles(workspace.clone()));
            }
            if entry.role_env.contains_key(name) {
                by.push(ConfigReference::WorkspaceRoleEnv(workspace.clone()));
            }
        }
        if !by.is_empty() {
            return Err(ConfigError::RoleInUse {
                role: name.clone(),
                by,
            });
        }
        self.roles.remove(name);
        Ok(())
    }

    fn add_workspace(
        &mut self,
        name: WorkspaceName,
        workspace: Workspace,
    ) -> Result<(), ConfigError> {
        if self.workspaces.contains_key(&name) {
            return Err(duplicate(KIND_WORKSPACE, &name));
        }
        self.check_workspace(&name, &workspace)?;
        let mut workspaces: Vec<WorkspaceName> = self
            .workspaces
            .iter()
            .filter(|(_, owner)| owner.path == workspace.path)
            .map(|(owner, _)| owner.clone())
            .collect();
        if !workspaces.is_empty() {
            workspaces.push(name);
            workspaces.sort();
            return Err(ConfigError::DuplicatePath {
                path: workspace.path,
                workspaces,
            });
        }
        self.workspaces.insert(name, workspace);
        Ok(())
    }

    fn remove_workspace(&mut self, name: &WorkspaceName) -> Result<(), ConfigError> {
        self.workspaces
            .remove(name)
            .map(|_removed| ())
            .ok_or_else(|| missing(KIND_WORKSPACE, name))
    }

    fn set_workspace_default_role(
        &mut self,
        workspace: &WorkspaceName,
        role: Option<RoleName>,
    ) -> Result<(), ConfigError> {
        if let Some(role) = &role {
            self.require_role(role)?;
        }
        let entry = self.workspace_mut(workspace)?;
        if let Some(role) = &role
            && !entry.roles.allows(role)
        {
            return Err(default_role_not_allowed(workspace, role));
        }
        entry.default_role = role;
        Ok(())
    }

    fn set_workspace_roles(
        &mut self,
        workspace: &WorkspaceName,
        roles: AllowedRoles,
    ) -> Result<(), ConfigError> {
        if let AllowedRoles::Only(set) = &roles {
            for role in set.iter() {
                self.require_role(role)?;
            }
        }
        let entry = self.workspace_mut(workspace)?;
        if let Some(default) = &entry.default_role
            && !roles.allows(default)
        {
            return Err(default_role_not_allowed(workspace, default));
        }
        entry.roles = roles;
        Ok(())
    }

    fn set_env(
        &mut self,
        scope: &EnvScope,
        name: EnvName,
        value: Option<PlainEnvValue>,
    ) -> Result<(), ConfigError> {
        let name =
            ScopedEnvName::new(name.clone()).map_err(|reserved| ConfigError::InvalidValue {
                key: scope.key(&name),
                message: reserved.to_string(),
            })?;
        match scope {
            EnvScope::Global => put(&mut self.env, name, value),
            EnvScope::Workspace(workspace) => {
                put(&mut self.workspace_mut(workspace)?.env, name, value);
            }
            EnvScope::WorkspaceRole(workspace, role) => {
                self.require_role(role)?;
                let entry = self.workspace_mut(workspace)?;
                let table = entry.role_env.entry(role.clone()).or_default();
                put(table, name, value);
                if table.is_empty() {
                    entry.role_env.remove(role);
                }
            }
        }
        Ok(())
    }
}

fn put(table: &mut EnvTable, name: ScopedEnvName, value: Option<PlainEnvValue>) {
    match value {
        Some(value) => {
            table.insert(name, value);
        }
        None => {
            table.remove(&name);
        }
    }
}
