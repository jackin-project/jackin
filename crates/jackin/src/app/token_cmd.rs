// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Claude OAuth token setup commands.

use anyhow::Result;

use crate::cli;
use jackin_config::AppConfig;
use jackin_core::JackinPaths;

#[allow(
    clippy::too_many_lines,
    reason = "Claude token subcommand dispatch (setup/rotate/revoke/doctor); one line over cap. Tracked for the root-jackin-integration decomposition slice (codebase-health)."
)]
pub(super) fn handle_claude_token(
    paths: &JackinPaths,
    config: &mut AppConfig,
    action: cli::WorkspaceClaudeTokenCommand,
) -> Result<()> {
    use crate::workspace::token_setup;
    use jackin_core::OpRef;

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
        let probe = jackin_env::OpCli::new();
        jackin_env::resolve_op_uri_to_ref(input, &probe, account)
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
    prior: Option<jackin_core::EnvValue>,
    new_ref: &jackin_core::OpRef,
    account: Option<String>,
) -> Result<()> {
    let op_cli = jackin_env::OpCli::new().with_account(account);
    delete_prior_op_item_with_runner(prior, new_ref, &op_cli)
}

pub(crate) fn delete_prior_op_item_with_runner(
    prior: Option<jackin_core::EnvValue>,
    new_ref: &jackin_core::OpRef,
    op_writer: &dyn jackin_env::OpWriteRunner,
) -> Result<()> {
    let Some(jackin_core::EnvValue::OpRef(prior_ref)) = prior else {
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
    let Some(parts) = jackin_core::op_reference::parse_op_reference(&prior_ref.op) else {
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
pub(crate) fn validate_setup_role_allowed(
    config: &AppConfig,
    workspace: &str,
    role: &str,
) -> Result<()> {
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

    let mut labels: Vec<String> = vec!["All roles (workspace level)".to_owned()];
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
    use crate::workspace::token_setup;
    use jackin_env::{OpCli, OpStructRunner};
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
    let mut item_labels: Vec<String> = vec!["[ + New item ]".to_owned()];
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
            .default(token_setup::DEFAULT_FIELD_LABEL.to_owned())
            .interact_text()?;
        // Trim so padding can't reach the op item title / field id+label,
        // matching the TUI picker's commit trimming.
        return Ok(token_setup::TokenSetupArgs {
            vault: Some(vault.id.clone()),
            item_name: Some(item_name.trim().to_owned()),
            account: account_id,
            reuse: None,
            field_label: Some(field_label.trim().to_owned()),
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
    op: &jackin_env::OpCli,
    account_id: Option<&str>,
    vault_id: &str,
    item_id: &str,
) -> Result<(Option<String>, jackin_core::FieldTarget)> {
    use crate::workspace::token_setup;
    use jackin_core::FieldTarget;
    use jackin_core::op_reference::parse_op_reference;
    use jackin_env::OpStructRunner;

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
        .map(|s| s.clone().unwrap_or_else(|| "(root)".to_owned()))
        .collect();
    section_labels.push("[ + New section ]".to_owned());

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
        Some(name.trim().to_owned())
    } else {
        sections[section_choice].clone()
    };

    // Fields scoped to the chosen section.
    let scoped: Vec<&jackin_env::OpField> = fields
        .iter()
        .filter(|f| {
            parse_op_reference(&f.reference)
                .and_then(|p| p.section)
                .as_deref()
                == section.as_deref()
        })
        .collect();

    let mut field_labels: Vec<String> = vec!["[ + New field ]".to_owned()];
    field_labels.extend(scoped.iter().map(|f| {
        let label = if f.label.is_empty() { &f.id } else { &f.label };
        let kind = if f.concealed {
            "concealed".to_owned()
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
            .default(token_setup::DEFAULT_FIELD_LABEL.to_owned())
            .interact_text()?;
        return Ok((
            section,
            FieldTarget::New {
                label: field_label.trim().to_owned(),
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
