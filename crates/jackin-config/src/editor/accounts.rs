// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Account registry edits under the existing config write lock.
use super::{ConfigEditor, table_path_mut, validate_candidate};
use crate::accounts::account_source_fingerprint;
use crate::{AccountConfig, ConfigError, ConfigResult, validate_account_id};
use jackin_core::{Agent, WorkspaceName};
use std::collections::{BTreeMap, BTreeSet};
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
        let mut workspaces = self.workspace_docs.clone();
        if !account.enabled {
            remove_bindings(candidate.as_table_mut(), id);
            for doc in workspaces.values_mut() {
                remove_bindings(doc.as_table_mut(), id);
                remove_role_bindings(doc, id);
            }
            prune_agent_configurations(&mut candidate, &mut workspaces, id)?;
        }
        remove_scan_exclusion(&mut candidate, &account_source_fingerprint(account));
        validate_candidate(&candidate.to_string(), &workspaces)?;
        self.doc = candidate;
        self.workspace_docs = workspaces;
        Ok(())
    }
    /// Prune bindings to the given account ID across global, workspace, and role scopes.
    ///
    /// # Errors
    /// Returns an error if the candidate configuration is invalid.
    pub fn prune_account_bindings(&mut self, id: &str) -> ConfigResult<()> {
        let mut candidate = self.doc.clone();
        remove_bindings(candidate.as_table_mut(), id);
        let mut workspaces = self.workspace_docs.clone();
        for doc in workspaces.values_mut() {
            remove_bindings(doc.as_table_mut(), id);
            remove_role_bindings(doc, id);
        }
        validate_candidate(&candidate.to_string(), &workspaces)?;
        self.doc = candidate;
        self.workspace_docs = workspaces;
        Ok(())
    }
    /// Remove an account and every assignment/binding referring to it.
    ///
    /// # Errors
    /// Returns an error if the account does not exist or the candidate is invalid.
    pub fn remove_account(&mut self, id: &str) -> ConfigResult<()> {
        let existing: crate::AppConfig = toml::from_str(&self.doc.to_string())?;
        let account = existing
            .accounts
            .get(id)
            .ok_or_else(|| ConfigError::msg(format!("unknown account {id:?}")))?;
        let source_fingerprint = account_source_fingerprint(account);
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
        prune_agent_configurations(&mut candidate, &mut workspaces, id)?;
        add_scan_exclusion(&mut candidate, &source_fingerprint);
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
    account_source_fingerprint(left) == account_source_fingerprint(right)
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

fn prune_agent_configurations(
    global: &mut DocumentMut,
    workspaces: &mut BTreeMap<String, DocumentMut>,
    account_id: &str,
) -> ConfigResult<()> {
    let config: crate::AppConfig = toml::from_str(&global.to_string())?;
    let removed: BTreeSet<String> = config
        .agent_configurations
        .iter()
        .filter(|(_, configuration)| configuration.account == account_id)
        .map(|(id, _)| id.clone())
        .collect();
    if removed.is_empty() {
        return Ok(());
    }

    if let Some(configurations) = global
        .get_mut("agent_configurations")
        .and_then(Item::as_table_mut)
    {
        for id in &removed {
            configurations.remove(id);
        }
        if configurations.is_empty() {
            global.remove("agent_configurations");
        }
    }
    scrub_default_launch(global, &removed);
    for workspace in workspaces.values_mut() {
        scrub_default_launch(workspace, &removed);
        if let Some(roles) = workspace.get_mut("roles").and_then(Item::as_table_mut) {
            for (_, role) in roles.iter_mut() {
                if let Some(role) = role.as_table_mut() {
                    scrub_default_launch_table(role, &removed);
                }
            }
        }
    }
    Ok(())
}

fn scrub_default_launch(doc: &mut DocumentMut, removed: &BTreeSet<String>) {
    scrub_default_launch_table(doc.as_table_mut(), removed);
}

fn scrub_default_launch_table(table: &mut toml_edit::Table, removed: &BTreeSet<String>) {
    if let Some(ids) = table.get_mut("default_launch").and_then(Item::as_array_mut) {
        ids.retain(|id| id.as_str().is_none_or(|id| !removed.contains(id)));
    }
}

fn add_scan_exclusion(doc: &mut DocumentMut, fingerprint: &str) {
    if let Some(exclusions) = doc
        .get_mut("account_scan_exclusions")
        .and_then(Item::as_array_mut)
    {
        if !exclusions
            .iter()
            .any(|value| value.as_str() == Some(fingerprint))
        {
            exclusions.push(fingerprint);
        }
        return;
    }
    let mut exclusions = toml_edit::Array::new();
    exclusions.push(fingerprint);
    doc.insert("account_scan_exclusions", toml_edit::value(exclusions));
}

fn remove_scan_exclusion(doc: &mut DocumentMut, fingerprint: &str) {
    let remove_field = if let Some(exclusions) = doc
        .get_mut("account_scan_exclusions")
        .and_then(Item::as_array_mut)
    {
        exclusions.retain(|value| value.as_str() != Some(fingerprint));
        exclusions.is_empty()
    } else {
        false
    };
    if remove_field {
        doc.remove("account_scan_exclusions");
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
