// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Display;

use super::{
    AllowedRoles, Config, CredentialSource, EnvTable, GatewayEntry, RoleEntry, RoleSource,
    Workspace,
};
use crate::agent::Agent;
use crate::paths::HostPath;
use crate::provider::Provider;
use crate::tomlkit::{key, quote};

fn path_value(path: &HostPath) -> String {
    quote(&path.as_path().to_string_lossy())
}

fn bindings<K: Ord + Copy>(
    map: &BTreeMap<K, impl Display>,
    name: impl Fn(K) -> &'static str,
) -> String {
    let items: Vec<String> = map
        .iter()
        .map(|(slot, account)| format!("{} = {}", name(*slot), quote(&account.to_string())))
        .collect();
    format!("{{ {} }}", items.join(", "))
}

fn credential(source: &CredentialSource) -> String {
    match source {
        CredentialSource::Env { var } => {
            format!("{{ source = \"env\", var = {} }}", quote(var.as_str()))
        }
        CredentialSource::OnePassword {
            reference,
            account: None,
        } => format!(
            "{{ source = \"op\", reference = {} }}",
            quote(reference.as_str())
        ),
        CredentialSource::OnePassword {
            reference,
            account: Some(account),
        } => format!(
            "{{ source = \"op\", reference = {}, account = {} }}",
            quote(reference.as_str()),
            quote(account.as_str())
        ),
        CredentialSource::OmpHost => "{ source = \"omp-host\" }".to_owned(),
        CredentialSource::HostLogin { path } => {
            format!("{{ source = \"host-login\", path = {} }}", path_value(path))
        }
        CredentialSource::File { path } => {
            format!("{{ source = \"file\", path = {} }}", path_value(path))
        }
        CredentialSource::CodexLogin {
            home,
            chatgpt_account_id,
        } => format!(
            "{{ source = \"codex-login\", home = {}, chatgpt_account_id = {} }}",
            path_value(home),
            quote(chatgpt_account_id.as_str())
        ),
    }
}

fn role_source(source: &RoleSource) -> String {
    match source {
        RoleSource::Path(path) => format!("{{ path = {} }}", path_value(path)),
        RoleSource::Git { url, rev: None } => format!("{{ git = {} }}", quote(url.as_str())),
        RoleSource::Git {
            url,
            rev: Some(rev),
        } => format!(
            "{{ git = {}, rev = {} }}",
            quote(url.as_str()),
            quote(rev.as_str())
        ),
    }
}

fn gateway_blocks(name: &str, entry: &GatewayEntry, blocks: &mut Vec<String>) {
    let table = key(name);
    blocks.push(format!(
        "[gateways.{table}]\nopenshell_workspace = {}\nbuilder = {}\nplatform = {}\n",
        quote(entry.openshell_workspace.as_str()),
        quote(entry.builder.as_str()),
        quote(entry.platform.docker_arg()),
    ));
    if let Some(engine) = &entry.docker_engine {
        blocks.push(format!(
            "[gateways.{table}.docker_engine]\nport = {}\nclient_certs = {}\n",
            engine.port,
            path_value(&engine.client_certs),
        ));
    }
}

fn role_block(name: &str, entry: &RoleEntry) -> String {
    let mut block = format!(
        "[roles.{}]\nsource = {}\n",
        key(name),
        role_source(&entry.source)
    );
    if !entry.bindings.is_empty() {
        block.push_str(&format!(
            "bindings = {}\n",
            bindings(&entry.bindings, Agent::as_str)
        ));
    }
    block
}

fn workspace_block(name: &str, workspace: &Workspace) -> String {
    let mut block = format!(
        "[workspaces.{}]\npath = {}\ndocker = {}\n",
        key(name),
        path_value(&workspace.path),
        workspace.docker,
    );
    if let Some(role) = &workspace.default_role {
        block.push_str(&format!("default_role = {}\n", quote(role.as_str())));
    }
    if let AllowedRoles::Only(set) = &workspace.roles {
        let names: Vec<String> = set.iter().map(|role| quote(role.as_str())).collect();
        block.push_str(&format!("roles = [{}]\n", names.join(", ")));
    }
    if !workspace.bindings.is_empty() {
        block.push_str(&format!(
            "bindings = {}\n",
            bindings(&workspace.bindings, Provider::as_str)
        ));
    }
    let table = key(name);
    if !workspace.env.is_empty() {
        block.push_str(&format!(
            "\n[workspaces.{table}.env]\n{}",
            env_lines(&workspace.env)
        ));
    }
    for (role, vars) in &workspace.role_env {
        block.push_str(&format!(
            "\n[workspaces.{table}.role_env.{}]\n{}",
            key(role.as_str()),
            env_lines(vars)
        ));
    }
    block
}

fn env_lines(table: &EnvTable) -> String {
    table
        .iter()
        .map(|(name, value)| format!("{} = {}\n", key(name.as_str()), quote(value.as_str())))
        .collect::<Vec<_>>()
        .concat()
}

impl Config {
    pub fn render(&self) -> String {
        let mut blocks = Vec::new();
        if let Some(name) = &self.default_gateway {
            blocks.push(format!("default_gateway = {}\n", quote(name.as_str())));
        }
        for (name, entry) in &self.gateways {
            gateway_blocks(name.as_str(), entry, &mut blocks);
        }
        if !self.env.is_empty() {
            blocks.push(format!("[env]\n{}", env_lines(&self.env)));
        }
        if !self.defaults.is_empty() {
            let lines: Vec<String> = self
                .defaults
                .iter()
                .map(|(provider, account)| {
                    format!("{} = {}\n", provider.as_str(), quote(account.as_str()))
                })
                .collect();
            blocks.push(format!("[defaults.accounts]\n{}", lines.concat()));
        }
        for account in self.accounts.values() {
            blocks.push(format!(
                "[accounts.{}]\nprovider = {}\ncredential = {}\n",
                key(account.name.as_str()),
                quote(account.provider.as_str()),
                credential(&account.credential),
            ));
        }
        for (name, entry) in &self.roles {
            blocks.push(role_block(name.as_str(), entry));
        }
        for (name, workspace) in &self.workspaces {
            blocks.push(workspace_block(name.as_str(), workspace));
        }
        blocks.join("\n")
    }
}
