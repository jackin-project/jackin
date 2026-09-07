// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Account registry commands; printed output contains metadata only.

use anyhow::{Context, Result, bail};
use jackin_config::{AccountConfig, AccountCredential, AiProvider, AppConfig, ConfigEditor};
use jackin_core::{EnvValue, JackinPaths, WorkspaceName};
use std::io::{IsTerminal, Read};

use crate::cli::account::{AccountCommand, AddAccountArgs, WorkspaceAccountCommand};

pub(super) fn handle(
    command: AccountCommand,
    config: &AppConfig,
    paths: &JackinPaths,
) -> Result<()> {
    match command {
        AccountCommand::List => {
            for (id, account) in &config.accounts {
                println!("{}", account_row(id, account));
            }
            if config.accounts.is_empty() {
                println!("No accounts. Run `jackin account scan` or `jackin account add --help`.");
            }
        }
        AccountCommand::Scan => scan(config, paths)?,
        AccountCommand::Enable { id } => set_enabled(config, paths, &id, true)?,
        AccountCommand::Disable { id } => set_enabled(config, paths, &id, false)?,
        AccountCommand::Default { id, agent } => {
            let mut editor = ConfigEditor::open(paths)?;
            editor.set_account_binding(None, None, agent, Some(&id))?;
            editor.save()?;
            println!("Selected {id} as the {agent} default.");
        }
        AccountCommand::Add(args) => {
            jackin_config::validate_account_id(&args.id)?;
            if config.accounts.contains_key(&args.id) {
                bail!("account {:?} already exists", args.id);
            }
            let account = build_account(&args, paths)?;
            let mut candidate = config.clone();
            candidate.accounts.insert(args.id.clone(), account.clone());
            candidate.validate_accounts()?;
            let mut editor = ConfigEditor::open(paths)?;
            editor.upsert_account(&args.id, &account)?;
            editor.save()?;
            println!("Added {}.", args.id);
        }
        AccountCommand::Remove { id } => {
            if !config.accounts.contains_key(&id) {
                bail!("unknown account {id:?}");
            }
            let mut editor = ConfigEditor::open(paths)?;
            editor.remove_account(&id)?;
            editor.save()?;
            println!("Removed {id} and its assignments.");
        }
    }
    Ok(())
}

fn set_enabled(config: &AppConfig, paths: &JackinPaths, id: &str, enabled: bool) -> Result<()> {
    let mut account = config
        .accounts
        .get(id)
        .with_context(|| format!("unknown account {id:?}"))?
        .clone();
    account.enabled = enabled;
    let mut editor = ConfigEditor::open(paths)?;
    editor.upsert_account(id, &account)?;
    if !enabled {
        for (agent, selected) in &config.account_bindings {
            if selected == id {
                editor.set_account_binding(None, None, *agent, None)?;
            }
        }
    }
    editor.save()?;
    println!("{} {id}.", if enabled { "Enabled" } else { "Disabled" });
    Ok(())
}

fn scan(config: &AppConfig, paths: &JackinPaths) -> Result<()> {
    let report = jackin_config::discover_default_accounts(&paths.home_dir);
    let mut candidate = config.clone();
    let mut editor = ConfigEditor::open(paths)?;
    let mut added = 0;
    for found in report.accounts {
        if candidate.accounts.values().any(|account| matches!(&account.credential, AccountCredential::Profile { agent, directory } if *agent == found.agent && *directory == found.directory)) { continue; }
        let base = format!("default-{}", found.agent);
        let mut id = base.clone();
        let mut suffix = 2;
        while candidate.accounts.contains_key(&id) {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }
        let account = AccountConfig {
            enabled: true,
            name: format!("{} default", found.agent),
            provider: AiProvider::for_agent(found.agent),
            credential: AccountCredential::Profile {
                agent: found.agent,
                directory: found.directory,
            },
        };
        editor.upsert_account(&id, &account)?;
        candidate.accounts.insert(id.clone(), account);
        println!("Imported {id}.");
        added += 1;
    }
    let environment = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    for (provider, variable) in jackin_config::discover_environment_accounts(&environment) {
        let reference = EnvValue::Plain(format!("${variable}"));
        if candidate.accounts.values().any(|account| account.provider == provider && matches!(&account.credential, AccountCredential::ApiKey { value, .. } if *value == reference)) { continue; }
        let base = format!("{}-api-key", provider.slug());
        let mut id = base.clone();
        let mut suffix = 2;
        while candidate.accounts.contains_key(&id) {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }
        let account = AccountConfig {
            enabled: true,
            name: format!("{provider} API key"),
            provider,
            credential: AccountCredential::ApiKey {
                value: reference,
                base_url: None,
                model: None,
            },
        };
        editor.upsert_account(&id, &account)?;
        candidate.accounts.insert(id.clone(), account);
        println!("Imported {id} from {variable}.");
        added += 1;
    }
    for (agent, variable) in jackin_config::discover_environment_oauth_accounts(&environment) {
        let reference = EnvValue::Plain(format!("${variable}"));
        if candidate.accounts.values().any(|account| matches!(&account.credential, AccountCredential::OAuthToken { agent: owner, value } if *owner == agent && *value == reference)) { continue; }
        let base = format!("{agent}-oauth-token");
        let mut id = base.clone();
        let mut suffix = 2;
        while candidate.accounts.contains_key(&id) {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }
        let account = AccountConfig {
            enabled: true,
            name: format!("{agent} OAuth token"),
            provider: AiProvider::for_agent(agent),
            credential: AccountCredential::OAuthToken {
                agent,
                value: reference,
            },
        };
        editor.upsert_account(&id, &account)?;
        candidate.accounts.insert(id.clone(), account);
        println!("Imported {id} from {variable}.");
        added += 1;
    }
    if added > 0 {
        editor.save()?;
    }
    for issue in report.issues {
        eprintln!(
            "{}: {} ({})",
            issue.agent,
            issue.error,
            issue.directory.display()
        );
    }
    println!(
        "Imported {added} account(s). Assign access with `jackin workspace account assign WORKSPACE ACCOUNT`."
    );
    Ok(())
}

fn build_account(args: &AddAccountArgs, paths: &JackinPaths) -> Result<AccountConfig> {
    let provider = args
        .provider
        .as_deref()
        .map(str::parse)
        .transpose()?
        .or_else(|| args.agent.map(AiProvider::for_agent))
        .context("--provider is required")?;
    let credential = if let Some(directory) = &args.directory {
        let agent = args.agent.context("--agent is required for a profile")?;
        let directory = std::path::PathBuf::from(crate::workspace::resolve_path(
            directory
                .to_str()
                .context("account directory must be UTF-8")?,
        ));
        let directory = directory
            .canonicalize()
            .context("account directory cannot be opened")?;
        if !directory.is_dir() {
            bail!("account path must be a directory");
        }
        let found = jackin_config::discover_account_directory(agent, &directory, &paths.home_dir)?;
        if found.is_none() {
            bail!("no {agent} authentication found in {}", directory.display());
        }
        AccountCredential::Profile { agent, directory }
    } else if args.oauth_token {
        let agent = args
            .agent
            .context("--agent is required for an OAuth token")?;
        if agent != jackin_core::Agent::Claude {
            bail!("OAuth tokens are supported only for claude");
        }
        AccountCredential::OAuthToken {
            agent,
            value: read_secret(args)?,
        }
    } else {
        AccountCredential::ApiKey {
            value: read_secret(args)?,
            base_url: args.base_url.clone(),
            model: args.model.clone(),
        }
    };
    Ok(AccountConfig {
        enabled: true,
        name: args.name.clone().unwrap_or_else(|| args.id.clone()),
        provider,
        credential,
    })
}

fn read_secret(args: &AddAccountArgs) -> Result<EnvValue> {
    if let Some(reference) = &args.secret_ref {
        if !valid_secret_reference(reference) {
            bail!(
                "--secret-ref accepts only $VAR, ${{VAR}}, or op:// references; use --stdin for a literal secret"
            );
        }
        return super::config_cmd::resolve_env_value_for_cli(reference, false);
    }
    let secret = if args.stdin {
        if std::io::stdin().is_terminal() {
            bail!("--stdin requires piped input; omit it for a masked prompt");
        }
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(64 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 {
            bail!("credential exceeds 64 KiB");
        }
        String::from_utf8(bytes).context("credential must be UTF-8")?
    } else {
        if !std::io::stdin().is_terminal() {
            bail!("non-interactive account setup requires --stdin or --secret-ref");
        }
        dialoguer::Password::new()
            .with_prompt("Credential")
            .interact()?
    };
    let secret = secret.trim_end_matches(['\r', '\n']);
    if secret.trim().is_empty() {
        bail!("credential cannot be empty");
    }
    Ok(EnvValue::Plain(secret.to_owned()))
}

fn valid_secret_reference(value: &str) -> bool {
    if value.starts_with("op://") {
        return value.len() > 5;
    }
    let Some(variable) = value.strip_prefix('$') else {
        return false;
    };
    let variable = if let Some(braced) = variable.strip_prefix('{') {
        let Some(inner) = braced.strip_suffix('}') else {
            return false;
        };
        inner
    } else {
        variable
    };
    !variable.is_empty()
        && variable.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}

fn account_row(id: &str, account: &AccountConfig) -> String {
    let source = match &account.credential {
        AccountCredential::Profile { agent, directory } => {
            format!("profile:{agent} {}", directory.display())
        }
        AccountCredential::ApiKey { .. } => "api-key".to_owned(),
        AccountCredential::OAuthToken { agent, .. } => format!("oauth-token:{agent}"),
    };
    format!(
        "{id}\t{}\t{}\t{source}\t{}",
        account.name,
        account.provider,
        if account.enabled {
            "enabled"
        } else {
            "disabled"
        }
    )
}

pub(super) fn handle_workspace(
    command: WorkspaceAccountCommand,
    config: &AppConfig,
    paths: &JackinPaths,
) -> Result<()> {
    let workspace = match &command {
        WorkspaceAccountCommand::List { workspace }
        | WorkspaceAccountCommand::Assign { workspace, .. }
        | WorkspaceAccountCommand::Unassign { workspace, .. }
        | WorkspaceAccountCommand::Select { workspace, .. } => WorkspaceName::parse(workspace)?,
    };
    let ws = config.require_workspace(&workspace)?;
    match command {
        WorkspaceAccountCommand::List { .. } => {
            for id in &ws.accounts {
                let account = config
                    .accounts
                    .get(id)
                    .with_context(|| format!("unknown account {id:?}"))?;
                println!("{}", account_row(id, account));
            }
            for (agent, id) in &ws.account_bindings {
                println!("{agent} -> {id}");
            }
            for (role, overrides) in &ws.roles {
                for (agent, id) in &overrides.account_bindings {
                    println!("{role}: {agent} -> {id}");
                }
            }
            if ws.accounts.is_empty() {
                println!("No accounts assigned.");
            }
        }
        WorkspaceAccountCommand::Assign { account, .. } => {
            if !config.accounts.contains_key(&account) {
                bail!("unknown account {account:?}");
            }
            let mut ids = ws.accounts.clone();
            if !ids.contains(&account) {
                ids.push(account.clone());
            }
            let mut editor = ConfigEditor::open(paths)?;
            editor.set_workspace_accounts(&workspace, &ids)?;
            editor.save()?;
            println!("Assigned {account} to {workspace}.");
        }
        WorkspaceAccountCommand::Unassign { account, .. } => {
            let ids = ws
                .accounts
                .iter()
                .filter(|id| **id != account)
                .cloned()
                .collect::<Vec<_>>();
            let mut editor = ConfigEditor::open(paths)?;
            editor.set_workspace_accounts(&workspace, &ids)?;
            editor.save()?;
            println!("Unassigned {account} from {workspace}.");
        }
        WorkspaceAccountCommand::Select {
            account,
            agent,
            role,
            ..
        } => {
            if let Some(role) = &role {
                if !config.roles.contains_key(role) {
                    bail!("unknown role {role:?}");
                }
                if !ws.allowed_roles.is_empty() && !ws.allowed_roles.contains(role) {
                    bail!("role {role:?} is not allowed in {workspace}");
                }
            }
            let mut editor = ConfigEditor::open(paths)?;
            editor.set_account_binding(
                Some(&workspace),
                role.as_deref(),
                agent,
                account.as_deref(),
            )?;
            editor.save()?;
            println!("Updated {agent} account binding for {workspace}.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
