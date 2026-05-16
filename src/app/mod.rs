pub mod context;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;

use crate::cli::cleanup::{EjectArgs, PurgeArgs};
use crate::cli::role::{ConsoleArgs, HardlineArgs, LoadArgs};
use crate::cli::{self, Cli, Command, PruneCommand, WorkspaceCommand};
use crate::config::{self, AppConfig};
use crate::console;
use crate::docker::ShellRunner;
use crate::instance;
use crate::paths::JackinPaths;
use crate::runtime;
use crate::selector::{RoleSelector, Selector};
use crate::tui;
use crate::workspace::{
    self, LoadWorkspaceInput, WorkspaceConfig, WorkspaceEdit, parse_mount_spec_resolved,
    resolve_path,
};

use self::context::{
    TargetKind, classify_target, prompt_agent_choice_if_needed, remember_last_agent,
    resolve_agent_from_context, resolve_running_container_from_context, resolve_target_name,
};

/// Parse an `auth_forward` mode value as it arrived from the CLI.
fn parse_auth_forward_mode_from_cli(raw: &str) -> anyhow::Result<config::AuthForwardMode> {
    raw.parse().map_err(|e: String| anyhow::anyhow!("{e}"))
}

/// Parse an agent slug as it arrived from the CLI.
fn parse_agent_from_cli(raw: &str) -> anyhow::Result<crate::agent::Agent> {
    raw.parse()
        .map_err(|_| anyhow::anyhow!("unknown agent {raw:?}; expected one of: claude, codex, amp"))
}

#[allow(clippy::too_many_lines)]
pub fn run(cli: Cli) -> Result<()> {
    let debug = cli.debug;
    tui::set_debug_mode(debug);

    // Resolve the subcommand. Bare `jackin` currently routes to the same
    // console handler as `jackin console`; the TTY-capability fallback and
    // the deprecation warning for `launch` land in a follow-up commit.
    let command = match cli.command {
        Some(cmd) => cmd,
        None => Command::Console(cli.console_args),
    };

    let command = match command {
        Command::Role(command) => return crate::role_authoring::run(command),
        command => command,
    };

    let paths = JackinPaths::detect()?;
    let mut config = AppConfig::load_or_init(&paths)?;
    let mut runner = ShellRunner { debug };

    match command {
        Command::Load(LoadArgs {
            selector,
            target,
            mounts,
            rebuild,
            no_intro,
            force,
            agent,
            role_branch,
        }) => {
            let cwd = std::env::current_dir()?;

            let (class, workspace_input) = if let Some(sel) = selector {
                let class = RoleSelector::parse(&sel)?;
                let input = match target {
                    None => LoadWorkspaceInput::CurrentDir,
                    Some(t) => match classify_target(&t) {
                        TargetKind::Path { src, dst } => LoadWorkspaceInput::Path { src, dst },
                        TargetKind::Name(name) => resolve_target_name(&name, &config, &cwd)?,
                    },
                };
                (class, input)
            } else {
                // No selector — resolve role from workspace context
                resolve_agent_from_context(&config, &cwd)?
            };

            let saved_workspace_name = if let LoadWorkspaceInput::Saved(ref name) = workspace_input
            {
                Some(name.clone())
            } else {
                None
            };

            let ad_hoc_mounts = mounts
                .iter()
                .map(|value| parse_mount_spec_resolved(value))
                .collect::<Result<Vec<_>>>()?;

            let resolved_workspace = crate::workspace::resolve_load_workspace(
                &config,
                &class,
                &cwd,
                workspace_input,
                &ad_hoc_mounts,
            )?;

            let sensitive = crate::workspace::find_sensitive_mounts(&resolved_workspace.mounts);
            if !sensitive.is_empty() && !crate::workspace::confirm_sensitive_mounts(&sensitive)? {
                anyhow::bail!("aborted — sensitive mount paths were not confirmed");
            }

            let mut opts = runtime::LoadOptions::for_load(no_intro, debug, rebuild);
            opts.force = force;
            opts.agent = match agent {
                Some(explicit) => Some(explicit),
                None => {
                    prompt_agent_choice_if_needed(&paths, &class, resolved_workspace.default_agent)?
                }
            };
            opts.role_branch = role_branch;
            // Pre-launch reconcile: if a previous role in a keep_awake
            // workspace already runs, ensure caffeinate is up before we
            // build/launch (so a long Docker build doesn't see the host
            // sleep). Post-launch reconcile below catches the new role.
            runtime::reconcile_keep_awake(&paths, &mut runner);
            let result = runtime::load_role(
                &paths,
                &mut config,
                &class,
                &resolved_workspace,
                &mut runner,
                &opts,
            );
            remember_last_agent(
                &paths,
                &mut config,
                saved_workspace_name.as_deref(),
                &class,
                &result,
            );
            runtime::reconcile_keep_awake(&paths, &mut runner);
            result
        }
        Command::Console(ConsoleArgs {}) => {
            let cwd = std::env::current_dir()?;
            let Some(outcome) = console::run_console(config, &paths, &cwd)? else {
                return Ok(());
            };

            // config was consumed by run_console (the manager may have written to
            // disk). Reload so the post-console path sees the latest state.
            let mut config = AppConfig::load_or_init(&paths)?;
            let (class, workspace, selected_agent) = match outcome {
                console::ConsoleOutcome::Launch(class, workspace, selected_agent) => {
                    (class, workspace, selected_agent)
                }
                outcome @ console::ConsoleOutcome::InstanceAction { .. } => {
                    return handle_console_instance_action(
                        &paths,
                        &mut config,
                        outcome,
                        &mut runner,
                    );
                }
            };

            let sensitive = crate::workspace::find_sensitive_mounts(&workspace.mounts);
            if !sensitive.is_empty() && !crate::workspace::confirm_sensitive_mounts(&sensitive)? {
                anyhow::bail!("aborted — sensitive mount paths were not confirmed");
            }

            let mut opts = runtime::LoadOptions::for_launch(debug);
            opts.agent = match selected_agent {
                Some(agent) => Some(agent),
                None => prompt_agent_choice_if_needed(&paths, &class, workspace.default_agent)?,
            };
            runtime::reconcile_keep_awake(&paths, &mut runner);
            let result =
                runtime::load_role(&paths, &mut config, &class, &workspace, &mut runner, &opts);
            remember_last_agent(&paths, &mut config, Some(&workspace.label), &class, &result);
            runtime::reconcile_keep_awake(&paths, &mut runner);
            result
        }
        Command::Hardline(HardlineArgs {
            selector,
            inspect,
            new,
            agent,
            shell,
        }) => {
            // `--inspect` / `--new` / `--shell` mutual exclusion is enforced by
            // clap `conflicts_with_all` on `HardlineArgs`; no runtime guard needed.
            let explicit_selector = selector.is_some();
            let container = if let Some(sel) = selector {
                if let Some(container) = resolve_instance_reference(&paths, &sel)? {
                    container
                } else {
                    match Selector::parse(&sel)? {
                        Selector::Container(name) => name,
                        Selector::Role(class) => resolve_role_to_container(&class, &mut runner)?,
                    }
                }
            } else {
                let cwd = std::env::current_dir()?;
                resolve_running_container_from_context(&paths, &config, &cwd, &mut runner)?
            };
            if shell {
                return runtime::spawn_shell_session(&paths, &container, &mut runner);
            }
            let action = if inspect {
                HardlineAction::Inspect
            } else if new {
                HardlineAction::NewSession
            } else if explicit_selector {
                prompt_explicit_hardline_action_if_multiple_sessions(&container, &mut runner)?
            } else {
                prompt_hardline_action(&container)?
            };
            if action == HardlineAction::Inspect {
                println!(
                    "{}",
                    runtime::inspect_hardline_instance(&paths, &container, &mut runner)?
                );
                return Ok(());
            }
            if action == HardlineAction::Cancel {
                return Ok(());
            }
            if action == HardlineAction::NewSession {
                let manifest = instance::InstanceManifest::read(&paths.data_dir.join(&container))
                    .with_context(|| {
                        format!(
                            "cannot start a new agent session in `{container}` because its instance manifest is missing"
                        )
                    })?;
                let selected_agent = if let Some(agent) = agent {
                    agent
                } else {
                    resolve_new_session_agent(&paths, &config, &manifest)?
                };
                runtime::reconcile_keep_awake(&paths, &mut runner);
                let result = runtime::spawn_agent_session(
                    &paths,
                    &container,
                    Some(&manifest),
                    selected_agent,
                    &mut runner,
                );
                runtime::reconcile_keep_awake(&paths, &mut runner);
                return result;
            }
            runtime::reconcile_keep_awake(&paths, &mut runner);
            let result = if let Some(manifest) =
                restore_candidate_for_hardline(&paths, &container, &mut runner)?
            {
                restore_hardline_instance(&paths, &mut config, &manifest, &mut runner)
            } else {
                runtime::hardline_agent(&paths, &container, &mut runner)
            };
            runtime::reconcile_keep_awake(&paths, &mut runner);
            result
        }
        Command::Eject(EjectArgs {
            selector,
            all,
            purge,
        }) => {
            let containers = if let Some(container) = resolve_instance_reference(&paths, &selector)?
            {
                if all {
                    anyhow::bail!("--all applies only to role selectors, not instance IDs");
                }
                vec![container]
            } else {
                match Selector::parse(&selector)? {
                    Selector::Container(container) => vec![container],
                    Selector::Role(class) => {
                        if all {
                            runtime::matching_family(
                                &class,
                                &runtime::list_managed_role_names(&mut runner)?,
                            )
                        } else {
                            vec![resolve_role_to_container(&class, &mut runner)?]
                        }
                    }
                }
            };
            // Wrap the loop so a partial failure still hits the trailing
            // reconcile — otherwise a `--all` eject that errors on
            // container N+1 would leave caffeinate running even though
            // earlier containers were already removed.
            let result: anyhow::Result<()> = (|| {
                if containers.is_empty() {
                    println!("No matching roles found.");
                } else {
                    for container in &containers {
                        runtime::eject_role(container, &mut runner)
                            .with_context(|| format!("ejecting {container}"))?;
                        if purge {
                            runtime::purge_container_state(&paths, container, &mut runner)
                                .with_context(|| format!("purging local state for {container}"))?;
                            println!("Ejected and purged {container}.");
                        } else {
                            println!("Ejected {container}.");
                        }
                    }
                }
                Ok(())
            })();
            runtime::reconcile_keep_awake(&paths, &mut runner);
            result
        }
        Command::Exile => {
            let names = runtime::list_managed_role_names(&mut runner)?;
            let result: anyhow::Result<()> = (|| {
                if names.is_empty() {
                    println!("No roles running.");
                } else {
                    for name in &names {
                        runtime::eject_role(name, &mut runner)
                            .with_context(|| format!("ejecting {name}"))?;
                        println!("Ejected {name}.");
                    }
                }
                Ok(())
            })();
            runtime::reconcile_keep_awake(&paths, &mut runner);
            result
        }
        Command::Config(config_cmd) => match config_cmd {
            cli::ConfigCommand::Mount(mount_cmd) => match mount_cmd {
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
                    let mount = config::MountConfig {
                        src: resolved_src,
                        dst: dst.clone(),
                        readonly,
                        isolation: crate::isolation::MountIsolation::Shared,
                    };
                    crate::workspace::validate_mounts(std::slice::from_ref(&mount))?;
                    let sensitive =
                        crate::workspace::find_sensitive_mounts(std::slice::from_ref(&mount));
                    if !sensitive.is_empty()
                        && !crate::workspace::confirm_sensitive_mounts(&sensitive)?
                    {
                        anyhow::bail!("aborted — sensitive mount paths were not confirmed");
                    }
                    let (matched, mut candidate_rows): (
                        Vec<crate::config::GlobalMountRow>,
                        Vec<crate::config::GlobalMountRow>,
                    ) = config
                        .list_mount_rows()
                        .into_iter()
                        .partition(|row| row.name == name && row.scope == scope);
                    let existing = matched.into_iter().next();
                    candidate_rows.push(crate::config::GlobalMountRow {
                        scope: scope.clone(),
                        name: name.clone(),
                        mount: mount.clone(),
                    });
                    AppConfig::validate_global_mount_rows(&candidate_rows)?;
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
                    editor.add_mount(&name, mount, scope.as_deref());
                    editor.save()?;
                    if let Some(prev) = existing {
                        println!(
                            "Replaced mount {name:?} ({scope_label}):\n  was: {} -> {}\n  now: {} -> {}{ro}",
                            prev.mount.src, prev.mount.dst, src, dst
                        );
                    } else {
                        println!(
                            "Added mount {name:?} ({scope_label}):\n  {dst}\n  host: {src}{ro}"
                        );
                    }
                    Ok(())
                }
                cli::MountCommand::Remove { name, scope } => {
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
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
                                    mount: mount_display(&row.mount.src, &row.mount.dst),
                                    mode: mount_mode(row.mount.readonly),
                                })
                                .collect();
                            let mut table = Table::new(rows);
                            table.with(Style::modern_rounded());
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
                                    mount: mount_display(&row.mount.src, &row.mount.dst),
                                    mode: mount_mode(row.mount.readonly),
                                })
                                .collect();
                            let mut table = Table::new(rows);
                            table.with(Style::modern_rounded());
                            println!("Scoped global mounts:");
                            println!("{table}");
                        }
                    }
                    Ok(())
                }
            },
            cli::ConfigCommand::Trust(trust_cmd) => match trust_cmd {
                cli::TrustCommand::Grant { selector } => {
                    let class = RoleSelector::parse(&selector)?;
                    config.resolve_role_source(&class)?;
                    let was_trusted = config.roles.get(&class.key()).is_some_and(|a| a.trusted);
                    if was_trusted {
                        println!("{} is already trusted.", class.key());
                    } else {
                        let mut editor = crate::config::ConfigEditor::open(&paths)?;
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
                        let mut editor = crate::config::ConfigEditor::open(&paths)?;
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
            },
            cli::ConfigCommand::Auth(auth_cmd) => match auth_cmd {
                cli::AuthCommand::Set { mode, agent } => {
                    let parsed_agent = parse_agent_from_cli(&agent)?;
                    let parsed_mode = parse_auth_forward_mode_from_cli(&mode)?;
                    if !parsed_agent.supported_modes().contains(&parsed_mode) {
                        anyhow::bail!(
                            "auth_forward {parsed_mode} is not supported for {parsed_agent}; \
                             supported modes: {:?}",
                            parsed_agent.supported_modes()
                        );
                    }
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
                    editor.set_global_auth_forward(parsed_agent, parsed_mode);
                    editor.save()?;
                    println!("Set global {parsed_agent} auth forwarding to {parsed_mode}.");
                    Ok(())
                }
                cli::AuthCommand::Show => {
                    print!("{}", render_auth_show(&config));
                    Ok(())
                }
            },
            cli::ConfigCommand::Env(env_cmd) => match env_cmd {
                cli::EnvCommand::Set {
                    key,
                    value,
                    role,
                    comment,
                } => {
                    if key.is_empty() {
                        anyhow::bail!("env var key cannot be empty");
                    }
                    if crate::env_model::is_reserved(&key) {
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
                    let scope = role.map_or(config::EnvScope::Global, config::EnvScope::Role);
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
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
                    let scope = role.map_or(config::EnvScope::Global, config::EnvScope::Role);
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
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
                                .map(|(k, v)| (k.clone(), v.as_display_str().to_string()))
                                .collect()
                        },
                        |a| {
                            config.roles.get(a).map_or_else(Vec::new, |src| {
                                src.env
                                    .iter()
                                    .map(|(k, v)| (k.clone(), v.as_display_str().to_string()))
                                    .collect()
                            })
                        },
                    );
                    print_env_table(&vars);
                    Ok(())
                }
            },
        },
        Command::Workspace(command) => match command {
            WorkspaceCommand::Create {
                name,
                workdir,
                mounts,
                allowed_roles,
                default_role,
                default_agent,
                mount_isolation,
                keep_awake,
                git_pull,
            } => {
                let expanded_workdir = workspace::resolve_path(&workdir);
                let parsed_mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec_resolved(value))
                    .collect::<Result<Vec<_>>>()?;
                let mut plan = workspace::planner::plan_create(&parsed_mounts)?;
                workspace::planner::apply_isolation_overrides(
                    &mut plan.final_mounts,
                    &mount_isolation,
                )?;
                if !plan.collapsed.is_empty() {
                    let removed_list: Vec<String> = plan
                        .collapsed
                        .iter()
                        .map(|r| tui::shorten_home(&r.child.src))
                        .collect();
                    // Parent paths in a single create are all the same set; pick
                    // the first for the summary headline.
                    let parent = tui::shorten_home(&plan.collapsed[0].covered_by.src);
                    eprintln!(
                        "collapsed {} redundant mount(s) under {parent}: {}",
                        plan.collapsed.len(),
                        removed_list.join(", ")
                    );
                }
                let mount_count = plan.final_mounts.len();
                let ws = WorkspaceConfig {
                    version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                    workdir: expanded_workdir,
                    mounts: plan.final_mounts,
                    allowed_roles,
                    default_role,
                    default_agent,
                    last_role: None,
                    env: std::collections::BTreeMap::new(),
                    roles: std::collections::BTreeMap::new(),
                    keep_awake: crate::workspace::KeepAwakeConfig {
                        enabled: keep_awake,
                    },
                    op_account: None,
                    claude: None,
                    codex: None,
                    amp: None,
                    kimi: None,
                    opencode: None,
                    github: None,
                    git_pull_on_entry: git_pull,
                };
                let mut editor = crate::config::ConfigEditor::open(&paths)?;
                editor.create_workspace(&name, ws)?;
                editor.save()?;
                println!(
                    "Created workspace {name:?} (workdir: {}, {mount_count} mount(s)).",
                    tui::shorten_home(&workdir)
                );
                Ok(())
            }
            WorkspaceCommand::List => {
                let workspaces = config.list_workspaces();
                if workspaces.is_empty() {
                    println!("No workspaces configured.");
                    println!();
                    println!("Add one with:");
                    println!(
                        "  jackin workspace create <name> --workdir /path/to/project --mount /path/to/project"
                    );
                } else {
                    use tabled::settings::Style;
                    use tabled::{Table, Tabled};
                    #[derive(Tabled)]
                    struct Row {
                        #[tabled(rename = "Name")]
                        name: String,
                        #[tabled(rename = "Workdir")]
                        workdir: String,
                        #[tabled(rename = "Mounts")]
                        mounts: usize,
                        #[tabled(rename = "Allowed Roles")]
                        allowed: String,
                        #[tabled(rename = "Default Role")]
                        default_role: String,
                        #[tabled(rename = "Agent")]
                        agent: String,
                    }
                    let rows: Vec<Row> = workspaces
                        .iter()
                        .map(|(name, ws)| Row {
                            name: (*name).to_string(),
                            workdir: tui::shorten_home(&ws.workdir),
                            mounts: ws.mounts.len(),
                            allowed: if ws.allowed_roles.is_empty() {
                                "any role".to_string()
                            } else {
                                ws.allowed_roles.join(", ")
                            },
                            default_role: ws.default_role.as_deref().unwrap_or("none").to_string(),
                            agent: ws.resolved_agent().slug().to_string(),
                        })
                        .collect();
                    let mut table = Table::new(rows);
                    table.with(Style::modern_rounded());
                    println!("{table}");
                    println!();
                    tui::hint("Run ", "jackin workspace show <name>", " for details.");
                }
                Ok(())
            }
            WorkspaceCommand::Show { name } => {
                let workspace = config.require_workspace(&name)?;
                print!("{}", render_workspace_show(&config, &name, workspace));
                Ok(())
            }
            WorkspaceCommand::Edit {
                name,
                workdir,
                mounts,
                remove_destinations,
                no_workdir_mount,
                allowed_roles,
                remove_allowed_roles,
                default_role,
                clear_default_role,
                default_agent,
                clear_default_agent,
                assume_yes,
                prune,
                mount_isolation,
                delete_isolated_state,
                keep_awake,
                no_keep_awake,
                git_pull,
                no_git_pull,
            } => {
                // Map paired flags to Option<bool>: None = no change.
                // Mutual exclusion is enforced at parse time by clap's
                // `conflicts_with`, so at most one of the two is true.
                let git_pull_change = if git_pull {
                    Some(true)
                } else if no_git_pull {
                    Some(false)
                } else {
                    None
                };
                let keep_awake_change = if keep_awake {
                    Some(true)
                } else if no_keep_awake {
                    Some(false)
                } else {
                    None
                };
                let upsert_mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec_resolved(value))
                    .collect::<Result<Vec<_>>>()?;

                let current_ws = config.require_workspace(&name)?.clone();

                let plan = workspace::planner::plan_edit(
                    &current_ws,
                    &upsert_mounts,
                    &remove_destinations,
                    no_workdir_mount,
                )?;

                // Reject pre-existing violations unless --prune.
                if !plan.pre_existing_collapses.is_empty() && !prune {
                    let details: Vec<String> = plan
                        .pre_existing_collapses
                        .iter()
                        .map(|r| {
                            format!(
                                "{} covered by {}",
                                tui::shorten_home(&r.child.src),
                                tui::shorten_home(&r.covered_by.src),
                            )
                        })
                        .collect();
                    anyhow::bail!(
                        "workspace {name:?} already contains redundant mounts:\n  - {}\n\
                         run `jackin workspace prune {name}` to clean up, or pass --prune to this edit",
                        details.join("\n  - ")
                    );
                }

                let all_collapses: Vec<&workspace::Removal> = plan
                    .edit_driven_collapses
                    .iter()
                    .chain(plan.pre_existing_collapses.iter())
                    .collect();

                // If there are any collapses to apply, prompt (or bail on
                // non-TTY without --yes).
                if !all_collapses.is_empty() && !assume_yes {
                    tui::require_interactive_stdin(
                        "refusing to collapse mounts without confirmation; pass --yes to proceed non-interactively",
                    )?;

                    if !plan.edit_driven_collapses.is_empty() {
                        eprintln!(
                            "Adding mount(s) will subsume {} existing mount(s):",
                            plan.edit_driven_collapses.len()
                        );
                        for r in &plan.edit_driven_collapses {
                            eprintln!("  • {}", tui::shorten_home(&r.child.src));
                        }
                    }
                    if !plan.pre_existing_collapses.is_empty() {
                        eprintln!(
                            "Cleaning up {} pre-existing redundant mount(s):",
                            plan.pre_existing_collapses.len()
                        );
                        for r in &plan.pre_existing_collapses {
                            eprintln!("  • {}", tui::shorten_home(&r.child.src));
                        }
                    }
                    eprintln!("These will be removed from the workspace.");

                    let confirmed = dialoguer::Confirm::new()
                        .with_prompt("Proceed?")
                        .default(false)
                        .interact()?;
                    if !confirmed {
                        anyhow::bail!("aborted by operator");
                    }
                }

                // Collect what changed for the summary (preserves the existing
                // summary output, plus collapse lines).
                let mut changes: Vec<String> = Vec::new();
                if let Some(ref w) = workdir {
                    changes.push(format!("workdir → {}", tui::shorten_home(w)));
                }
                for m in &upsert_mounts {
                    if all_collapses.iter().any(|r| r.child.dst == m.dst) {
                        continue;
                    }
                    if m.src == m.dst {
                        changes.push(format!("added mount {}", tui::shorten_home(&m.src)));
                    } else {
                        changes.push(format!(
                            "added mount {} → {}",
                            tui::shorten_home(&m.src),
                            tui::shorten_home(&m.dst)
                        ));
                    }
                }
                for dst in &remove_destinations {
                    changes.push(format!("removed mount {}", tui::shorten_home(dst)));
                }
                for r in &all_collapses {
                    changes.push(format!(
                        "collapsed {} under {}",
                        tui::shorten_home(&r.child.src),
                        tui::shorten_home(&r.covered_by.src)
                    ));
                }
                if no_workdir_mount {
                    changes.push("removed workdir auto-mount".to_string());
                }
                for role in &allowed_roles {
                    changes.push(format!("allowed role {role}"));
                }
                for role in &remove_allowed_roles {
                    changes.push(format!("removed role {role}"));
                }
                if clear_default_role {
                    changes.push("cleared default role".to_string());
                } else if let Some(ref role) = default_role {
                    changes.push(format!("default role → {role}"));
                }
                if clear_default_agent {
                    changes.push("cleared default agent".to_string());
                } else if let Some(agent) = default_agent {
                    changes.push(format!("default agent → {}", agent.slug()));
                }
                if let Some(v) = keep_awake_change {
                    changes.push(format!(
                        "keep-awake → {}",
                        if v { "enabled" } else { "disabled" }
                    ));
                }
                if let Some(v) = git_pull_change {
                    changes.push(format!(
                        "git-pull-on-entry → {}",
                        if v { "enabled" } else { "disabled" }
                    ));
                }

                // Build the prospective mount list (mirrors edit_workspace's
                // merge order) so we can check for source drift on any mount
                // that has preserved isolated state on disk.
                let mut prospective_mounts: Vec<workspace::MountConfig> = current_ws
                    .mounts
                    .iter()
                    .filter(|m| !plan.effective_removals.iter().any(|d| d == &m.dst))
                    .cloned()
                    .collect();
                if no_workdir_mount {
                    let workdir = &current_ws.workdir;
                    prospective_mounts.retain(|m| !(m.src == *workdir && m.dst == *workdir));
                }
                for upsert in &upsert_mounts {
                    if let Some(existing) = prospective_mounts
                        .iter_mut()
                        .find(|existing| existing.dst == upsert.dst)
                    {
                        *existing = upsert.clone();
                    } else {
                        prospective_mounts.push(upsert.clone());
                    }
                }
                let detection = crate::config::detect_workspace_edit_drift(
                    &paths,
                    &name,
                    &prospective_mounts,
                    &mut runner,
                )?;
                if !detection.running_containers.is_empty() {
                    anyhow::bail!(
                        "cannot edit workspace `{name}` while these containers are running with isolated state: {}; eject them first",
                        detection.running_containers.join(", ")
                    );
                }
                if !detection.stopped_records.is_empty() {
                    if !delete_isolated_state {
                        let names: Vec<String> = detection
                            .stopped_records
                            .iter()
                            .map(|r| r.container_name.clone())
                            .collect();
                        anyhow::bail!(
                            "edit affects preserved isolated state for {} container(s): {}; pass --delete-isolated-state to remove and apply, or restore the previous src",
                            detection.stopped_records.len(),
                            names.join(", ")
                        );
                    }
                    for rec in &detection.stopped_records {
                        let container_dir = paths.data_dir.join(&rec.container_name);
                        crate::isolation::cleanup::force_cleanup_isolated(
                            rec,
                            &container_dir,
                            &mut runner,
                        )?;
                    }
                }

                let mut editor = crate::config::ConfigEditor::open(&paths)?;
                editor.edit_workspace(
                    &name,
                    WorkspaceEdit {
                        workdir: workdir.map(|w| resolve_path(&w)),
                        upsert_mounts,
                        remove_destinations: plan.effective_removals,
                        no_workdir_mount,
                        allowed_roles_to_add: allowed_roles,
                        allowed_roles_to_remove: remove_allowed_roles,
                        default_role: if clear_default_role {
                            Some(None)
                        } else {
                            default_role.map(Some)
                        },
                        default_agent: if clear_default_agent {
                            Some(None)
                        } else {
                            default_agent.map(Some)
                        },
                        mount_isolation_overrides: mount_isolation,
                        keep_awake_enabled: keep_awake_change,
                        git_pull_on_entry_enabled: git_pull_change,
                    },
                )?;
                editor.save()?;
                println!("Updated workspace {name:?}:");
                for change in &changes {
                    println!("  - {change}");
                }
                Ok(())
            }
            WorkspaceCommand::Prune { name, assume_yes } => {
                let current_ws = config.require_workspace(&name)?.clone();

                // All existing mounts; nothing new.
                let plan = workspace::plan_collapse(&current_ws.mounts, &[])?;
                if plan.removed.is_empty() {
                    println!("Workspace {name:?} has no redundant mounts.");
                    return Ok(());
                }

                if !assume_yes {
                    tui::require_interactive_stdin(
                        "refusing to collapse mounts without confirmation; pass --yes to proceed non-interactively",
                    )?;
                    eprintln!(
                        "Will remove {} redundant mount(s) from workspace {name:?}:",
                        plan.removed.len()
                    );
                    for r in &plan.removed {
                        eprintln!(
                            "  • {} (covered by {})",
                            tui::shorten_home(&r.child.src),
                            tui::shorten_home(&r.covered_by.src),
                        );
                    }
                    let confirmed = dialoguer::Confirm::new()
                        .with_prompt("Proceed?")
                        .default(false)
                        .interact()?;
                    if !confirmed {
                        anyhow::bail!("aborted by operator");
                    }
                }

                let remove_dsts: Vec<String> =
                    plan.removed.iter().map(|r| r.child.dst.clone()).collect();
                let mut editor = crate::config::ConfigEditor::open(&paths)?;
                editor.edit_workspace(
                    &name,
                    WorkspaceEdit {
                        remove_destinations: remove_dsts,
                        ..WorkspaceEdit::default()
                    },
                )?;
                editor.save()?;
                println!(
                    "Pruned {} redundant mount(s) from workspace {name:?}.",
                    plan.removed.len()
                );
                Ok(())
            }
            WorkspaceCommand::Remove { name } => {
                let mut editor = crate::config::ConfigEditor::open(&paths)?;
                editor.remove_workspace(&name)?;
                editor.save()?;
                println!("Removed workspace {name:?}.");
                Ok(())
            }
            WorkspaceCommand::Env(env_cmd) => match env_cmd {
                cli::WorkspaceEnvCommand::Set {
                    workspace,
                    key,
                    value,
                    role,
                    comment,
                } => {
                    if key.is_empty() {
                        anyhow::bail!("env var key cannot be empty");
                    }
                    if crate::env_model::is_reserved(&key) {
                        anyhow::bail!(
                            "env name {key:?} is reserved by the jackin runtime and cannot be set"
                        );
                    }
                    config.require_workspace(&workspace)?;
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
                    let scope = workspace_env_scope(workspace, role);
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
                    editor.set_env_var(&scope, &key, env_value)?;
                    if let Some(ref c) = comment {
                        editor.set_env_comment(&scope, &key, Some(c));
                    }
                    editor.save()?;
                    println!("Set {key}.");
                    Ok(())
                }
                cli::WorkspaceEnvCommand::Unset {
                    workspace,
                    key,
                    role,
                } => {
                    if key.is_empty() {
                        anyhow::bail!("env var key cannot be empty");
                    }
                    let ws = config.require_workspace(&workspace)?;
                    // CLAUDE_CODE_OAUTH_TOKEN under oauth_token mode is owned
                    // by the claude-token orchestrator; an unset here would
                    // silently break auth at the next launch.
                    if key == crate::operator_env::CLAUDE_OAUTH_TOKEN_ENV
                        && role.is_none()
                        && ws.claude.as_ref().map(|c| c.auth_forward)
                            == Some(crate::config::AuthForwardMode::OAuthToken)
                    {
                        anyhow::bail!(
                            "CLAUDE_CODE_OAUTH_TOKEN is managed by \
                             `jackin workspace claude-token` — use \
                             `jackin workspace claude-token revoke {workspace}` \
                             to clear it"
                        );
                    }
                    let scope = workspace_env_scope(workspace, role);
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
                    if editor.remove_env_var(&scope, &key) {
                        editor.save()?;
                        println!("Removed {key}.");
                    } else {
                        drop(editor);
                        println!("{key} not set.");
                    }
                    Ok(())
                }
                cli::WorkspaceEnvCommand::List { workspace, role } => {
                    let ws = config.require_workspace(&workspace)?;
                    let vars: Vec<(String, String)> = role.as_ref().map_or_else(
                        || {
                            ws.env
                                .iter()
                                .map(|(k, v)| (k.clone(), v.as_display_str().to_string()))
                                .collect()
                        },
                        |a| {
                            ws.roles.get(a).map_or_else(Vec::new, |ov| {
                                ov.env
                                    .iter()
                                    .map(|(k, v)| (k.clone(), v.as_display_str().to_string()))
                                    .collect()
                            })
                        },
                    );
                    print_env_table(&vars);
                    Ok(())
                }
            },
            WorkspaceCommand::ClaudeToken(action) => {
                handle_claude_token(&paths, &mut config, action)
            }
        },
        Command::Purge(PurgeArgs { selector, all }) => {
            if let Some(container) = resolve_instance_reference(&paths, &selector)? {
                if all {
                    anyhow::bail!("--all applies only to role selectors, not instance IDs");
                }
                runtime::purge_container_state(&paths, &container, &mut runner)?;
                println!("Purged state for {container}.");
                return Ok(());
            }

            match Selector::parse(&selector)? {
                Selector::Container(container) => {
                    runtime::purge_container_state(&paths, &container, &mut runner)?;
                    println!("Purged state for {container}.");
                    Ok(())
                }
                Selector::Role(class) => {
                    if all {
                        runtime::purge_class_data(&paths, &class, &mut runner)?;
                        println!("Purged all state for {}.", class.key());
                    } else {
                        let container = resolve_role_to_container(&class, &mut runner)?;
                        runtime::purge_container_state(&paths, &container, &mut runner)?;
                        println!("Purged state for {container}.");
                    }
                    Ok(())
                }
            }
        }
        Command::Prune(cmd) => match cmd {
            PruneCommand::Roles => runtime::prune_roles(&paths),
            PruneCommand::Cache => runtime::prune_cache(&paths),
            PruneCommand::Images => runtime::prune_images(&mut runner),
            PruneCommand::Instances => runtime::prune_instances(&paths, &mut runner),
            PruneCommand::All(args) => {
                if !args.yes {
                    let confirmed = dialoguer::Confirm::new()
                        .with_prompt(
                            "Remove all prunable data? (instances, images, role cache, shared cache)",
                        )
                        .default(false)
                        .interact()?;
                    if !confirmed {
                        anyhow::bail!("aborted by operator");
                    }
                }
                // Run every step regardless of individual failures so a single
                // Docker error doesn't leave the role cache and shared cache
                // untouched.
                let results = [
                    runtime::prune_instances(&paths, &mut runner).context("prune instances"),
                    runtime::prune_images(&mut runner).context("prune images"),
                    runtime::prune_roles(&paths).context("prune roles"),
                    runtime::prune_cache(&paths).context("prune cache"),
                ];
                let errors: Vec<anyhow::Error> =
                    results.into_iter().filter_map(Result::err).collect();
                if errors.is_empty() {
                    Ok(())
                } else {
                    for err in &errors {
                        eprintln!("{} {err:#}", "error:".red().bold());
                    }
                    anyhow::bail!("{} prune step(s) failed", errors.len())
                }
            }
        },
        Command::Help { .. } => {
            // Handled upstream in dispatch before reaching this function.
            unreachable!("Command::Help is dispatched to Action::PrintHelp before run() is called")
        }
        Command::Role(_) => unreachable!("Command::Role returns before config-backed dispatch"),
    }
}

/// Resolve a CLI-supplied env value string into the appropriate [`EnvValue`]
/// variant. Values starting with `op://` trigger headless resolution via the
/// 1Password CLI; all other values are stored as [`EnvValue::Plain`] unchanged.
///
/// Errors when:
/// - The value starts with `op://` but the `op` CLI is unavailable.
/// - The value starts with `op://` and contains `${VAR}` substitution syntax.
/// - Resolution fails (vault/item/field not found, ambiguity, etc.).
fn resolve_env_value_for_cli(value: &str) -> anyhow::Result<crate::operator_env::EnvValue> {
    if !value.starts_with("op://") {
        return Ok(crate::operator_env::EnvValue::Plain(value.to_string()));
    }

    // Probe op CLI availability before attempting structural queries.
    let op_cli = crate::operator_env::OpCli::new();
    crate::operator_env::OpRunner::probe(&op_cli).map_err(|e| {
        anyhow::anyhow!(
            "`op` CLI not available; cannot resolve `op://...` reference. \
             Install 1Password CLI, or use a non-op:// value.\n\
             Probe error: {e}"
        )
    })?;

    let op_ref = crate::operator_env::resolve_op_uri_to_ref(value, &op_cli, None)?;
    Ok(crate::operator_env::EnvValue::OpRef(op_ref))
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
            vault,
            item_name,
            op_account,
            reuse,
        } => {
            let reuse_ref = reuse
                .as_deref()
                .map(|r| parse_reuse(r, op_account.as_deref()))
                .transpose()?;
            let args = token_setup::TokenSetupArgs {
                vault,
                item_name,
                account: op_account,
                reuse: reuse_ref,
            };
            let report = token_setup::run_setup(paths, config, &workspace, &args)?;
            print_token_setup_report(&report);
            Ok(())
        }
        cli::WorkspaceClaudeTokenCommand::Rotate {
            workspace,
            vault,
            item_name,
            op_account,
        } => {
            let prior = config
                .workspaces
                .get(&workspace)
                .and_then(|w| w.env.get(crate::operator_env::CLAUDE_OAUTH_TOKEN_ENV))
                .cloned();
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
            };
            let report = token_setup::run_setup(paths, config, &workspace, &args)?;
            print_token_setup_report(&report);
            delete_prior_op_item(prior, &report.op_ref, report.op_account)?;
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
                "op_account       {}",
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
    op_writer
        .item_delete(&parts.item, &parts.vault, None)
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
    println!("op:// reference  {}", report.op_ref.path);
    println!(
        "op_account       {}",
        report.op_account.as_deref().unwrap_or("(default)")
    );
    println!(
        "token sha256     {}… (12 hex prefix; matches stored value)",
        report.token_sha256_prefix
    );
    if let Some(expiry) = report.expiry_estimate.as_deref() {
        println!("expires (est.)   {expiry}");
    }
    println!("auth_forward     oauth_token (synthesised CLAUDE_CODE_OAUTH_TOKEN)");
    println!();
    if report.created {
        println!("New token captured and stored in 1Password.");
    } else {
        println!("Existing op:// reference adopted; no new item created.");
    }
}

#[derive(tabled::Tabled)]
struct EnvRow {
    #[tabled(rename = "Key")]
    key: String,
    #[tabled(rename = "Value")]
    value: String,
}

fn print_env_table(vars: &[(String, String)]) {
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
    table.with(Style::modern_rounded());
    println!("{table}");
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

fn prompt_explicit_hardline_action_if_multiple_sessions(
    container: &str,
    runner: &mut impl crate::docker::CommandRunner,
) -> Result<HardlineAction> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        return Ok(HardlineAction::Reconnect);
    }
    let state = runtime::inspect_container_state(runner, container);
    let sessions = runtime::inspect_agent_sessions(runner, container, &state);
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

fn handle_console_instance_action(
    paths: &JackinPaths,
    config: &mut AppConfig,
    outcome: console::ConsoleOutcome,
    runner: &mut ShellRunner,
) -> Result<()> {
    let console::ConsoleOutcome::InstanceAction { container, action } = outcome else {
        unreachable!("console launch outcomes are handled before instance actions")
    };
    match action {
        console::ConsoleInstanceAction::Reconnect => {
            runtime::reconcile_keep_awake(paths, runner);
            let result = if let Some(manifest) =
                restore_candidate_for_hardline(paths, &container, runner)?
            {
                restore_hardline_instance(paths, config, &manifest, runner)
            } else {
                runtime::hardline_agent(paths, &container, runner)
            };
            runtime::reconcile_keep_awake(paths, runner);
            result
        }
        console::ConsoleInstanceAction::NewSession => {
            let manifest = instance::InstanceManifest::read(&paths.data_dir.join(&container))
                .with_context(|| {
                    format!(
                        "cannot start a new agent session in `{container}` because its instance manifest is missing"
                    )
                })?;
            let selected_agent = resolve_new_session_agent(paths, config, &manifest)?;
            runtime::reconcile_keep_awake(paths, runner);
            let result = runtime::spawn_agent_session(
                paths,
                &container,
                Some(&manifest),
                selected_agent,
                runner,
            );
            runtime::reconcile_keep_awake(paths, runner);
            result
        }
        console::ConsoleInstanceAction::Shell => {
            runtime::spawn_shell_session(paths, &container, runner)
        }
        console::ConsoleInstanceAction::Inspect => {
            println!(
                "{}",
                runtime::inspect_hardline_instance(paths, &container, runner)?
            );
            Ok(())
        }
        console::ConsoleInstanceAction::Purge => {
            runtime::purge_container_state(paths, &container, runner)?;
            println!("Purged state for {container}.");
            Ok(())
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

fn resolve_role_to_container(
    class: &RoleSelector,
    runner: &mut impl crate::docker::CommandRunner,
) -> Result<String> {
    let candidates = runtime::matching_family(class, &runtime::list_managed_role_names(runner)?);
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

fn restore_candidate_for_hardline(
    paths: &JackinPaths,
    container: &str,
    runner: &mut impl crate::docker::CommandRunner,
) -> Result<Option<instance::InstanceManifest>> {
    let state_dir = paths.data_dir.join(container);
    let Some(mut manifest) = instance::InstanceManifest::read_optional(&state_dir)? else {
        return Ok(None);
    };
    if !manifest.is_restore_candidate() {
        return Ok(None);
    }

    match runtime::inspect_container_state(runner, container) {
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
        runtime::ContainerState::Running | runtime::ContainerState::Stopped { .. } => Ok(None),
    }
}

fn restore_hardline_instance(
    paths: &JackinPaths,
    config: &mut AppConfig,
    manifest: &instance::InstanceManifest,
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

    let sensitive = crate::workspace::find_sensitive_mounts(&workspace.mounts);
    if !sensitive.is_empty() && !crate::workspace::confirm_sensitive_mounts(&sensitive)? {
        anyhow::bail!("aborted — sensitive mount paths were not confirmed");
    }

    let opts = runtime::LoadOptions {
        agent: Some(manifest.agent()?),
        role_branch: manifest.role_source_ref.clone(),
        restore_container_base: Some(manifest.container_base.clone()),
        restore_role_source_git: Some(manifest.role_source_git.clone()),
        ..runtime::LoadOptions::default()
    };
    runtime::load_role(paths, config, &class, &workspace, runner, &opts)
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
                kind: crate::console::manager::mount_info::inspect(&m.src).label(),
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
mod auth_set_tests {
    use super::*;

    #[test]
    fn parse_auth_forward_mode_from_cli_accepts_sync() {
        let mode = parse_auth_forward_mode_from_cli("sync").unwrap();
        assert_eq!(mode, crate::config::AuthForwardMode::Sync);
    }

    #[test]
    fn parse_auth_forward_mode_from_cli_rejects_bogus() {
        assert!(parse_auth_forward_mode_from_cli("bogus").is_err());
    }

    #[test]
    fn auth_show_prints_builtin_agents() {
        // No global override means each agent falls through to its
        // default-mode (Sync). The point of this test is the output shape:
        // all built-in agents are surfaced, so a non-Claude-primary operator
        // running `jackin config auth show` is not silently shown only Claude.
        let config = AppConfig::default();
        let out = render_auth_show(&config);
        assert!(out.contains("claude:"), "missing claude line: {out}");
        assert!(out.contains("codex:"), "missing codex line: {out}");
        assert!(out.contains("amp:"), "missing amp line: {out}");
    }

    #[test]
    fn resolve_instance_reference_matches_manifest_instance_id() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
            container_base: "jk-k7p9m2xq-workspace-agentsmith",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jk_agent-smith",
            docker: instance::DockerResources {
                role_container: "jk-k7p9m2xq-workspace-agentsmith".to_string(),
                dind_container: "jk-k7p9m2xq-workspace-agentsmith-dind".to_string(),
                network: "jk-k7p9m2xq-workspace-agentsmith-net".to_string(),
                certs_volume: "jk-k7p9m2xq-workspace-agentsmith-dind-certs".to_string(),
            },
        });
        let state_dir = paths.data_dir.join(&manifest.container_base);
        manifest.write(&state_dir).unwrap();
        instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();

        let resolved = resolve_instance_reference(&paths, "k7p9m2xq").unwrap();

        assert_eq!(
            resolved.as_deref(),
            Some("jk-k7p9m2xq-workspace-agentsmith")
        );
    }

    #[test]
    fn resolve_instance_reference_ignores_purged_tombstones() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
            container_base: "jk-k7p9m2xq-workspace-agentsmith",
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jk_agent-smith",
            docker: instance::DockerResources {
                role_container: "jk-k7p9m2xq-workspace-agentsmith".to_string(),
                dind_container: "jk-k7p9m2xq-workspace-agentsmith-dind".to_string(),
                network: "jk-k7p9m2xq-workspace-agentsmith-net".to_string(),
                certs_volume: "jk-k7p9m2xq-workspace-agentsmith-dind-certs".to_string(),
            },
        });
        manifest.mark_status(instance::InstanceStatus::Purged);
        instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();

        let resolved = resolve_instance_reference(&paths, "k7p9m2xq").unwrap();

        assert!(resolved.is_none());
    }

    #[test]
    fn hardline_action_options_expose_recovery_controls() {
        let options = hardline_action_options();

        assert_eq!(options[0].1, HardlineAction::Reconnect);
        assert_eq!(options[1].1, HardlineAction::NewSession);
        assert_eq!(options[2].1, HardlineAction::Inspect);
        assert_eq!(options[3].1, HardlineAction::Cancel);
        assert!(options[1].0.contains("agent session"));
        assert!(options[2].0.contains("Inspect"));
    }

    #[test]
    fn explicit_hardline_prompts_only_for_multiple_agent_sessions() {
        assert!(!has_multiple_agent_sessions(
            &runtime::AgentSessionInventory::NotRunning
        ));
        assert!(!has_multiple_agent_sessions(
            &runtime::AgentSessionInventory::Sessions(vec![runtime::AgentSession {
                pid: "1".to_string(),
                command: "claude".to_string(),
            }])
        ));
        assert!(has_multiple_agent_sessions(
            &runtime::AgentSessionInventory::Sessions(vec![
                runtime::AgentSession {
                    pid: "1".to_string(),
                    command: "claude".to_string(),
                },
                runtime::AgentSession {
                    pid: "2".to_string(),
                    command: "codex".to_string(),
                },
            ])
        ));
    }

    #[test]
    fn ad_hoc_restore_input_accepts_original_project_directory() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let project = project.canonicalize().unwrap();
        let manifest = ad_hoc_manifest_for_workdir(&project);

        let input = ad_hoc_restore_input_for_current_dir(&manifest, &project, false);

        assert!(matches!(input, Some(LoadWorkspaceInput::CurrentDir)));
    }

    #[test]
    fn ad_hoc_restore_input_can_use_confirmed_moved_project_directory() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original");
        let moved = temp.path().join("moved");
        std::fs::create_dir(&original).unwrap();
        std::fs::create_dir(&moved).unwrap();
        let original = original.canonicalize().unwrap();
        let moved = moved.canonicalize().unwrap();
        let manifest = ad_hoc_manifest_for_workdir(&original);

        assert!(ad_hoc_restore_input_for_current_dir(&manifest, &moved, false).is_none());
        let input = ad_hoc_restore_input_for_current_dir(&manifest, &moved, true);

        match input {
            Some(LoadWorkspaceInput::Path { src, dst }) => {
                assert_eq!(src, moved.display().to_string());
                assert_eq!(dst, original.display().to_string());
            }
            other => panic!("expected moved project path input; got {other:?}"),
        }
    }

    #[test]
    fn ad_hoc_restore_input_can_use_entered_moved_project_path() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original");
        let moved = temp.path().join("moved");
        std::fs::create_dir(&original).unwrap();
        std::fs::create_dir(&moved).unwrap();
        let original = original.canonicalize().unwrap();
        let moved = moved.canonicalize().unwrap();
        let manifest = ad_hoc_manifest_for_workdir(&original);

        let input = ad_hoc_restore_input_for_moved_path(&manifest, &moved);

        match input {
            Some(LoadWorkspaceInput::Path { src, dst }) => {
                assert_eq!(src, moved.display().to_string());
                assert_eq!(dst, original.display().to_string());
            }
            other => panic!("expected moved project path input; got {other:?}"),
        }
    }

    #[test]
    fn ad_hoc_restore_input_rejects_missing_entered_moved_project_path() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original");
        std::fs::create_dir(&original).unwrap();
        let original = original.canonicalize().unwrap();
        let manifest = ad_hoc_manifest_for_workdir(&original);

        let input = ad_hoc_restore_input_for_moved_path(&manifest, &temp.path().join("missing"));

        assert!(input.is_none());
    }

    #[test]
    fn classify_moved_path_entry_empty_input_cancels() {
        assert!(matches!(
            classify_moved_path_entry(""),
            MovedPathEntryStep::Cancel
        ));
        assert!(matches!(
            classify_moved_path_entry("   \t  "),
            MovedPathEntryStep::Cancel
        ));
    }

    #[test]
    fn classify_moved_path_entry_accepts_existing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("project");
        std::fs::create_dir_all(&dir).unwrap();
        match classify_moved_path_entry(&dir.display().to_string()) {
            MovedPathEntryStep::Accepted(p) => {
                assert_eq!(p, dir.canonicalize().unwrap());
            }
            other => panic!("expected Accepted, got {other:?}"),
        }
    }

    #[test]
    fn classify_moved_path_entry_rejects_regular_file_with_retry() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("not-a-dir");
        std::fs::write(&file, "").unwrap();
        match classify_moved_path_entry(&file.display().to_string()) {
            MovedPathEntryStep::Retry(msg) => assert!(msg.contains("not a directory"), "{msg}"),
            other => panic!("expected Retry, got {other:?}"),
        }
    }

    #[test]
    fn classify_moved_path_entry_rejects_missing_path_with_retry() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("does-not-exist");
        match classify_moved_path_entry(&missing.display().to_string()) {
            MovedPathEntryStep::Retry(msg) => assert!(msg.contains("cannot use"), "{msg}"),
            other => panic!("expected Retry, got {other:?}"),
        }
    }

    impl std::fmt::Debug for MovedPathEntryStep {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Cancel => write!(f, "Cancel"),
                Self::Accepted(p) => write!(f, "Accepted({})", p.display()),
                Self::Retry(s) => write!(f, "Retry({s})"),
            }
        }
    }

    #[test]
    fn moved_path_browser_choices_include_parent_sorted_children_and_manual_escape() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let cwd = root.join("current");
        let alpha = cwd.join("alpha");
        let beta = cwd.join("Beta");
        std::fs::create_dir_all(&beta).unwrap();
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::write(cwd.join("not-a-dir"), "").unwrap();

        let choices = moved_path_browser_choices(&cwd);

        assert_eq!(
            choices,
            vec![
                MovedPathBrowserChoice::SelectCurrent(cwd.canonicalize().unwrap()),
                MovedPathBrowserChoice::Parent(root.canonicalize().unwrap()),
                MovedPathBrowserChoice::Child(alpha.canonicalize().unwrap()),
                MovedPathBrowserChoice::Child(beta.canonicalize().unwrap()),
                MovedPathBrowserChoice::Manual,
                MovedPathBrowserChoice::Cancel,
            ]
        );
    }

    fn ad_hoc_manifest_for_workdir(workdir: &std::path::Path) -> instance::InstanceManifest {
        let workdir = workdir.display().to_string();
        instance::InstanceManifest::new(instance::NewInstanceManifest {
            container_base: "jk-k7p9m2xq-agentsmith",
            workspace_name: None,
            workspace_label: &workdir,
            workdir: &workdir,
            host_workdir_fingerprint: &instance::manifest::host_path_fingerprint(&workdir),
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jk_agent-smith",
            docker: instance::DockerResources {
                role_container: "jk-k7p9m2xq-agentsmith".to_string(),
                dind_container: "jk-k7p9m2xq-agentsmith-dind".to_string(),
                network: "jk-k7p9m2xq-agentsmith-net".to_string(),
                certs_volume: "jk-k7p9m2xq-agentsmith-dind-certs".to_string(),
            },
        })
    }

    #[test]
    fn hardline_restore_candidate_marks_missing_manifest_available() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container = "jk-k7p9m2xq-workspace-agentsmith";
        let mut manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
            container_base: container,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jk_agent-smith",
            docker: instance::DockerResources {
                role_container: container.to_string(),
                dind_container: format!("{container}-dind"),
                network: format!("{container}-net"),
                certs_volume: format!("{container}-dind-certs"),
            },
        });
        manifest.mark_status(instance::InstanceStatus::Crashed);
        let state_dir = paths.data_dir.join(container);
        manifest.write(&state_dir).unwrap();
        instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
        let mut runner = runtime::FakeRunner::default();

        let candidate = restore_candidate_for_hardline(&paths, container, &mut runner)
            .unwrap()
            .expect("missing crashed manifest should restore");

        assert_eq!(candidate.container_base, container);
        let manifest = instance::InstanceManifest::read(&state_dir).unwrap();
        assert_eq!(manifest.status, instance::InstanceStatus::RestoreAvailable);
        let index = instance::InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert_eq!(
            index.instances[0].status,
            instance::InstanceStatus::RestoreAvailable
        );
    }

    #[test]
    fn hardline_restore_candidate_errors_when_docker_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let container = "jk-k7p9m2xq-workspace-agentsmith";
        let mut manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
            container_base: container,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jk_agent-smith",
            docker: instance::DockerResources {
                role_container: container.to_string(),
                dind_container: format!("{container}-dind"),
                network: format!("{container}-net"),
                certs_volume: format!("{container}-dind-certs"),
            },
        });
        manifest.mark_status(instance::InstanceStatus::Crashed);
        manifest.write(&paths.data_dir.join(container)).unwrap();
        let mut runner = runtime::FakeRunner::default();
        runner.fail_with.push((
            "docker inspect".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));

        let error = restore_candidate_for_hardline(&paths, container, &mut runner).unwrap_err();

        assert!(error.to_string().contains("Docker is unavailable"));
    }

    #[test]
    fn workspace_show_includes_isolation_column() {
        let temp = tempfile::tempdir().unwrap();
        let worktree_src = temp.path().join("x");
        let cache_src = temp.path().join("cache");
        std::fs::create_dir_all(&worktree_src).unwrap();
        std::fs::create_dir_all(&cache_src).unwrap();
        let ws = crate::workspace::WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: "/workspace/jackin".into(),
            mounts: vec![
                crate::workspace::MountConfig {
                    src: worktree_src.display().to_string(),
                    dst: "/workspace/jackin".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Worktree,
                },
                crate::workspace::MountConfig {
                    src: cache_src.display().to_string(),
                    dst: "/workspace/cache".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ],
            allowed_roles: vec![],
            default_role: None,
            default_agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: crate::workspace::KeepAwakeConfig::default(),
            op_account: None,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            github: None,
            git_pull_on_entry: false,
        };
        let out = render_workspace_show(&AppConfig::default(), "jackin", &ws);
        assert!(out.contains("Isolation"));
        assert!(out.contains("Type"));
        assert!(out.contains("folder"));
        assert!(out.contains("worktree"));
        assert!(out.contains("shared"));
    }

    #[test]
    fn workspace_show_splits_workspace_and_global_mount_groups() {
        let temp = tempfile::tempdir().unwrap();
        let global_src = temp.path().join("gradle");
        std::fs::create_dir_all(&global_src).unwrap();
        let work_src = temp.path().join("work");
        std::fs::create_dir_all(&work_src).unwrap();
        let mut config = AppConfig::default();
        config
            .roles
            .insert("agent-smith".into(), crate::config::RoleSource::default());
        config.add_mount(
            "gradle-cache",
            crate::workspace::MountConfig {
                src: global_src.display().to_string(),
                dst: "/home/agent/.gradle/caches".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
            None,
        );
        let ws = crate::workspace::WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: "/workspace/jackin".into(),
            mounts: vec![crate::workspace::MountConfig {
                src: work_src.display().to_string(),
                dst: "/workspace/jackin".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles: vec!["agent-smith".into()],
            ..Default::default()
        };

        let out = render_workspace_show(&config, "jackin", &ws);

        assert!(out.contains("Workspace mounts:"), "{out}");
        assert!(out.contains("Global mounts:"), "{out}");
        assert!(!out.contains("Global mounts (agent-smith):"), "{out}");
        assert!(out.contains("gradle-cache"), "{out}");
        assert!(!out.contains("│ Scope"), "{out}");
    }

    #[test]
    fn workspace_show_explains_ambiguous_role_scoped_global_mounts() {
        let temp = tempfile::tempdir().unwrap();
        let global_src = temp.path().join("secrets");
        std::fs::create_dir_all(&global_src).unwrap();
        let mut config = AppConfig::default();
        config
            .roles
            .insert("alpha".into(), crate::config::RoleSource::default());
        config
            .roles
            .insert("beta".into(), crate::config::RoleSource::default());
        config.add_mount(
            "team-secrets",
            crate::workspace::MountConfig {
                src: global_src.display().to_string(),
                dst: "/secrets".into(),
                readonly: true,
                isolation: crate::isolation::MountIsolation::Shared,
            },
            Some("alpha"),
        );
        let ws = crate::workspace::WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: "/workspace/jackin".into(),
            mounts: vec![],
            allowed_roles: vec!["alpha".into(), "beta".into()],
            ..Default::default()
        };

        let out = render_workspace_show(&config, "jackin", &ws);

        assert!(out.contains("selected role"), "{out}");
        assert!(!out.contains("team-secrets"), "{out}");
    }

    #[test]
    fn workspace_show_keeps_scope_column_for_scoped_global_mounts() {
        let temp = tempfile::tempdir().unwrap();
        let global_src = temp.path().join("secrets");
        std::fs::create_dir_all(&global_src).unwrap();
        let mut config = AppConfig::default();
        config.roles.insert(
            "chainargos/agent-brown".into(),
            crate::config::RoleSource::default(),
        );
        config.add_mount(
            "team-secrets",
            crate::workspace::MountConfig {
                src: global_src.display().to_string(),
                dst: "/secrets".into(),
                readonly: true,
                isolation: crate::isolation::MountIsolation::Shared,
            },
            Some("chainargos/*"),
        );
        let ws = crate::workspace::WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: "/workspace/jackin".into(),
            mounts: vec![],
            allowed_roles: vec!["chainargos/agent-brown".into()],
            ..Default::default()
        };

        let out = render_workspace_show(&config, "jackin", &ws);

        assert!(
            out.contains("Global mounts (chainargos/agent-brown):"),
            "{out}"
        );
        assert!(out.contains("│ Scope"), "{out}");
        assert!(out.contains("chainargos/*"), "{out}");
    }

    /// Test fake for [`crate::operator_env::OpWriteRunner`] used by
    /// the rotate-cleanup tests below.
    struct FakeOpWriter {
        deletes: std::cell::RefCell<Vec<(String, String)>>,
        fail_delete: bool,
    }
    impl FakeOpWriter {
        fn new() -> Self {
            Self {
                deletes: std::cell::RefCell::new(Vec::new()),
                fail_delete: false,
            }
        }
        fn failing() -> Self {
            Self {
                deletes: std::cell::RefCell::new(Vec::new()),
                fail_delete: true,
            }
        }
    }
    impl crate::operator_env::OpWriteRunner for FakeOpWriter {
        fn item_create(
            &self,
            _params: crate::operator_env::OpItemCreateParams<'_>,
        ) -> anyhow::Result<crate::operator_env::OpRef> {
            anyhow::bail!("rotate-cleanup tests do not exercise item_create")
        }
        fn item_delete(
            &self,
            item_id: &str,
            vault_id: &str,
            _account: Option<&str>,
        ) -> anyhow::Result<()> {
            self.deletes
                .borrow_mut()
                .push((vault_id.to_string(), item_id.to_string()));
            if self.fail_delete {
                anyhow::bail!("simulated item_delete failure");
            }
            Ok(())
        }
    }

    /// Rotate's prior-item cleanup parses the prior op:// reference,
    /// issues a delete with the parsed UUIDs, and returns Ok.
    #[test]
    fn delete_prior_op_item_with_op_ref_calls_writer_with_parsed_uuids() {
        let prior = Some(crate::operator_env::EnvValue::OpRef(
            crate::operator_env::OpRef {
                op: "op://VAULT_UUID/OLD_ITEM/FIELD".into(),
                path: "Personal/Prior/token".into(),
            },
        ));
        let new_ref = crate::operator_env::OpRef {
            op: "op://VAULT_UUID/NEW_ITEM/FIELD".into(),
            path: "Personal/New/token".into(),
        };
        let writer = FakeOpWriter::new();
        delete_prior_op_item_with_runner(prior, &new_ref, &writer).unwrap();
        assert_eq!(
            *writer.deletes.borrow(),
            vec![("VAULT_UUID".to_string(), "OLD_ITEM".to_string())],
        );
    }

    /// Rotate's prior-item cleanup is a no-op when the prior slot is
    /// `None` or holds a literal token — jackin does not know where
    /// the literal came from.
    #[test]
    fn delete_prior_op_item_skips_when_prior_is_none_or_literal() {
        let new_ref = crate::operator_env::OpRef {
            op: "op://V/I/F".into(),
            path: "Personal/New/token".into(),
        };
        let writer = FakeOpWriter::new();
        delete_prior_op_item_with_runner(None, &new_ref, &writer).unwrap();
        assert!(writer.deletes.borrow().is_empty());

        let writer = FakeOpWriter::new();
        delete_prior_op_item_with_runner(
            Some(crate::operator_env::EnvValue::Plain("literal".into())),
            &new_ref,
            &writer,
        )
        .unwrap();
        assert!(writer.deletes.borrow().is_empty());
    }

    /// Rotate must NOT delete the new item it just created if the
    /// new and prior `op://` references are equal — a same-ref result
    /// indicates a deeper bug, but the safety guard prevents data
    /// loss until the operator runs `doctor`.
    #[test]
    fn delete_prior_op_item_skips_when_new_ref_equals_prior() {
        let same = crate::operator_env::OpRef {
            op: "op://V/I/F".into(),
            path: "Personal/Item/token".into(),
        };
        let writer = FakeOpWriter::new();
        delete_prior_op_item_with_runner(
            Some(crate::operator_env::EnvValue::OpRef(same.clone())),
            &same,
            &writer,
        )
        .unwrap();
        assert!(writer.deletes.borrow().is_empty());
    }

    /// `op item delete` failure promotes to whole-rotate `Err` with
    /// a copy-pasteable manual-delete command, so exit-code-driven
    /// automation surfaces the orphan.
    #[test]
    fn delete_prior_op_item_propagates_err_with_actionable_hint() {
        let prior = Some(crate::operator_env::EnvValue::OpRef(
            crate::operator_env::OpRef {
                op: "op://V_UUID/I_UUID/F".into(),
                path: "Personal/Prior/token".into(),
            },
        ));
        let new_ref = crate::operator_env::OpRef {
            op: "op://V_UUID/I_NEW/F".into(),
            path: "Personal/New/token".into(),
        };
        let writer = FakeOpWriter::failing();
        let err = delete_prior_op_item_with_runner(prior, &new_ref, &writer).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("simulated item_delete failure"), "got: {msg}");
        assert!(
            msg.contains("op item delete I_UUID --vault V_UUID"),
            "must include copy-pasteable recovery command, got: {msg}"
        );
    }
}

#[cfg(test)]
mod resolve_role_tests {
    use super::*;

    #[test]
    fn resolve_role_no_match_errors() {
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = runtime::FakeRunner::default();
        runner.capture_queue.push_back(String::new());
        let err = resolve_role_to_container(&selector, &mut runner).unwrap_err();
        assert!(
            err.to_string().contains("no managed container found"),
            "{err}"
        );
    }

    #[test]
    fn resolve_role_multiple_matches_errors_with_names() {
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = runtime::FakeRunner::default();
        runner
            .capture_queue
            .push_back("jk-k7p9m2xq-agentsmith\njk-a1b2c3d4-agentsmith".to_string());
        let err = resolve_role_to_container(&selector, &mut runner).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("multiple containers found"), "{msg}");
        assert!(msg.contains("jk-k7p9m2xq-agentsmith"), "{msg}");
        assert!(msg.contains("jk-a1b2c3d4-agentsmith"), "{msg}");
    }

    #[test]
    fn resolve_role_single_match_returns_name() {
        let selector = RoleSelector::new(None, "agent-smith");
        let mut runner = runtime::FakeRunner::default();
        runner
            .capture_queue
            .push_back("jk-k7p9m2xq-agentsmith".to_string());
        let name = resolve_role_to_container(&selector, &mut runner).unwrap();
        assert_eq!(name, "jk-k7p9m2xq-agentsmith");
    }
}
