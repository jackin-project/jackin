// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Account registry edits under the existing config write lock.
use super::{ConfigEditor, table_path_mut, validate_candidate};
use crate::{AccountConfig, ConfigError, ConfigResult, validate_account_id};
use jackin_core::{Agent, WorkspaceName};
use toml_edit::{DocumentMut, Item};

impl ConfigEditor {
    /// Insert or replace an account. Existing references must remain compatible.
    ///
    /// # Errors
    /// Rejects invalid account IDs, credentials and incompatible references.
    pub fn upsert_account(&mut self, id: &str, account: &AccountConfig) -> ConfigResult<()> {
        validate_account_id(id)?;
        let existing: crate::AppConfig = toml::from_str(&self.doc.to_string())?;
        for (other_id, other) in &existing.accounts {
            if other_id != id && same_credential_source(account, other) {
                return Err(ConfigError::msg(format!(
                    "credential source already registered as {other_id:?}"
                )));
            }
        }
        let mut candidate = self.doc.clone();
        let encoded: DocumentMut = toml::to_string(account)?.parse()?;
        table_path_mut(&mut candidate, &["accounts".into()])
            .insert(id, Item::Table(encoded.as_table().clone()));
        validate_candidate(&candidate.to_string(), &self.workspace_docs)?;
        self.doc = candidate;
        Ok(())
    }
    /// Remove an account and every assignment/binding referring to it.
    ///
    /// # Errors
    /// Returns an error if the account does not exist or the candidate is invalid.
    pub fn remove_account(&mut self, id: &str) -> ConfigResult<()> {
        let mut candidate = self.doc.clone();
        if candidate
            .get_mut("accounts")
            .and_then(Item::as_table_mut)
            .is_none_or(|table| table.remove(id).is_none())
        {
            return Err(ConfigError::msg(format!("unknown account {id:?}")));
        }
        remove_bindings(candidate.as_table_mut(), id);
        let mut workspaces = self.workspace_docs.clone();
        for doc in workspaces.values_mut() {
            if let Some(ids) = doc.get_mut("accounts").and_then(Item::as_array_mut) {
                ids.retain(|value| value.as_str() != Some(id));
            }
            remove_bindings(doc.as_table_mut(), id);
            remove_role_bindings(doc, id);
        }
        validate_candidate(&candidate.to_string(), &workspaces)?;
        self.doc = candidate;
        self.workspace_docs = workspaces;
        Ok(())
    }
    /// Replace a workspace's allowed accounts, pruning bindings for removed IDs.
    ///
    /// # Errors
    /// Rejects missing workspaces, unknown IDs and duplicate assignments.
    pub fn set_workspace_accounts(
        &mut self,
        workspace: &WorkspaceName,
        ids: &[String],
    ) -> ConfigResult<()> {
        let mut docs = self.workspace_docs.clone();
        let doc = docs
            .get_mut(workspace.as_str())
            .ok_or_else(|| ConfigError::WorkspaceNotFound(workspace.as_str().into()))?;
        let previous: crate::WorkspaceConfig = toml::from_str(&doc.to_string())?;
        for removed in previous.accounts.iter().filter(|id| !ids.contains(id)) {
            remove_bindings(doc.as_table_mut(), removed);
            remove_role_bindings(doc, removed);
        }
        let values = ids.iter().map(String::as_str).collect::<toml_edit::Array>();
        doc.insert("accounts", toml_edit::value(values));
        validate_candidate(&self.doc.to_string(), &docs)?;
        self.workspace_docs = docs;
        Ok(())
    }
    /// Set or clear a global, workspace, or workspace-role account selection.
    ///
    /// # Errors
    /// Rejects unknown accounts, incompatible agents and unauthorized workspace accounts.
    pub fn set_account_binding(
        &mut self,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        agent: Agent,
        account: Option<&str>,
    ) -> ConfigResult<()> {
        if let Some(id) = account {
            let existing: crate::AppConfig = toml::from_str(&self.doc.to_string())?;
            if !existing
                .accounts
                .get(id)
                .is_some_and(|account| account.supports_agent(agent))
            {
                return Err(ConfigError::msg(format!(
                    "account {id:?} is disabled or incompatible with {agent}"
                )));
            }
        }
        if role.is_some() && workspace.is_none() {
            return Err(ConfigError::msg(
                "role account bindings require a workspace",
            ));
        }
        let mut global = self.doc.clone();
        let mut docs = self.workspace_docs.clone();
        let doc = if let Some(ws) = workspace {
            docs.get_mut(ws.as_str())
                .ok_or_else(|| ConfigError::WorkspaceNotFound(ws.as_str().into()))?
        } else {
            &mut global
        };
        let path = role.map_or_else(
            || vec!["account_bindings".into()],
            |role| vec!["roles".into(), role.into(), "account_bindings".into()],
        );
        let table = table_path_mut(doc, &path);
        if let Some(id) = account {
            table.insert(agent.slug(), toml_edit::value(id));
        } else {
            table.remove(agent.slug());
        }
        if workspace.is_some() {
            prune_empty_role_overrides(doc)?;
        }
        validate_candidate(&global.to_string(), &docs)?;
        self.doc = global;
        self.workspace_docs = docs;
        Ok(())
    }
}

fn same_credential_source(left: &AccountConfig, right: &AccountConfig) -> bool {
    use crate::AccountCredential;
    if left.provider != right.provider {
        return false;
    }
    match (&left.credential, &right.credential) {
        (
            AccountCredential::Profile {
                agent: a,
                directory: x,
            },
            AccountCredential::Profile {
                agent: b,
                directory: y,
            },
        ) => a == b && x == y,
        (
            AccountCredential::ApiKey {
                value: x,
                base_url: a,
                ..
            },
            AccountCredential::ApiKey {
                value: y,
                base_url: b,
                ..
            },
        ) => x == y && a == b,
        (
            AccountCredential::OAuthToken { agent: a, value: x },
            AccountCredential::OAuthToken { agent: b, value: y },
        ) => a == b && x == y,
        _ => false,
    }
}
fn remove_bindings(table: &mut toml_edit::Table, id: &str) {
    if let Some(bindings) = table
        .get_mut("account_bindings")
        .and_then(Item::as_table_mut)
    {
        bindings.retain(|_, value| value.as_str() != Some(id));
    }
}

fn remove_role_bindings(doc: &mut DocumentMut, id: &str) {
    let Some(roles) = doc.get_mut("roles").and_then(Item::as_table_mut) else {
        return;
    };
    for (_, role) in roles.iter_mut() {
        if let Some(table) = role.as_table_mut() {
            remove_bindings(table, id);
        }
    }
}

fn prune_empty_role_overrides(doc: &mut DocumentMut) -> ConfigResult<()> {
    // Table::to_string omits nested tables. Parse the entire document so a
    // nonempty nested account_bindings/env table cannot look like an empty role.
    let workspace: crate::WorkspaceConfig = toml::from_str(&doc.to_string())?;
    let Some(roles) = doc.get_mut("roles").and_then(Item::as_table_mut) else {
        return Ok(());
    };
    for (name, value) in workspace.roles {
        if value == crate::WorkspaceRoleOverride::default() {
            roles.remove(&name);
        }
    }
    Ok(())
}
