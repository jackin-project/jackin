// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Config subcommand dispatch — extracted from `app::run` to keep mod.rs focused.

use anyhow::Result;

use crate::cli::{self, ConfigCommand};
use crate::workspace::resolve_path;
use jackin_config::{self, AppConfig};
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;

#[derive(tabled::Tabled)]
pub(super) struct EnvRow {
    #[tabled(rename = "Key")]
    key: String,
    #[tabled(rename = "Value")]
    value: String,
}

pub(super) fn resolve_env_value_for_cli(value: &str) -> Result<jackin_core::EnvValue> {
    if !value.starts_with("op://") {
        return Ok(jackin_core::EnvValue::Plain(value.to_owned()));
    }

    // Probe op CLI availability before attempting structural queries.
    let op_cli = jackin_env::OpCli::new();
    jackin_env::OpRunner::probe(&op_cli).map_err(|e| {
        anyhow::anyhow!(
            "`op` CLI not available; cannot resolve `op://...` reference. \
             Install 1Password CLI, or use a non-op:// value.\n\
             Probe error: {e}"
        )
    })?;

    let op_ref = jackin_env::resolve_op_uri_to_ref(value, &op_cli, None)?;
    Ok(jackin_core::EnvValue::OpRef(op_ref))
}

pub(super) fn print_env_table(vars: &[(String, String)]) {
    use tabled::Table;
    use tabled::settings::Style;
    if vars.is_empty() {
        println!("No env vars set.");
        return;
    }
    let rows: Vec<EnvRow> = vars
        .iter()
        .map(|(k, v)| EnvRow {
            key: k.clone(),
            value: v.clone(),
        })
        .collect();
    let mut table = Table::new(rows);
    table.with(Style::modern());
    println!("{table}");
}

pub(super) fn handle(
    cmd: ConfigCommand,
    config: &mut AppConfig,
    paths: &JackinPaths,
    _debug: bool,
) -> Result<()> {
    match cmd {
        ConfigCommand::Mount(mount_cmd) => handle_mount_cmd(mount_cmd, config, paths),
        ConfigCommand::Trust(trust_cmd) => handle_trust_cmd(trust_cmd, config, paths),
        ConfigCommand::Auth(auth_cmd) => handle_auth_cmd(auth_cmd, config, paths),
        ConfigCommand::Env(env_cmd) => handle_env_cmd(env_cmd, config, paths),
        ConfigCommand::Git(git_cmd) => handle_git_cmd(git_cmd, paths),
    }
}

fn handle_mount_cmd(
    mount_cmd: cli::MountCommand,
    config: &AppConfig,
    paths: &JackinPaths,
) -> Result<()> {
    match mount_cmd {
        cli::MountCommand::Add {
            name,
            src,
            dst,
            readonly,
            scope,
        } => {
            let ro = if readonly { " (read-only)" } else { "" };
            let scope_label = scope.as_deref().unwrap_or("global");
            let resolved_src = resolve_path(&src);
            let mount = jackin_config::MountConfig {
                src: resolved_src,
                dst: dst.clone(),
                readonly,
                isolation: jackin_core::MountIsolation::Shared,
            };
            crate::workspace::validate_mounts(std::slice::from_ref(&mount))?;
            let sensitive = crate::workspace::find_sensitive_mounts(std::slice::from_ref(&mount));
            if !sensitive.is_empty() && !crate::workspace::confirm_sensitive_mounts(&sensitive)? {
                anyhow::bail!("aborted — sensitive mount paths were not confirmed");
            }
            let (matched, mut candidate_rows): (
                Vec<jackin_config::GlobalMountRow>,
                Vec<jackin_config::GlobalMountRow>,
            ) = config
                .list_mount_rows()
                .into_iter()
                .partition(|row| row.name == name && row.scope == scope);
            let existing = matched.into_iter().next();
            candidate_rows.push(jackin_config::GlobalMountRow {
                scope: scope.clone(),
                name: name.clone(),
                mount: mount.clone(),
            });
            AppConfig::validate_global_mount_rows(&candidate_rows)?;
            let mut editor = jackin_config::ConfigEditor::open(paths)?;
            editor.add_mount(&name, mount, scope.as_deref());
            editor.save()?;
            if let Some(prev) = existing {
                println!(
                    "Replaced mount {name:?} ({scope_label}):\n  was: {} -> {}\n  now: {} -> {}{ro}",
                    prev.mount.src, prev.mount.dst, src, dst
                );
            } else {
                println!("Added mount {name:?} ({scope_label}):\n  {dst}\n  host: {src}{ro}");
            }
            Ok(())
        }
        cli::MountCommand::Remove { name, scope } => {
            let mut editor = jackin_config::ConfigEditor::open(paths)?;
            if editor.remove_mount(&name, scope.as_deref()) {
                editor.save()?;
                println!("Removed mount {name:?}.");
            } else {
                drop(editor);
                println!("Mount {name:?} not found.");
            }
            Ok(())
        }
        cli::MountCommand::List => {
            let mounts = config.list_mount_rows();
            if mounts.is_empty() {
                println!("No mounts configured.");
            } else {
                use tabled::settings::Style;
                use tabled::{Table, Tabled};
                #[derive(Tabled)]
                struct GlobalRow {
                    #[tabled(rename = "Name")]
                    name: String,
                    #[tabled(rename = "Mount")]
                    mount: String,
                    #[tabled(rename = "Mode")]
                    mode: String,
                }
                #[derive(Tabled)]
                struct Row {
                    #[tabled(rename = "Scope")]
                    scope: String,
                    #[tabled(rename = "Name")]
                    name: String,
                    #[tabled(rename = "Mount")]
                    mount: String,
                    #[tabled(rename = "Mode")]
                    mode: String,
                }
                let (global, scoped): (Vec<_>, Vec<_>) =
                    mounts.iter().partition(|row| row.scope.is_none());
                let has_global = !global.is_empty();
                if !global.is_empty() {
                    let rows: Vec<GlobalRow> = global
                        .into_iter()
                        .map(|row| GlobalRow {
                            name: row.name.clone(),
                            mount: super::mount_display(&row.mount.src, &row.mount.dst),
                            mode: super::mount_mode(row.mount.readonly),
                        })
                        .collect();
                    let mut table = Table::new(rows);
                    table.with(Style::modern());
                    println!("Global mounts:");
                    println!("{table}");
                }
                if !scoped.is_empty() {
                    if has_global {
                        println!();
                    }
                    let rows: Vec<Row> = scoped
                        .into_iter()
                        .map(|row| Row {
                            scope: row.scope.clone().unwrap_or_default(),
                            name: row.name.clone(),
                            mount: super::mount_display(&row.mount.src, &row.mount.dst),
                            mode: super::mount_mode(row.mount.readonly),
                        })
                        .collect();
                    let mut table = Table::new(rows);
                    table.with(Style::modern());
                    println!("Scoped global mounts:");
                    println!("{table}");
                }
            }
            Ok(())
        }
    }
}

fn handle_trust_cmd(
    trust_cmd: cli::TrustCommand,
    config: &mut AppConfig,
    paths: &JackinPaths,
) -> Result<()> {
    match trust_cmd {
        cli::TrustCommand::Grant { selector } => {
            let class = RoleSelector::parse(&selector)?;
            config.resolve_role_source(&class)?;
            let was_trusted = config.roles.get(&class.key()).is_some_and(|a| a.trusted);
            if was_trusted {
                println!("{} is already trusted.", class.key());
            } else {
                let mut editor = jackin_config::ConfigEditor::open(paths)?;
                if let Some(source) = config.roles.get(&class.key()) {
                    editor.upsert_agent_source(&class.key(), source);
                }
                editor.set_agent_trust(&class.key(), true);
                editor.save()?;
                println!("Trusted {}.", class.key());
            }
            Ok(())
        }
        cli::TrustCommand::Revoke { selector } => {
            let class = RoleSelector::parse(&selector)?;
            if AppConfig::is_builtin_agent(&class.key()) {
                anyhow::bail!("{} is a built-in role and is always trusted.", class.key());
            }
            let was_trusted = config.roles.get(&class.key()).is_some_and(|a| a.trusted);
            if was_trusted {
                let mut editor = jackin_config::ConfigEditor::open(paths)?;
                editor.set_agent_trust(&class.key(), false);
                editor.save()?;
                println!("Revoked trust for {}.", class.key());
            } else {
                println!("{} is not currently trusted.", class.key());
            }
            Ok(())
        }
        cli::TrustCommand::List => {
            let roles: Vec<_> = config
                .roles
                .iter()
                .filter(|(_, source)| source.trusted)
                .map(|(key, _)| key.clone())
                .collect();
            if roles.is_empty() {
                println!("No trusted roles.");
            } else {
                for key in roles {
                    println!("{key}");
                }
            }
            Ok(())
        }
    }
}

fn handle_auth_cmd(
    auth_cmd: cli::AuthCommand,
    config: &AppConfig,
    paths: &JackinPaths,
) -> Result<()> {
    match auth_cmd {
        cli::AuthCommand::Set { mode, agent } => {
            let parsed_agent = super::parse_agent_from_cli(&agent)?;
            let parsed_mode = super::parse_auth_forward_mode_from_cli(&mode)?;
            if !parsed_agent.supported_modes().contains(&parsed_mode) {
                anyhow::bail!(
                    "auth_forward {parsed_mode} is not supported for {parsed_agent}; \
                         supported modes: {:?}",
                    parsed_agent.supported_modes()
                );
            }
            let mut editor = jackin_config::ConfigEditor::open(paths)?;
            editor.set_global_auth_forward(parsed_agent, parsed_mode);
            editor.save()?;
            println!("Set global {parsed_agent} auth forwarding to {parsed_mode}.");
            Ok(())
        }
        cli::AuthCommand::Show => {
            print!("{}", super::render_auth_show(config));
            Ok(())
        }
    }
}

fn handle_env_cmd(env_cmd: cli::EnvCommand, config: &AppConfig, paths: &JackinPaths) -> Result<()> {
    match env_cmd {
        cli::EnvCommand::Set {
            key,
            value,
            role,
            comment,
        } => {
            if key.is_empty() {
                anyhow::bail!("env var key cannot be empty");
            }
            if jackin_core::is_reserved(&key) {
                anyhow::bail!(
                    "env name {key:?} is reserved by the jackin runtime and cannot be set"
                );
            }
            if let Some(ref agent_key) = role
                && !config.roles.contains_key(agent_key)
            {
                anyhow::bail!(
                    "role {agent_key:?} is not registered; register it with \
                         `jackin role register` (or run a command that resolves it) \
                         before setting role-scoped env vars"
                );
            }
            let env_value = resolve_env_value_for_cli(&value)?;
            let scope = role.map_or(
                jackin_config::EnvScope::Global,
                jackin_config::EnvScope::Role,
            );
            let mut editor = jackin_config::ConfigEditor::open(paths)?;
            editor.set_env_var(&scope, &key, env_value)?;
            if let Some(ref c) = comment {
                editor.set_env_comment(&scope, &key, Some(c));
            }
            editor.save()?;
            println!("Set {key}.");
            Ok(())
        }
        cli::EnvCommand::Unset { key, role } => {
            if key.is_empty() {
                anyhow::bail!("env var key cannot be empty");
            }
            let scope = role.map_or(
                jackin_config::EnvScope::Global,
                jackin_config::EnvScope::Role,
            );
            let mut editor = jackin_config::ConfigEditor::open(paths)?;
            if editor.remove_env_var(&scope, &key) {
                editor.save()?;
                println!("Removed {key}.");
            } else {
                drop(editor);
                println!("{key} not set.");
            }
            Ok(())
        }
        cli::EnvCommand::List { role } => {
            let vars: Vec<(String, String)> = role.as_ref().map_or_else(
                || {
                    config
                        .env
                        .iter()
                        .map(|(k, v)| (k.clone(), v.as_display_str().to_owned()))
                        .collect()
                },
                |a| {
                    config.roles.get(a).map_or_else(Vec::new, |src| {
                        src.env
                            .iter()
                            .map(|(k, v)| (k.clone(), v.as_display_str().to_owned()))
                            .collect()
                    })
                },
            );
            print_env_table(&vars);
            Ok(())
        }
    }
}

fn handle_git_cmd(git_cmd: cli::GitCommand, paths: &JackinPaths) -> Result<()> {
    match git_cmd {
        cli::GitCommand::CoauthorTrailer(cmd) => {
            let enable = match cmd {
                cli::CoauthorTrailerCommand::Enable => true,
                cli::CoauthorTrailerCommand::Disable => false,
            };
            let mut editor = jackin_config::ConfigEditor::open(paths)?;
            editor.set_git_coauthor_trailer(enable);
            let saved = editor.save()?;
            if saved.git.coauthor_trailer {
                println!("coauthor_trailer: enabled");
            } else {
                println!("coauthor_trailer: disabled");
            }
            Ok(())
        }
        cli::GitCommand::Dco(cmd) => {
            let enable = match cmd {
                cli::DcoCommand::Enable => true,
                cli::DcoCommand::Disable => false,
            };
            let mut editor = jackin_config::ConfigEditor::open(paths)?;
            editor.set_git_dco(enable);
            let saved = editor.save()?;
            if saved.git.dco {
                println!("dco: enabled");
            } else {
                println!("dco: disabled");
            }
            Ok(())
        }
    }
}
