//! CLI dispatch: maps parsed `Cli` commands to runtime, console, workspace,
//! and instance calls.
//!
//! `pub async fn run` is the binary entry point after argument parsing. Not a
//! stable library boundary — callers are `main.rs` and tests only.
//!
//! Not responsible for: argument parsing (`cli/`), runtime mechanics
//! (`runtime/`), or TUI rendering (`console/tui/`). This module is glue.

mod config_cmd;
pub mod context;
mod load_cmd;
mod prune_cmd;
mod workspace_cmd;

use anyhow::{Context, Result};

use crate::cli::role::ConsoleArgs;
use crate::cli::{self, Cli, Command};
use crate::config::{self, AppConfig};
use crate::console;
use crate::docker::ShellRunner;
use crate::docker_client::{BollardDockerClient, DockerApi};
use crate::instance;
use crate::paths::JackinPaths;
use crate::runtime;
use crate::selector::RoleSelector;
use crate::tui;
use crate::workspace::{self, LoadWorkspaceInput, WorkspaceConfig, resolve_path};

use self::context::prompt_agent_choice_if_needed;

/// Parse an `auth_forward` mode value as it arrived from the CLI.
fn parse_auth_forward_mode_from_cli(raw: &str) -> anyhow::Result<config::AuthForwardMode> {
    raw.parse().map_err(|e: String| anyhow::anyhow!("{e}"))
}

/// Parse an agent slug as it arrived from the CLI.
fn parse_agent_from_cli(raw: &str) -> anyhow::Result<crate::agent::Agent> {
    raw.parse()
        .map_err(|_| anyhow::anyhow!("unknown agent {raw:?}; expected one of: claude, codex, amp"))
}

fn rich_prelaunch_choice(title: &str, items: Vec<String>) -> anyhow::Result<usize> {
    runtime::progress::prelaunch_select_choice(
        std::env::var_os("JACKIN_NO_MOTION").is_some(),
        title,
        items,
    )
}

async fn play_construct_intro_if_needed(
    paths: &JackinPaths,
    docker: &impl DockerApi,
) -> runtime::EntryClaim {
    let claim = runtime::claim_construct_entry(paths, docker).await;
    if (claim.start_kind() == runtime::StartKind::FreshConstruct
        || runtime::force_boundary_intro_enabled())
        && runtime::progress::rich_terminal_supported()
    {
        // The intro is two screens: the opening phrase/brand screen, then the
        // accelerating warp into the Construct.
        crate::tui::warp_intro();
    }
    claim
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::large_stack_frames)]
pub async fn run(cli: Cli) -> Result<()> {
    let debug = cli.debug;
    tui::set_debug_mode(debug);

    // Resolve the subcommand. Bare `jackin` currently routes to the same
    // console handler as `jackin console`; the TTY-capability fallback and
    // the deprecation warning for `launch` land in a follow-up commit.
    let command = match cli.command {
        Some(cmd) => cmd,
        None => Command::Console(cli.console_args),
    };

    let paths = JackinPaths::detect()?;
    let command_name = command_name(&command);
    let diagnostics = crate::diagnostics::RunDiagnostics::start(&paths, debug, command_name)?;
    let _diagnostics_guard = diagnostics.activate();
    crate::diagnostics::prune_old_runs(&paths);
    if debug {
        announce_debug_run(&diagnostics);
    }
    let command = match command {
        Command::Role(command) => return crate::role_authoring::run(command),
        command => command,
    };
    let mut config = AppConfig::load_or_init(&paths)?;
    let mut runner = ShellRunner { debug };
    let connect_docker = || BollardDockerClient::connect();

    match command {
        Command::Load(args) => {
            load_cmd::handle_load(
                args,
                &mut config,
                &paths,
                debug,
                &mut runner,
                connect_docker,
            )
            .await
        }
        Command::Console(ConsoleArgs {}) => load_cmd::handle_console(config, paths, debug).await,
        Command::Hardline(args) => {
            load_cmd::handle_hardline(args, config, paths, debug, connect_docker).await
        }
        Command::Eject(args) => load_cmd::handle_eject(args, &paths, debug, connect_docker).await,
        Command::Exile => load_cmd::handle_exile(&paths, debug, connect_docker).await,
        Command::Logs(args) => runtime::logs::run(&paths, args),
        Command::Config(config_cmd) => config_cmd::handle(config_cmd, &mut config, &paths, debug),
        Command::Workspace(command) => {
            workspace_cmd::handle(command, &mut config, &paths, debug).await
        }
        Command::Purge(args) => {
            prune_cmd::handle_purge(args, &paths, &mut runner, connect_docker).await
        }
        Command::Prune(cmd) => {
            prune_cmd::handle_prune(cmd, &paths, &mut runner, connect_docker).await
        }
        Command::Help { .. } => {
            // Handled upstream in dispatch before reaching this function.
            unreachable!("Command::Help is dispatched to Action::PrintHelp before run() is called")
        }
        Command::Role(_) => unreachable!("Command::Role returns before config-backed dispatch"),
    }
}

const fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Load(_) => "load",
        Command::Hardline(_) => "hardline",
        Command::Eject(_) => "eject",
        Command::Exile => "exile",
        Command::Purge(_) => "purge",
        Command::Prune(_) => "prune",
        Command::Console(_) => "console",
        Command::Role(_) => "role",
        Command::Workspace(_) => "workspace",
        Command::Config(_) => "config",
        Command::Logs(_) => "logs",
        Command::Help { .. } => "help",
    }
}

/// In `--debug`, surface the diagnostics run id on the plain CLI before
/// anything else runs — never through a rich TUI. This is identical for
/// every command (CLI or TUI): print the run id the operator must keep to
/// retrieve the run's diagnostics file later, then, on an interactive
/// terminal, gate on Enter so the id is read before the normal flow (rich
/// or CLI, per terminal capability) takes over. Debug evidence itself is
/// written only to the run file, never echoed here.
fn announce_debug_run(diagnostics: &crate::diagnostics::RunDiagnostics) {
    use owo_colors::OwoColorize as _;
    use std::io::{IsTerminal, Write};
    let mut err = std::io::stderr();
    let _ = writeln!(err);
    let _ = writeln!(
        err,
        "{} debug mode — save this run id to retrieve the run later:",
        "[jackin]".bold()
    );
    let _ = writeln!(err, "    {}", diagnostics.run_id());
    if std::io::stdin().is_terminal() {
        let _ = write!(err, "[jackin] press Enter to continue... ");
        let _ = err.flush();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
    }
}

fn workspace_env_scope(workspace: String, role: Option<String>) -> config::EnvScope {
    match role {
        Some(a) => config::EnvScope::WorkspaceRole { workspace, role: a },
        None => config::EnvScope::Workspace(workspace),
    }
}

#[allow(clippy::too_many_lines)]
fn handle_claude_token(
    paths: &JackinPaths,
    config: &mut AppConfig,
    action: cli::WorkspaceClaudeTokenCommand,
) -> Result<()> {
    use crate::operator_env::OpRef;
    use crate::workspace::token_setup;

    fn parse_reuse(input: &str, account: Option<&str>) -> Result<OpRef> {
        if !input.starts_with("op://") {
            anyhow::bail!(
                "--reuse expects an op:// reference (got {input:?}); see \
                 https://developer.1password.com/docs/cli/secret-reference-syntax/"
            );
        }
        // Canonicalise via the same disambiguation path the picker
        // uses. Thread the operator's `--op-account` into every
        // underlying `op vault list` / `item list` / `item get` query
        // so multi-1P-account operators resolve `--reuse` against the
        // pinned account; otherwise a coincidentally-named item in
        // the default account could swap in for the intended one.
        let probe = crate::operator_env::OpCli::new();
        crate::operator_env::resolve_op_uri_to_ref(input, &probe, account)
    }

    match action {
        cli::WorkspaceClaudeTokenCommand::Setup {
            workspace,
            role,
            vault,
            item_name,
            op_account,
            reuse,
            plain,
            interactive,
        } => {
            config.require_workspace(&workspace)?;

            // Interactive mode: walk the operator through 1Password with
            // plain CLI prompts (account → vault → item → field) when
            // --interactive is set. The rich TUI drill-down lives only in
            // `jackin console`; the CLI stays CLI.
            let (args, role) = if interactive {
                // --role flag wins; otherwise prompt for the scope so the
                // interactive path selects everything.
                let role = match role {
                    Some(r) => Some(r),
                    None => prompt_interactive_role(config, &workspace)?,
                };
                let args = match prompt_interactive_token_source()? {
                    InteractiveTokenSource::Plain => token_setup::TokenSetupArgs {
                        account: op_account,
                        plain_text: true,
                        ..Default::default()
                    },
                    InteractiveTokenSource::Op => {
                        prompt_interactive_token_store(&workspace, op_account)?
                    }
                };
                (args, role)
            } else if plain {
                let args = token_setup::TokenSetupArgs {
                    account: op_account,
                    plain_text: true,
                    ..Default::default()
                };
                (args, role)
            } else {
                let reuse_ref = reuse
                    .as_deref()
                    .map(|r| parse_reuse(r, op_account.as_deref()))
                    .transpose()?;
                let args = token_setup::TokenSetupArgs {
                    vault,
                    item_name,
                    account: op_account,
                    reuse: reuse_ref,
                    field_label: None,
                    edit_existing: None,
                    section: None,
                    plain_text: false,
                };
                (args, role)
            };

            // A flag-supplied role is taken verbatim, so reject one the
            // workspace doesn't allow before minting — otherwise the OAuth
            // round-trip runs and wires a token to a dead role scope. The
            // interactive prompt already only offers allowed roles.
            if let Some(role) = role.as_deref() {
                validate_setup_role_allowed(config, &workspace, role)?;
            }
            let scope = match role {
                Some(role) => token_setup::TokenSetupScope::WorkspaceRole { workspace, role },
                None => token_setup::TokenSetupScope::Workspace(workspace),
            };
            let report = token_setup::run_setup(paths, config, &scope, &args)?;
            print_token_setup_report(&report);
            Ok(())
        }
        cli::WorkspaceClaudeTokenCommand::Rotate {
            workspace,
            role,
            vault,
            item_name,
            op_account,
        } => {
            // Reject a disallowed flag-supplied role before minting, same
            // as setup — otherwise rotate wires a token to a dead scope.
            if let Some(role) = role.as_deref() {
                validate_setup_role_allowed(config, &workspace, role)?;
            }
            let scope = match role {
                Some(role) => token_setup::TokenSetupScope::WorkspaceRole { workspace, role },
                None => token_setup::TokenSetupScope::Workspace(workspace),
            };
            // Read the prior token from the SAME scope being rotated so a
            // role-scoped token (wired by `setup --role`) is found and its
            // vault/op-item are reused, not the workspace-level slot.
            let prior = token_setup::prior_token_slot(config, &scope);
            // Default rotate to the prior item's vault when
            // `--vault` is not supplied. Without this, the
            // documented `rotate my-app` form errors inside
            // `create_op_item` AFTER the PTY token capture
            // completes. See [`token_setup::vault_for_rotate`].
            let derived_vault = token_setup::vault_for_rotate(vault, prior.as_ref());
            let args = token_setup::TokenSetupArgs {
                vault: derived_vault,
                item_name,
                account: op_account,
                reuse: None,
                field_label: None,
                edit_existing: None,
                section: None,
                plain_text: false,
            };
            let report = token_setup::run_setup(paths, config, &scope, &args)?;
            print_token_setup_report(&report);
            // Rotate is an op-only flow (it always mints into a new 1P
            // item), so the report always carries an op ref here.
            let new_ref = report
                .op_ref
                .as_ref()
                .expect("rotate always wires an op reference");
            delete_prior_op_item(prior, new_ref, report.op_account)?;
            Ok(())
        }
        cli::WorkspaceClaudeTokenCommand::Revoke {
            workspace,
            delete_op_item,
        } => {
            let report = token_setup::run_revoke(paths, config, &workspace, delete_op_item)?;
            if report.cleared_slot {
                println!(
                    "Cleared canonical slot for workspace {:?}.",
                    report.workspace
                );
            } else {
                println!(
                    "Workspace {:?} had no canonical slot — config left unchanged.",
                    report.workspace
                );
            }
            if report.deleted_op_item {
                println!("Deleted referenced 1P item.");
            }
            Ok(())
        }
        cli::WorkspaceClaudeTokenCommand::Doctor { workspace } => {
            let report = token_setup::run_doctor(config, &workspace)?;
            println!("workspace        {}", report.workspace);
            println!("auth_forward     {}", report.mode);
            println!(
                "op account       {}",
                report.op_account.as_deref().unwrap_or("(default)")
            );
            if let Some(r) = &report.op_ref {
                println!("op_ref           {}", r.path);
            } else {
                println!("op_ref           (literal slot)");
            }
            println!(
                "token sha256     {}… (12 hex prefix; matches stored value)",
                report.token_sha256_prefix
            );
            Ok(())
        }
    }
}

/// Delete the previous 1P item after a rotate succeeded.
///
/// A delete failure promotes the whole rotate to an `Err` so
/// exit-code-driven automation (CI, `set -e`) sees the orphan —
/// silent swallowing would let the vault accumulate dangling tokens,
/// and on a credential-revocation rotation the old token would
/// remain live in 1P.
fn delete_prior_op_item(
    prior: Option<crate::operator_env::EnvValue>,
    new_ref: &crate::operator_env::OpRef,
    account: Option<String>,
) -> Result<()> {
    let op_cli = crate::operator_env::OpCli::new().with_account(account);
    delete_prior_op_item_with_runner(prior, new_ref, &op_cli)
}

fn delete_prior_op_item_with_runner(
    prior: Option<crate::operator_env::EnvValue>,
    new_ref: &crate::operator_env::OpRef,
    op_writer: &dyn crate::operator_env::OpWriteRunner,
) -> Result<()> {
    let Some(crate::operator_env::EnvValue::OpRef(prior_ref)) = prior else {
        return Ok(());
    };
    if prior_ref.op == new_ref.op {
        eprintln!(
            "[jackin] rotate: new op-ref matches prior — this is unexpected for a successful \
             rotate (`item_create` should always produce a new item id). The new token may not \
             be the freshly-captured one. Re-run `claude-token doctor` to verify."
        );
        return Ok(());
    }
    let Some(parts) = crate::operator_env::parse_op_reference(&prior_ref.op) else {
        eprintln!(
            "[jackin] rotate: prior slot {path:?} ({op}) is not in UUID form; \
             delete by hand if desired",
            path = prior_ref.path,
            op = prior_ref.op,
        );
        return Ok(());
    };
    // Only delete an item jackin created. An item the operator adopted via
    // `--reuse` or interactive edit-in-place carries no jackin tag and may
    // hold the operator's other fields, so deleting the whole item would be
    // data loss — leave it. Fail safe on a read error: don't delete what we
    // can't verify.
    match op_writer.item_tags(&parts.item, &parts.vault, prior_ref.account.as_deref()) {
        Ok(tags) if crate::workspace::token_setup::tags_indicate_jackin_owned(&tags) => {}
        Ok(_) => {
            eprintln!(
                "[jackin] rotate: prior item ({path}) is not jackin-managed (no `{tag}` tag) — \
                 leaving it untouched so none of your other fields are deleted. The new token is \
                 wired and live; remove the old field by hand if you want: `{hint}`",
                path = prior_ref.path,
                tag = crate::workspace::token_setup::JACKIN_TAG,
                hint = parts.manual_delete_hint(),
            );
            return Ok(());
        }
        Err(e) => {
            eprintln!(
                "[jackin] rotate: could not verify whether prior item ({path}) is jackin-managed: \
                 {e} — leaving it untouched to avoid deleting an adopted item. Delete by hand if \
                 needed: `{hint}`",
                path = prior_ref.path,
                hint = parts.manual_delete_hint(),
            );
            return Ok(());
        }
    }
    op_writer
        // The prior item lives in the account that minted it, which can
        // differ from the new ref's account on a cross-account rotate.
        // Pinning the delete to the new account would orphan it.
        .item_delete(&parts.item, &parts.vault, prior_ref.account.as_deref())
        .map_err(|e| {
            anyhow::anyhow!(
                "rotate: prior item ({path}) was NOT deleted: {e} \
                 (delete by hand: `{hint}`)",
                path = prior_ref.path,
                hint = parts.manual_delete_hint(),
            )
        })?;
    eprintln!("Deleted prior 1P item ({}).", prior_ref.path);
    Ok(())
}

fn print_token_setup_report(report: &crate::workspace::token_setup::TokenSetupReport) {
    println!();
    println!("Workspace        {}", report.workspace);
    if let Some(version) = report.claude_cli_version.as_deref() {
        println!("Claude CLI       {version}");
    }
    if let Some(op_ref) = report.op_ref.as_ref() {
        println!("op:// reference  {}", op_ref.path);
        println!(
            "op account       {}",
            report.op_account.as_deref().unwrap_or("(default)")
        );
    } else {
        println!("stored           plain text in workspace/role config");
    }
    println!(
        "token sha256     {}… (12 hex prefix; matches stored value)",
        report.token_sha256_prefix
    );
    if let Some(expiry) = report.expiry_estimate.as_deref() {
        println!("expires (est.)   {expiry}");
    }
    println!("auth_forward     oauth_token (synthesised CLAUDE_CODE_OAUTH_TOKEN)");
    println!();
    if report.op_ref.is_none() {
        println!("New token captured and stored as a literal in config.");
    } else if report.created {
        println!("New token captured and stored in 1Password.");
    } else {
        println!("Existing op:// reference adopted; no new item created.");
    }
}

/// Reject a flag-supplied `--role` the workspace does not allow, before
/// any token mint runs. Empty `allowed_roles` is the "any role" shorthand.
fn validate_setup_role_allowed(config: &AppConfig, workspace: &str, role: &str) -> Result<()> {
    use jackin_console::workspace::agent_is_effectively_allowed;
    let ws = config.require_workspace(workspace)?;
    if !agent_is_effectively_allowed(ws, role) {
        anyhow::bail!(
            "role {role:?} is not allowed in workspace {workspace:?}; allowed roles: {}",
            ws.allowed_roles.join(", ")
        );
    }
    Ok(())
}

/// Prompt for the token scope: all roles in the workspace, or a specific
/// role override. Returns `None` for the workspace level (all roles) and
/// `Some(role)` for a per-role override. Falls back to `None` when stdin
/// is not a TTY or the workspace has no allowed roles to scope to.
fn prompt_interactive_role(config: &AppConfig, workspace: &str) -> Result<Option<String>> {
    use std::io::IsTerminal;

    let roles: Vec<String> = config
        .workspaces
        .get(workspace)
        .map(|w| w.allowed_roles.clone())
        .unwrap_or_default();
    if roles.is_empty() || !std::io::stdin().is_terminal() {
        return Ok(None);
    }

    let mut labels: Vec<String> = vec!["All roles (workspace level)".to_string()];
    labels.extend(roles.iter().cloned());
    let idx = dialoguer::Select::new()
        .with_prompt("Scope")
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(if idx == 0 {
        None
    } else {
        Some(roles[idx - 1].clone())
    })
}

/// Storage target chosen at the top of the `--interactive` flow.
enum InteractiveTokenSource {
    /// Store the minted token as a literal in config.
    Plain,
    /// Walk the 1Password account → vault → item → field drill-down.
    Op,
}

/// First `--interactive` step: pick where the minted token is stored.
/// Plain-text skips the 1Password account/vault/item/field prompts
/// entirely.
fn prompt_interactive_token_source() -> Result<InteractiveTokenSource> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "--interactive requires a TTY. Pass --vault <name-or-uuid> or --plain for \
             non-interactive use."
        );
    }
    let idx = dialoguer::Select::new()
        .with_prompt("Store token in")
        .items(["Plain text", "1Password"])
        .default(0)
        .interact()?;
    Ok(if idx == 0 {
        InteractiveTokenSource::Plain
    } else {
        InteractiveTokenSource::Op
    })
}

/// Plain-CLI interactive selection of where a Claude token should land in
/// 1Password. Walks account → vault → item → field with `dialoguer`
/// prompts and returns the [`TokenSetupArgs`] the orchestrator runs. No
/// rich TUI — the console owns that surface; the CLI stays CLI.
fn prompt_interactive_token_store(
    workspace: &str,
    op_account: Option<String>,
) -> Result<crate::workspace::token_setup::TokenSetupArgs> {
    use crate::operator_env::{OpCli, OpStructRunner};
    use crate::workspace::token_setup;
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "--interactive requires a TTY. Pass --vault <name-or-uuid> for non-interactive use."
        );
    }

    let accounts = OpCli::new_interactive()
        .with_account(op_account.clone())
        .account_list()?;
    if accounts.is_empty() {
        anyhow::bail!("1Password CLI is not signed in. Run `op signin` in your shell, then retry.");
    }

    // An explicit --op-account wins; otherwise prompt only when ambiguous.
    let account_id: Option<String> = if op_account.is_some() {
        op_account
    } else if accounts.len() == 1 {
        Some(accounts[0].id.clone())
    } else {
        let labels: Vec<String> = accounts
            .iter()
            .map(|a| format!("{}  ({})", a.email, a.url))
            .collect();
        let idx = dialoguer::Select::new()
            .with_prompt("1Password account")
            .items(&labels)
            .default(0)
            .interact()?;
        Some(accounts[idx].id.clone())
    };

    let op = OpCli::new_interactive().with_account(account_id.clone());

    let vaults = op.vault_list(account_id.as_deref())?;
    if vaults.is_empty() {
        anyhow::bail!("No 1Password vaults available for this account.");
    }
    let vault_labels: Vec<&str> = vaults.iter().map(|v| v.name.as_str()).collect();
    let vault = &vaults[dialoguer::Select::new()
        .with_prompt("Vault")
        .items(&vault_labels)
        .default(0)
        .interact()?];

    let items = op.item_list(&vault.id, account_id.as_deref())?;
    let mut item_labels: Vec<String> = vec!["[ + New item ]".to_string()];
    item_labels.extend(items.iter().map(|i| {
        if i.subtitle.is_empty() {
            i.name.clone()
        } else {
            format!("{} ({})", i.name, i.subtitle)
        }
    }));
    let item_choice = dialoguer::Select::new()
        .with_prompt("Item")
        .items(&item_labels)
        .default(0)
        .interact()?;

    if item_choice == 0 {
        let default_name = token_setup::DEFAULT_ITEM_TEMPLATE.replace("{ws}", workspace);
        let item_name: String = dialoguer::Input::new()
            .with_prompt("New item name")
            .default(default_name)
            .interact_text()?;
        let field_label: String = dialoguer::Input::new()
            .with_prompt("Field label")
            .default(token_setup::DEFAULT_FIELD_LABEL.to_string())
            .interact_text()?;
        // Trim so padding can't reach the op item title / field id+label,
        // matching the TUI picker's commit trimming.
        return Ok(token_setup::TokenSetupArgs {
            vault: Some(vault.id.clone()),
            item_name: Some(item_name.trim().to_string()),
            account: account_id,
            reuse: None,
            field_label: Some(field_label.trim().to_string()),
            edit_existing: None,
            section: None,
            plain_text: false,
        });
    }

    let item = &items[item_choice - 1];
    let (section, field) =
        prompt_existing_item_section_and_field(&op, account_id.as_deref(), &vault.id, &item.id)?;

    Ok(token_setup::TokenSetupArgs {
        vault: None,
        item_name: None,
        account: account_id,
        reuse: None,
        field_label: None,
        edit_existing: Some(token_setup::EditExistingTarget {
            vault_id: vault.id.clone(),
            item_id: item.id.clone(),
            field,
            section,
        }),
        section: None,
        plain_text: false,
    })
}

/// Prompt for which section and field of an existing 1Password item the
/// token should land in. Mirrors the TUI Create-mode drill: first pick a
/// section (`(root)`, an existing named section, or `[ + New section ]`),
/// then pick a field scoped to that section (an existing field to
/// overwrite, or `[ + New field ]` to append). Returns the chosen section
/// (`None` for `(root)`) and a [`FieldTarget`] — `Existing` when an
/// existing field was picked (so the write targets that exact field and
/// preserves its placement), `New` for an appended field.
fn prompt_existing_item_section_and_field(
    op: &crate::operator_env::OpCli,
    account_id: Option<&str>,
    vault_id: &str,
    item_id: &str,
) -> Result<(Option<String>, crate::operator_env::FieldTarget)> {
    use crate::operator_env::{FieldTarget, OpStructRunner, parse_op_reference};
    use crate::workspace::token_setup;

    let fields = op.item_get(item_id, vault_id, account_id)?;

    // Distinct sections in first-appearance order; `None` is `(root)`.
    let mut sections: Vec<Option<String>> = vec![None];
    for f in &fields {
        if let Some(name) = parse_op_reference(&f.reference).and_then(|p| p.section)
            && !sections.iter().any(|s| s.as_deref() == Some(name.as_str()))
        {
            sections.push(Some(name));
        }
    }

    let mut section_labels: Vec<String> = sections
        .iter()
        .map(|s| s.clone().unwrap_or_else(|| "(root)".to_string()))
        .collect();
    section_labels.push("[ + New section ]".to_string());

    let section_choice = dialoguer::Select::new()
        .with_prompt("Section")
        .items(&section_labels)
        .default(0)
        .interact()?;

    let section: Option<String> = if section_choice == sections.len() {
        let name: String = dialoguer::Input::new()
            .with_prompt("New section name")
            .interact_text()?;
        // Trim to match the TUI picker so padding can't reach the section label.
        Some(name.trim().to_string())
    } else {
        sections[section_choice].clone()
    };

    // Fields scoped to the chosen section.
    let scoped: Vec<&crate::operator_env::OpField> = fields
        .iter()
        .filter(|f| {
            parse_op_reference(&f.reference)
                .and_then(|p| p.section)
                .as_deref()
                == section.as_deref()
        })
        .collect();

    let mut field_labels: Vec<String> = vec!["[ + New field ]".to_string()];
    field_labels.extend(scoped.iter().map(|f| {
        let label = if f.label.is_empty() { &f.id } else { &f.label };
        let kind = if f.concealed {
            "concealed".to_string()
        } else {
            f.field_type.to_lowercase()
        };
        format!("{label}  ({kind})")
    }));
    let field_choice = dialoguer::Select::new()
        .with_prompt("Field")
        .items(&field_labels)
        .default(0)
        .interact()?;

    if field_choice == 0 {
        let field_label: String = dialoguer::Input::new()
            .with_prompt("New field label")
            .default(token_setup::DEFAULT_FIELD_LABEL.to_string())
            .interact_text()?;
        return Ok((
            section,
            FieldTarget::New {
                label: field_label.trim().to_string(),
            },
        ));
    }
    let f = scoped[field_choice - 1];
    let label = if f.label.is_empty() {
        f.id.clone()
    } else {
        f.label.clone()
    };
    Ok((
        section,
        FieldTarget::Existing {
            id: f.id.clone(),
            label,
        },
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HardlineAction {
    Reconnect,
    NewSession,
    Inspect,
    Cancel,
}

fn prompt_hardline_action(container: &str) -> Result<HardlineAction> {
    prompt_hardline_action_with_prompt(&format!(
        "Instance `{container}` is available. Choose hardline action:"
    ))
}

async fn prompt_explicit_hardline_action_if_multiple_sessions(
    container: &str,
    docker: &impl DockerApi,
) -> Result<HardlineAction> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        return Ok(HardlineAction::Reconnect);
    }
    let state = docker.inspect_container_state(container).await;
    let sessions = runtime::inspect_agent_sessions(docker, container, &state).await;
    if !has_multiple_agent_sessions(&sessions) {
        return Ok(HardlineAction::Reconnect);
    }
    prompt_hardline_action_with_prompt(&format!(
        "Instance `{}` has multiple detected agent sessions ({}). Docker can reconnect the original container TTY or start another foreground session. Choose hardline action:",
        container,
        runtime::describe_agent_session_count(&sessions)
    ))
}

const fn has_multiple_agent_sessions(sessions: &runtime::AgentSessionInventory) -> bool {
    matches!(sessions, runtime::AgentSessionInventory::Sessions(items) if items.len() > 1)
}

fn prompt_hardline_action_with_prompt(prompt: &str) -> Result<HardlineAction> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        return Ok(HardlineAction::Reconnect);
    }

    let options = hardline_action_options();
    let labels: Vec<&str> = options.iter().map(|(label, _)| *label).collect();
    let choice = tui::prompt_choice(prompt, &labels)?;
    Ok(options[choice].1)
}

/// Pick the agent for a new foreground session inside an existing
/// instance, mirroring the `load` / `hardline --new` resolution order:
/// workspace `default_agent` short-circuits the prompt; otherwise
/// `prompt_agent_choice_if_needed` offers the manifest's supported
/// agents; on non-TTY or single-agent roles, fall back to the
/// workspace default or the manifest's recorded agent.
fn resolve_new_session_agent(
    paths: &JackinPaths,
    config: &AppConfig,
    manifest: &instance::InstanceManifest,
) -> Result<crate::agent::Agent> {
    let class = RoleSelector::parse(&manifest.role_key)?;
    let workspace_default_agent = manifest
        .workspace_name
        .as_deref()
        .and_then(|name| config.workspaces.get(name))
        .and_then(|ws| ws.default_agent);
    // Prompt declined to ask → workspace default covers it, role is
    // single-agent, or non-TTY context. Prefer the workspace default;
    // fall back to the manifest's recorded agent.
    prompt_agent_choice_if_needed(paths, &class, workspace_default_agent)?.map_or_else(
        || workspace_default_agent.map_or_else(|| manifest.agent(), Ok),
        Ok,
    )
}

/// Bridge from the TUI event loop to async docker work for Stop/Purge.
/// Now that `run_in_place` is async, the work runs directly on the
/// existing Tokio runtime — no nested runtime or OS thread needed.
struct ConsoleInPlaceHandler {
    paths: JackinPaths,
    debug: bool,
}

impl console::InstanceActionHandler for ConsoleInPlaceHandler {
    async fn run_in_place(
        &mut self,
        container: &str,
        action: console::ConsoleInstanceAction,
    ) -> anyhow::Result<()> {
        let docker = BollardDockerClient::connect()?;
        let mut runner = ShellRunner { debug: self.debug };
        // Wrap the eject + post-condition work in an async block so a
        // partial failure still hits the trailing reconcile +
        // manifest-status update. Without this, an eject that errored
        // after removing the last keep-awake container would leave
        // caffeinate asserted on the host and the on-disk manifest
        // stuck at Active/Running while the container is half-gone.
        let result: anyhow::Result<()> = async {
            match action {
                console::ConsoleInstanceAction::Stop => {
                    runtime::eject_role(&self.paths, container, &docker).await
                }
                console::ConsoleInstanceAction::Purge => {
                    runtime::eject_role(&self.paths, container, &docker).await?;
                    runtime::purge_container_state(&self.paths, container, &docker, &mut runner)
                        .await
                }
                _ => Ok(()),
            }
        }
        .await;
        if matches!(action, console::ConsoleInstanceAction::Stop) {
            mark_instance_restore_available_after_stop(
                &self.paths,
                container,
                &docker,
                result.is_ok(),
            )
            .await;
        }
        runtime::reconcile_keep_awake(&self.paths, &docker, &mut runner).await;
        result
    }
}

/// Promote the manifest for `container` to `RestoreAvailable` so the
/// console list reflects "stopped, recoverable on demand" instead of the
/// stale `Active` / `Running` that `eject_role` would otherwise leave
/// behind (eject removes Docker resources but writes nothing to the
/// on-disk index). Logs and proceeds on error — the eject itself
/// succeeded and a stale row is recoverable on next interaction with
/// the container.
fn mark_instance_restore_available(paths: &JackinPaths, container: &str) {
    let state_dir = paths.data_dir.join(container);
    match instance::InstanceManifest::read(&state_dir) {
        Ok(mut manifest) => {
            if let Err(e) = manifest.mark_restore_available(paths) {
                eprintln!("[jackin] failed to mark instance {container} as RestoreAvailable: {e}");
            }
        }
        Err(e) => {
            eprintln!("[jackin] cannot update instance manifest for {container} after stop: {e}");
        }
    }
}

async fn mark_instance_restore_available_after_stop(
    paths: &JackinPaths,
    container: &str,
    docker: &impl DockerApi,
    stop_succeeded: bool,
) {
    if stop_succeeded {
        mark_instance_restore_available(paths, container);
        return;
    }

    if matches!(
        docker.inspect_container_state(container).await,
        runtime::ContainerState::NotFound
    ) {
        mark_instance_restore_available(paths, container);
    }
}

async fn handle_console_instance_action(
    paths: &JackinPaths,
    config: &mut AppConfig,
    outcome: console::ConsoleOutcome,
    docker: &impl DockerApi,
    runner: &mut ShellRunner,
) -> Result<()> {
    let console::ConsoleOutcome::InstanceAction { container, action } = outcome else {
        unreachable!("console launch outcomes are handled before instance actions")
    };
    match action {
        console::ConsoleInstanceAction::Reconnect => {
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            let result = if let Some(manifest) =
                restore_candidate_for_hardline(paths, &container, docker).await?
            {
                restore_hardline_instance(paths, config, &manifest, docker, runner).await
            } else {
                runtime::hardline_agent(paths, &container, docker, runner).await
            };
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            result
        }
        console::ConsoleInstanceAction::ReconnectFocus(session_id) => {
            // Same as `Reconnect` but forwards a pane-focus id to the
            // daemon. Only fires for running instances reachable via
            // the bind-mounted socket — `restore_hardline_instance`
            // (cold-restore path) does not surface the snapshot
            // preview that produces a focus id, so we route directly
            // through the focused hardline.
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            let result = runtime::hardline_agent_with_focus(
                paths,
                &container,
                Some(session_id),
                docker,
                runner,
            )
            .await;
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            result
        }
        console::ConsoleInstanceAction::NewSession
        | console::ConsoleInstanceAction::NewSessionWithAgent(_) => {
            let manifest = instance::InstanceManifest::read(&paths.data_dir.join(&container))
                .with_context(|| {
                    format!(
                        "cannot start a new agent session in `{container}` because its instance manifest is missing"
                    )
                })?;
            let selected_agent =
                if let console::ConsoleInstanceAction::NewSessionWithAgent(agent) = action {
                    agent
                } else {
                    resolve_new_session_agent(paths, config, &manifest)?
                };
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            let result = runtime::spawn_agent_session(
                paths,
                &container,
                Some(&manifest),
                selected_agent,
                None,
                &[],
                config.git.coauthor_trailer,
                config.git.dco,
                docker,
                runner,
            )
            .await;
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            result
        }
        console::ConsoleInstanceAction::Shell => {
            runtime::spawn_shell_session(paths, &container, docker, runner).await
        }
        console::ConsoleInstanceAction::Inspect => {
            println!(
                "{}",
                runtime::inspect_hardline_instance(paths, &container, docker).await?
            );
            Ok(())
        }
        // Stop and Purge are dispatched via `ConsoleInPlaceHandler::run_in_place`
        // (see `console::ConsoleInstanceAction::runs_in_place`), so
        // the console event loop never returns
        // `ConsoleOutcome::InstanceAction` for them. Bail with a
        // diagnostic — `unreachable!` would panic in a future caller
        // that bypasses the runs_in_place gate; bail surfaces the
        // dispatch bug without taking the process down.
        console::ConsoleInstanceAction::Stop | console::ConsoleInstanceAction::Purge => {
            anyhow::bail!(
                "{action:?} must run via ConsoleInPlaceHandler::run_in_place; reached handle_console_instance_action by mistake"
            )
        }
    }
}

const fn hardline_action_options() -> [(&'static str, HardlineAction); 4] {
    [
        (
            "Reconnect or recover this instance",
            HardlineAction::Reconnect,
        ),
        (
            "Start another foreground agent session",
            HardlineAction::NewSession,
        ),
        ("Inspect state without attaching", HardlineAction::Inspect),
        ("Cancel", HardlineAction::Cancel),
    ]
}

async fn resolve_role_to_container(
    class: &RoleSelector,
    docker: &impl DockerApi,
) -> Result<String> {
    let candidates =
        runtime::matching_family(class, &runtime::list_managed_role_names(docker).await?);
    match candidates.len() {
        1 => Ok(candidates.into_iter().next().unwrap()),
        0 => anyhow::bail!("no managed container found for role `{}`", class.key()),
        _ => anyhow::bail!(
            "multiple containers found for role `{}`: {}; pass a specific container name",
            class.key(),
            candidates.join(", ")
        ),
    }
}

fn resolve_instance_reference(paths: &JackinPaths, input: &str) -> Result<Option<String>> {
    let index = instance::InstanceIndex::read_or_rebuild(&paths.data_dir)?;
    let mut matches = Vec::new();
    for entry in index.instances {
        if entry.status == instance::InstanceStatus::Purged {
            continue;
        }
        if entry.container_base == input || entry.instance_id == input {
            matches.push(entry.container_base);
        }
    }
    matches.sort();
    matches.dedup();

    match matches.as_slice() {
        [] => Ok(None),
        [container] => Ok(Some(container.clone())),
        _ => anyhow::bail!(
            "instance reference {input:?} is ambiguous; pass the full container name instead"
        ),
    }
}

async fn restore_candidate_for_hardline(
    paths: &JackinPaths,
    container: &str,
    docker: &impl DockerApi,
) -> Result<Option<instance::InstanceManifest>> {
    let state_dir = paths.data_dir.join(container);
    let Some(mut manifest) = instance::InstanceManifest::read_optional(&state_dir)? else {
        return Ok(None);
    };
    if !manifest.is_restore_candidate() {
        return Ok(None);
    }

    match docker.inspect_container_state(container).await {
        runtime::ContainerState::NotFound => {
            manifest.mark_restore_available(paths)?;
            Ok(Some(manifest))
        }
        runtime::ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!(
                "{}",
                runtime::docker_unavailable_msg(
                    &format!("inspect container `{container}`"),
                    &reason,
                )
            );
        }
        runtime::ContainerState::Running
        | runtime::ContainerState::Paused
        | runtime::ContainerState::Restarting
        | runtime::ContainerState::Created
        | runtime::ContainerState::Removing
        | runtime::ContainerState::Dead
        | runtime::ContainerState::Stopped { .. } => Ok(None),
    }
}

async fn restore_hardline_instance(
    paths: &JackinPaths,
    config: &mut AppConfig,
    manifest: &instance::InstanceManifest,
    docker: &impl DockerApi,
    runner: &mut impl crate::docker::CommandRunner,
) -> Result<()> {
    let class = RoleSelector::parse(&manifest.role_key)?;
    let cwd = std::env::current_dir()?;
    let workspace = if let Some(workspace_name) = manifest.workspace_name.as_ref() {
        workspace::resolve_load_workspace(
            config,
            &class,
            &cwd,
            LoadWorkspaceInput::Saved(workspace_name.clone()),
            &[],
        )?
    } else {
        let input = resolve_ad_hoc_restore_input(manifest, &cwd)?;
        workspace::resolve_load_workspace(config, &class, &cwd, input, &[])?
    };

    let opts = runtime::LoadOptions {
        agent: Some(manifest.agent()?),
        role_branch: manifest.role_source_ref.clone(),
        restore_container_base: Some(manifest.container_base.clone()),
        restore_role_source_git: Some(manifest.role_source_git.clone()),
        ..runtime::LoadOptions::default()
    };
    runtime::load_role(paths, config, &class, &workspace, docker, runner, &opts).await
}

fn resolve_ad_hoc_restore_input(
    manifest: &instance::InstanceManifest,
    cwd: &std::path::Path,
) -> Result<LoadWorkspaceInput> {
    let cwd = cwd.canonicalize()?;
    if ad_hoc_restore_input_for_current_dir(manifest, &cwd, false).is_some() {
        return Ok(LoadWorkspaceInput::CurrentDir);
    }
    if let Some(path) = prompt_moved_ad_hoc_project_path(manifest, &cwd)? {
        return ad_hoc_restore_input_for_moved_path(manifest, &path).with_context(|| {
            format!(
                "cannot restore ad-hoc instance `{}` from {}",
                manifest.container_base,
                path.display()
            )
        });
    }
    anyhow::bail!(
        "cannot restore ad-hoc instance `{}` from {}; rerun `jackin hardline {}` from its original project directory, select the moved project path interactively, or use `jackin eject {} --purge` to discard it",
        manifest.container_base,
        cwd.display(),
        manifest.container_base,
        manifest.container_base
    )
}

fn ad_hoc_restore_input_for_current_dir(
    manifest: &instance::InstanceManifest,
    cwd: &std::path::Path,
    allow_moved: bool,
) -> Option<LoadWorkspaceInput> {
    let cwd_str = cwd.display().to_string();
    let cwd_fingerprint = instance::manifest::host_path_fingerprint(&cwd_str);
    if cwd_fingerprint == manifest.host_workdir_fingerprint {
        return Some(LoadWorkspaceInput::CurrentDir);
    }
    if allow_moved {
        return Some(LoadWorkspaceInput::Path {
            src: cwd_str,
            dst: manifest.workdir.clone(),
        });
    }
    None
}

fn ad_hoc_restore_input_for_moved_path(
    manifest: &instance::InstanceManifest,
    path: &std::path::Path,
) -> Option<LoadWorkspaceInput> {
    let path = path.canonicalize().ok()?;
    ad_hoc_restore_input_for_current_dir(manifest, &path, true)
}

fn prompt_moved_ad_hoc_project_path(
    manifest: &instance::InstanceManifest,
    cwd: &std::path::Path,
) -> Result<Option<std::path::PathBuf>> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        return Ok(None);
    }
    let choices = [
        format!("Use current directory ({})", cwd.display()),
        "Browse for moved project path".to_string(),
        "Enter another moved project path".to_string(),
        "Cancel restore".to_string(),
    ];
    let selected = dialoguer::Select::new()
        .with_prompt(format!(
            "Ad-hoc instance `{}` was created for `{}`, but the current directory is `{}`. Which host path should be mounted at the original in-container workdir?",
            manifest.container_base,
            manifest.workdir,
            cwd.display()
        ))
        .items(&choices)
        .default(0)
        .interact()?;

    match selected {
        0 => Ok(Some(cwd.to_path_buf())),
        1 => prompt_ad_hoc_moved_path_browser(cwd),
        2 => prompt_ad_hoc_moved_path_entry(),
        _ => Ok(None),
    }
}

fn prompt_ad_hoc_moved_path_browser(start: &std::path::Path) -> Result<Option<std::path::PathBuf>> {
    let mut cwd = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    loop {
        let choices = moved_path_browser_choices(&cwd);
        let labels: Vec<String> = choices.iter().map(MovedPathBrowserChoice::label).collect();
        let selected = dialoguer::Select::new()
            .with_prompt(format!(
                "Browse to the moved project directory from {}",
                cwd.display()
            ))
            .items(&labels)
            .default(0)
            .interact()?;
        match choices
            .get(selected)
            .cloned()
            .unwrap_or(MovedPathBrowserChoice::Cancel)
        {
            MovedPathBrowserChoice::SelectCurrent(path) => return Ok(Some(path)),
            MovedPathBrowserChoice::Parent(path) | MovedPathBrowserChoice::Child(path) => {
                cwd = path;
            }
            MovedPathBrowserChoice::Manual => return prompt_ad_hoc_moved_path_entry(),
            MovedPathBrowserChoice::Cancel => return Ok(None),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MovedPathBrowserChoice {
    SelectCurrent(std::path::PathBuf),
    Parent(std::path::PathBuf),
    Child(std::path::PathBuf),
    Manual,
    Cancel,
}

impl MovedPathBrowserChoice {
    fn label(&self) -> String {
        match self {
            Self::SelectCurrent(path) => format!("Use this directory ({})", path.display()),
            Self::Parent(path) => format!("Go up ({})", path.display()),
            Self::Child(path) => format!(
                "{}/",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            Self::Manual => "Enter a path manually".to_string(),
            Self::Cancel => "Cancel restore".to_string(),
        }
    }
}

fn moved_path_browser_choices(cwd: &std::path::Path) -> Vec<MovedPathBrowserChoice> {
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let mut choices = vec![MovedPathBrowserChoice::SelectCurrent(cwd.clone())];
    if let Some(parent) = cwd.parent() {
        choices.push(MovedPathBrowserChoice::Parent(parent.to_path_buf()));
    }
    choices.extend(
        moved_path_browser_child_dirs(&cwd)
            .into_iter()
            .map(MovedPathBrowserChoice::Child),
    );
    choices.push(MovedPathBrowserChoice::Manual);
    choices.push(MovedPathBrowserChoice::Cancel);
    choices
}

fn moved_path_browser_child_dirs(cwd: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(cwd) else {
        return Vec::new();
    };
    let mut dirs: Vec<std::path::PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            path.is_dir().then_some(path)
        })
        .collect();
    dirs.sort_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    dirs
}

/// One step of the moved-path entry loop, factored out of the
/// `dialoguer::Input::interact_text()` call so the four cases (blank /
/// valid dir / not-a-dir / canonicalize-fail) can be unit-tested
/// without an interactive prompt.
enum MovedPathEntryStep {
    /// Empty input → operator cancelled.
    Cancel,
    /// Canonical absolute path; entry loop returns this.
    Accepted(std::path::PathBuf),
    /// Operator must retry; carries the message to print before the
    /// next prompt iteration.
    Retry(String),
}

fn classify_moved_path_entry(raw: &str) -> MovedPathEntryStep {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return MovedPathEntryStep::Cancel;
    }
    let path = std::path::PathBuf::from(resolve_path(trimmed));
    match path.canonicalize() {
        Ok(canonical) if canonical.is_dir() => MovedPathEntryStep::Accepted(canonical),
        Ok(canonical) => MovedPathEntryStep::Retry(format!(
            "path `{}` exists but is not a directory; enter a project directory or leave blank to cancel",
            canonical.display(),
        )),
        Err(err) => MovedPathEntryStep::Retry(format!(
            "cannot use `{}`: {err}; enter an existing project directory or leave blank to cancel",
            path.display(),
        )),
    }
}

fn prompt_ad_hoc_moved_path_entry() -> Result<Option<std::path::PathBuf>> {
    loop {
        let raw: String = dialoguer::Input::new()
            .with_prompt("Moved project path")
            .interact_text()?;
        match classify_moved_path_entry(&raw) {
            MovedPathEntryStep::Cancel => return Ok(None),
            MovedPathEntryStep::Accepted(path) => return Ok(Some(path)),
            MovedPathEntryStep::Retry(msg) => eprintln!("{msg}"),
        }
    }
}

/// Render the `config auth show` output as a string. Empty workspace + role
/// names fall through to layer 1 (global), so this prints the global default
/// for each agent. Printing every built-in agent avoids privileging any one
/// runtime in the no-context output until/unless an `--agent` flag is added.
fn render_auth_show(config: &AppConfig) -> String {
    use std::fmt::Write as _;
    let claude_mode = crate::config::resolve_mode(config, crate::agent::Agent::Claude, "", "");
    let codex_mode = crate::config::resolve_mode(config, crate::agent::Agent::Codex, "", "");
    let amp_mode = crate::config::resolve_mode(config, crate::agent::Agent::Amp, "", "");
    let kimi_mode = crate::config::resolve_mode(config, crate::agent::Agent::Kimi, "", "");
    let opencode_mode = crate::config::resolve_mode(config, crate::agent::Agent::Opencode, "", "");
    let mut out = String::new();
    let _ = writeln!(out, "claude: {claude_mode}");
    let _ = writeln!(out, "codex:  {codex_mode}");
    let _ = writeln!(out, "amp:    {amp_mode}");
    let _ = writeln!(out, "kimi:   {kimi_mode}");
    let _ = writeln!(out, "opencode: {opencode_mode}");
    out
}

/// Render the `workspace show <name>` output as a string. Includes the info
/// table (name/workdir/allowed/default-role), and, when there are mounts, a
/// trailing mounts table with one row per mount. The mounts table renders the
/// canonical lowercase isolation name (`shared`/`worktree`/`clone`) so the output
/// matches TOML/CLI input verbatim.
#[allow(clippy::too_many_lines)]
fn render_workspace_show(config: &AppConfig, name: &str, workspace: &WorkspaceConfig) -> String {
    use std::fmt::Write as _;
    use tabled::settings::Style;
    use tabled::{Table, Tabled};

    #[derive(Tabled)]
    struct MountRow {
        #[tabled(rename = "Mount")]
        mount: String,
        #[tabled(rename = "Mode")]
        mode: String,
        #[tabled(rename = "Isolation")]
        isolation: String,
        #[tabled(rename = "Type")]
        kind: String,
    }
    #[derive(Tabled)]
    struct GlobalMountRowWithScope {
        #[tabled(rename = "Scope")]
        scope: String,
        #[tabled(rename = "Name")]
        name: String,
        #[tabled(rename = "Mount")]
        mount: String,
        #[tabled(rename = "Mode")]
        mode: String,
    }
    #[derive(Tabled)]
    struct GlobalMountRow {
        #[tabled(rename = "Name")]
        name: String,
        #[tabled(rename = "Mount")]
        mount: String,
        #[tabled(rename = "Mode")]
        mode: String,
    }

    let allowed = if workspace.allowed_roles.is_empty() {
        "any role".to_string()
    } else {
        workspace.allowed_roles.join(", ")
    };
    let default_role = workspace.default_role.as_deref().unwrap_or("none");
    let agent = workspace.resolved_agent().slug();

    let short_workdir = tui::shorten_home(&workspace.workdir);
    let mut info: Vec<(&str, &str)> = vec![
        ("Name", name),
        ("Workdir", short_workdir.as_str()),
        ("Allowed Roles", allowed.as_str()),
        ("Default Role", default_role),
        ("Agent", agent),
    ];
    // Only surface keep_awake when opted in — disabled is the default and
    // shouldn't add noise. When enabled, the operator sees it here so a
    // mysteriously sleepless Mac traces back to the workspace.
    if workspace.keep_awake.enabled {
        info.push(("Keep Awake", "enabled (macOS only)"));
    }
    if workspace.git_pull_on_entry {
        info.push(("Git Pull", "on entry"));
    }
    let mut info_table = Table::builder(info.iter().map(|(k, v)| [*k, *v])).build();
    info_table
        .with(Style::modern_rounded())
        .with(tabled::settings::Remove::row(
            tabled::settings::object::Rows::first(),
        ));

    let mut out = String::new();
    let _ = writeln!(out, "{info_table}");

    if !workspace.mounts.is_empty() {
        let mount_rows: Vec<MountRow> = workspace
            .mounts
            .iter()
            .map(|m| MountRow {
                mount: mount_display(&m.src, &m.dst),
                mode: mount_mode(m.readonly),
                isolation: m.isolation.as_str().to_string(),
                kind: jackin_console::mount_info::inspect(&m.src).label(),
            })
            .collect();
        let mut mount_table = Table::new(mount_rows);
        mount_table.with(Style::modern_rounded());
        let _ = writeln!(out);
        let _ = writeln!(out, "Workspace mounts:");
        let _ = writeln!(out, "{mount_table}");
    }

    let render_unscoped_table = |out: &mut String, rows: &[&crate::config::GlobalMountRow]| {
        if rows.is_empty() {
            return;
        }
        let mut table = Table::new(rows.iter().map(|row| GlobalMountRow {
            name: row.name.clone(),
            mount: mount_display(&row.mount.src, &row.mount.dst),
            mode: mount_mode(row.mount.readonly),
        }));
        table.with(Style::modern_rounded());
        let _ = writeln!(out);
        let _ = writeln!(out, "Global mounts:");
        let _ = writeln!(out, "{table}");
    };

    match config.workspace_applicable_mount_rows(workspace) {
        crate::config::WorkspaceGlobalMountRows::Applicable { role, rows } => {
            if rows.is_empty() {
                return out;
            }
            let has_scoped_rows = rows.iter().any(|row| row.scope.is_some());
            if !has_scoped_rows {
                render_unscoped_table(&mut out, &rows.iter().collect::<Vec<_>>());
                return out;
            }
            let mut table = Table::new(rows.iter().map(|row| GlobalMountRowWithScope {
                scope: row.scope.as_deref().unwrap_or("global").to_string(),
                name: row.name.clone(),
                mount: mount_display(&row.mount.src, &row.mount.dst),
                mode: mount_mode(row.mount.readonly),
            }));
            table.with(Style::modern_rounded());
            let _ = writeln!(out);
            let _ = writeln!(out, "Global mounts ({role}):");
            let _ = writeln!(out, "{table}");
        }
        crate::config::WorkspaceGlobalMountRows::Ambiguous { candidates } => {
            // Unscoped global mounts apply regardless of role — render
            // them even when the role is ambiguous. Only the scoped
            // subset depends on role selection.
            let all_rows = config.list_mount_rows();
            let unscoped: Vec<&crate::config::GlobalMountRow> =
                all_rows.iter().filter(|row| row.scope.is_none()).collect();
            render_unscoped_table(&mut out, &unscoped);
            if all_rows.iter().any(|row| row.scope.is_some()) {
                let _ = writeln!(out);
                let _ = writeln!(
                    out,
                    "Role-scoped global mounts depend on selected role ({})",
                    candidates.join(", ")
                );
            }
        }
    }

    out
}

fn mount_mode(readonly: bool) -> String {
    if readonly { "read-only" } else { "read-write" }.to_string()
}

fn mount_display(src: &str, dst: &str) -> String {
    let short_dst = tui::shorten_home(dst);
    if src == dst {
        short_dst
    } else {
        format!("{}\nhost: {}", short_dst, tui::shorten_home(src))
    }
}

#[cfg(test)]
mod auth_set_tests;
#[cfg(test)]
mod resolve_role_tests;
