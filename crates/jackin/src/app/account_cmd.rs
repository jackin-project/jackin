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
    startup: &jackin_config::BootstrapReport,
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
        AccountCommand::Scan => {
            scan(paths, startup)?;
        }
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
    if !enabled {
        editor.prune_account_bindings(id)?;
    }
    editor.upsert_account(id, &account)?;
    editor.save()?;
    println!("{} {id}.", if enabled { "Enabled" } else { "Disabled" });
    Ok(())
}

/// Run `account scan`, returning the printed imported count.
///
/// `startup` is the bootstrap report from the process's config load: on a
/// fresh config the load already imported default accounts, and the scan
/// reports them as its own so the first scan prints the true imported
/// count instead of `Imported 0`. Startup issues are deliberately not
/// merged — `scan_for_accounts` re-discovers the same evidence issues, so
/// merging would print each twice.
fn scan(paths: &JackinPaths, startup: &jackin_config::BootstrapReport) -> Result<usize> {
    // One shared scan helper for every surface (CLI, Settings worker):
    // the open consumes any first-run marker, the scan imports default
    // evidence + environment, and the zshrc path seeds shell overrides.
    // Precedence on ID collision is open order — an operator
    // registration always wins and is never overwritten.
    let (mut editor, open_report) = ConfigEditor::open_detailed(paths)?;
    let scan_report = editor.scan_for_accounts()?;
    let zshrc_report = import_zshrc_accounts(&mut editor, paths)?;
    if scan_report.changed || zshrc_report.changed {
        editor.save()?;
    }
    let mut added = startup.added.clone();
    added.extend(open_report.added);
    added.extend(scan_report.added);
    added.extend(zshrc_report.added);
    for (id, account) in &added {
        match scan_reference_variable(account) {
            Some(variable) => println!("Imported {id} from {variable}."),
            None => println!("Imported {id}."),
        }
    }
    for issue in open_report
        .issues
        .into_iter()
        .chain(scan_report.issues)
        .chain(zshrc_report.issues)
    {
        let agent = issue.agent;
        let error = issue.error;
        eprintln!("{agent}: {error} ({})", issue.directory.display());
    }
    let added_count = added.len();
    println!(
        "Imported {added_count} account(s). Assign access with `jackin workspace account assign WORKSPACE ACCOUNT`."
    );
    Ok(added_count)
}

/// `.zshrc`-import path of [`scan`]: parse the operator's shell env (if
/// present), build the typed import plan, and seed accounts into the open
/// editor. Unresolved entries the plan could not consume are reported for
/// operator resolution; a missing `.zshrc` is a silent no-op, an
/// unreadable one a warning (never a scan failure).
fn import_zshrc_accounts(
    editor: &mut ConfigEditor,
    paths: &JackinPaths,
) -> Result<jackin_config::BootstrapReport> {
    let zshrc = paths.home_dir.join(".zshrc");
    let source = match std::fs::read_to_string(&zshrc) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(jackin_config::BootstrapReport::default());
        }
        Err(error) => {
            eprintln!("warning: cannot read {}: {error}", zshrc.display());
            return Ok(jackin_config::BootstrapReport::default());
        }
    };
    let import = jackin_config::parse_zshrc_source(&source);
    let plan = jackin_config::import_plan(&import);
    let report = editor.apply_zshrc_plan(&plan)?;
    let consumed_op_reads: std::collections::BTreeSet<(usize, &str)> = plan
        .op_refs
        .iter()
        .map(|candidate| (candidate.line, candidate.var.as_str()))
        .collect();
    let consumed_wrappers: std::collections::BTreeSet<(usize, &str)> = report
        .unapplied_zshrc_wrappers
        .iter()
        .map(|candidate| (candidate.line, candidate.var.as_str()))
        .collect();
    for entry in &import.unresolved {
        // Machine-consumed `op read` lines need no operator action (seeded,
        // or skipped because the account already exists). Everything else
        // stays visible. Line/name/kind only: the detail snippet can quote
        // shell text, so it never reaches operator output.
        if entry.kind == jackin_config::UnresolvedKind::OpRead
            && consumed_op_reads.contains(&(entry.line, entry.name.as_str()))
        {
            continue;
        }
        if entry.kind == jackin_config::UnresolvedKind::FunctionCall
            && consumed_wrappers.contains(&(entry.line, entry.name.as_str()))
        {
            continue;
        }
        let kind = entry.kind;
        eprintln!(
            "zshrc line {}: {} needs operator resolution ({kind:?})",
            entry.line, entry.name
        );
    }
    for candidate in &plan.op_refs {
        // Parsed but unseedable: no provider home for this variable.
        // Mirrors the apply-side attribution (presence-only probe).
        let probe = std::collections::BTreeMap::from([(candidate.var.clone(), String::from("1"))]);
        let known = !jackin_config::discover_environment_accounts(&probe).is_empty()
            || !jackin_config::discover_environment_oauth_accounts(&probe).is_empty();
        if !known {
            eprintln!(
                "zshrc line {}: {} has no provider mapping; add the account explicitly",
                candidate.line, candidate.var
            );
        }
    }
    for model in &report.unapplied_zshrc_models {
        eprintln!(
            "zshrc: model profile {} was parsed but has no matching persisted API-key account",
            model.name
        );
    }
    for wrapper in &report.unapplied_zshrc_wrappers {
        eprintln!(
            "zshrc line {}: wrapper for {} was parsed but is not supported by the launch path",
            wrapper.line, wrapper.var
        );
    }
    for _ in &report.unapplied_zshrc_xdg_roots {
        eprintln!(
            "zshrc: Amp XDG roots were parsed but no credentials were found under XDG_DATA_HOME/amp"
        );
    }
    Ok(report)
}

/// Extract the environment variable name from a scan-synthesized `$VAR` /
/// `${VAR}` credential reference. Anything else (profiles, 1Password refs,
/// unexpected shapes) yields `None` so the caller prints no suffix — and
/// no secret material or 1Password item ID can ever reach operator output
/// through this path.
fn scan_reference_variable(account: &AccountConfig) -> Option<&str> {
    let persisted = match &account.credential {
        AccountCredential::ApiKey { value, .. } | AccountCredential::OAuthToken { value, .. } => {
            value.as_persisted_str()
        }
        AccountCredential::Profile { .. } => return None,
    };
    let variable = persisted.strip_prefix('$')?;
    let variable = variable
        .strip_prefix('{')
        .and_then(|inner| inner.strip_suffix('}'))
        .unwrap_or(variable);
    (!variable.is_empty()
        && variable.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        }))
    .then_some(variable)
}

fn build_account(args: &AddAccountArgs, paths: &JackinPaths) -> Result<AccountConfig> {
    let provider = args
        .provider
        .as_deref()
        .map(str::parse)
        .transpose()?
        .or_else(|| args.agent.and_then(AiProvider::for_agent))
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
        let found = jackin_config::discover_account_directory(agent, &directory, &paths.home_dir)?
            .with_context(|| {
                format!("no {agent} authentication found in {}", directory.display())
            })?;
        AccountCredential::Profile {
            agent,
            directory,
            xdg_roots: None,
            source_selector: found.source_selector,
        }
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
        AccountCredential::Profile {
            agent, directory, ..
        } => {
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
