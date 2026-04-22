pub mod cli;
pub mod config;
pub mod derived_image;
pub mod docker;
pub mod env_resolver;
pub mod instance;
pub mod launch;
pub mod manifest;
pub mod paths;
pub mod repo;
pub mod repo_contract;
pub mod runtime;
pub mod selector;
pub mod terminal_prompter;
pub mod tui;
pub mod version_check;
pub mod workspace;

use anyhow::Result;
use cli::{Cli, Command, WorkspaceCommand};
use config::AppConfig;
use docker::ShellRunner;
use paths::JackinPaths;
use selector::{ClassSelector, Selector};
use std::io::ErrorKind;
use std::path::Path;
use workspace::{
    LoadWorkspaceInput, WorkspaceConfig, WorkspaceEdit, expand_tilde, parse_mount_spec_resolved,
    resolve_path,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetKind {
    Path { src: String, dst: String },
    Name(String),
}

/// Classify a target string as either a path or a plain name.
///
/// Contains `/`, or starts with `.` or `~` => always a path.
/// Otherwise => a plain name (workspace or directory name).
pub fn classify_target(target: &str) -> TargetKind {
    if target.contains('/') || target.starts_with('.') || target.starts_with('~') {
        // Parse optional :dst — but be careful with src:dst vs path-only.
        // A target like ~/Projects/my-app:/app has the pattern host:container.
        // We split on the LAST colon that is followed by a `/` at position 0
        // (i.e., an absolute container path), to distinguish from :ro suffix.
        //
        // Strategy: if there's a colon where the right side starts with `/`,
        // treat it as src:dst.
        let (src, dst) = if let Some(pos) = find_dst_separator(target) {
            (&target[..pos], &target[pos + 1..])
        } else {
            // Same path for both src and dst — expand tilde for dst too.
            let expanded = expand_tilde(target);
            return TargetKind::Path {
                src: target.to_string(),
                dst: expanded,
            };
        };
        TargetKind::Path {
            src: src.to_string(),
            dst: dst.to_string(),
        }
    } else {
        TargetKind::Name(target.to_string())
    }
}

/// Find the colon that separates src:dst in a target spec.
/// The dst part must start with `/` (absolute container path).
fn find_dst_separator(target: &str) -> Option<usize> {
    // Search for `:` followed by `/`
    for (i, _) in target.match_indices(':') {
        if target[i + 1..].starts_with('/') {
            return Some(i);
        }
    }
    None
}

fn resolve_target_name(name: &str, config: &AppConfig, cwd: &Path) -> Result<LoadWorkspaceInput> {
    let workspace_exists = config.workspaces.contains_key(name);
    let dir_exists = cwd.join(name).is_dir();

    match (workspace_exists, dir_exists) {
        (true, true) => {
            let choice = tui::prompt_choice(
                &format!("\"{name}\" matches both a saved workspace and a directory."),
                &[
                    &format!("Use workspace \"{name}\""),
                    &format!("Use directory ./{name}"),
                ],
            )?;
            if choice == 0 {
                Ok(LoadWorkspaceInput::Saved(name.to_string()))
            } else {
                let full_path = cwd.join(name);
                let canonical = full_path.display().to_string();
                Ok(LoadWorkspaceInput::Path {
                    src: canonical.clone(),
                    dst: canonical,
                })
            }
        }
        (true, false) => Ok(LoadWorkspaceInput::Saved(name.to_string())),
        (false, true) => {
            let full_path = cwd.join(name);
            let canonical = full_path.display().to_string();
            Ok(LoadWorkspaceInput::Path {
                src: canonical.clone(),
                dst: canonical,
            })
        }
        (false, false) => {
            anyhow::bail!(
                "\"{name}\" is neither a saved workspace nor a directory in the current path.\n\
                 Saved workspaces: {}\n\
                 Hint: use a path (e.g. ./{name}) to mount a directory.",
                if config.workspaces.is_empty() {
                    "(none)".to_string()
                } else {
                    config
                        .workspaces
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            );
        }
    }
}

/// Find the saved workspace whose host workdir or mounted host path best
/// matches `cwd`. Returns `None` when no saved workspace covers the path.
///
/// Shared by context resolvers for `jackin load` (class lookup) and
/// `jackin hardline` (running-container lookup).
fn find_saved_workspace_for_cwd<'a>(
    config: &'a AppConfig,
    cwd: &Path,
) -> Option<(&'a str, &'a WorkspaceConfig)> {
    config
        .workspaces
        .iter()
        .filter_map(|(name, ws)| {
            crate::workspace::saved_workspace_match_depth(ws, cwd).map(|depth| (name, ws, depth))
        })
        .max_by_key(|(_, _, depth)| *depth)
        .map(|(name, ws, _)| (name.as_str(), ws))
}

/// Resolve the agent and workspace from the current directory context.
///
/// Finds the saved workspace whose host workdir or mounted host path best
/// matches `cwd`, then picks the agent:
/// 1. `last_agent` (most recently used)
/// 2. `default_agent` (explicitly configured)
/// 3. If multiple agents available — prompt
/// 4. If exactly one agent — use it
/// 5. No match — error with guidance
fn resolve_agent_from_context(
    config: &AppConfig,
    cwd: &Path,
) -> Result<(ClassSelector, LoadWorkspaceInput)> {
    if let Some((name, ws)) = find_saved_workspace_for_cwd(config, cwd) {
        // Try last_agent, then default_agent
        let agent_key = ws.last_agent.as_deref().or(ws.default_agent.as_deref());

        if let Some(key) = agent_key
            && config.agents.contains_key(key)
        {
            let class = ClassSelector::parse(key)?;
            let allowed = ws.allowed_agents.is_empty()
                || ws.allowed_agents.iter().any(|allowed| allowed == key);
            if allowed {
                return Ok((class, LoadWorkspaceInput::Saved(name.to_string())));
            }
        }

        // No remembered agent — find eligible agents
        let eligible: Vec<ClassSelector> = config
            .agents
            .keys()
            .filter_map(|k| ClassSelector::parse(k).ok())
            .filter(|agent| {
                ws.allowed_agents.is_empty() || ws.allowed_agents.iter().any(|a| a == &agent.key())
            })
            .collect();

        match eligible.len() {
            0 => anyhow::bail!("no agents configured; add one with jackin load <agent>"),
            1 => {
                return Ok((
                    eligible.into_iter().next().unwrap(),
                    LoadWorkspaceInput::Saved(name.to_string()),
                ));
            }
            _ => {
                let options: Vec<String> = eligible.iter().map(ClassSelector::key).collect();
                let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
                let choice = tui::prompt_choice(
                    &format!("Workspace {name:?} has multiple agents. Select one:"),
                    &option_refs,
                )?;
                return Ok((
                    eligible[choice].clone(),
                    LoadWorkspaceInput::Saved(name.to_string()),
                ));
            }
        }
    }

    anyhow::bail!(
        "no saved workspace matches the current directory.\n\
         Run jackin load <agent> to use the current directory, or\n\
         run jackin launch for the interactive launcher."
    );
}

/// Resolve a running agent container from the current directory context.
///
/// Finds the saved workspace whose host workdir or mounted host path best
/// matches `cwd`, then picks a currently-running container whose class is
/// permitted by the workspace:
/// 1. If the workspace's `last_agent` has a running container — prefer it
/// 2. If exactly one running candidate — use it
/// 3. If multiple — prompt
/// 4. If zero — error with guidance to run `jackin load`
/// 5. No workspace match — error with guidance to pass an explicit selector
fn resolve_running_container_from_context(
    config: &AppConfig,
    cwd: &Path,
    runner: &mut impl docker::CommandRunner,
) -> Result<String> {
    let Some((name, ws)) = find_saved_workspace_for_cwd(config, cwd) else {
        anyhow::bail!(
            "no saved workspace matches the current directory.\n\
             Run jackin hardline <agent> to target explicitly, or\n\
             run jackin load <agent> to start a new session."
        );
    };

    let allowed_classes: Vec<ClassSelector> = if ws.allowed_agents.is_empty() {
        config
            .agents
            .keys()
            .filter_map(|k| ClassSelector::parse(k).ok())
            .collect()
    } else {
        ws.allowed_agents
            .iter()
            .filter_map(|k| ClassSelector::parse(k).ok())
            .collect()
    };

    let running = runtime::list_running_agent_names(runner)?;
    let mut candidates: Vec<String> = allowed_classes
        .iter()
        .flat_map(|class| runtime::matching_family(class, &running))
        .collect();
    candidates.sort();
    candidates.dedup();

    if let Some(last) = ws.last_agent.as_deref()
        && let Ok(last_class) = ClassSelector::parse(last)
    {
        let primary = crate::instance::primary_container_name(&last_class);
        if candidates.contains(&primary) {
            return Ok(primary);
        }
    }

    match candidates.len() {
        0 => anyhow::bail!(
            "no running agents found for workspace {name:?}.\n\
             Start one with jackin load, or run jackin hardline <agent> to target explicitly."
        ),
        1 => Ok(candidates.into_iter().next().unwrap()),
        _ => {
            let option_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
            let choice = tui::prompt_choice(
                &format!("Workspace {name:?} has multiple running agents. Select one:"),
                &option_refs,
            )?;
            Ok(candidates.into_iter().nth(choice).unwrap())
        }
    }
}

fn remember_last_agent(
    paths: &JackinPaths,
    config: &mut AppConfig,
    workspace_name: Option<&str>,
    class: &ClassSelector,
    load_result: &Result<()>,
) {
    if load_result.is_err() {
        return;
    }

    if let Some(workspace_name) = workspace_name
        && let Some(workspace) = config.workspaces.get_mut(workspace_name)
    {
        workspace.last_agent = Some(class.key());
        if let Err(error) = config.save(paths) {
            eprintln!("warning: failed to save last-used agent: {error}");
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn run(cli: Cli) -> Result<()> {
    let paths = JackinPaths::detect()?;
    let mut config = AppConfig::load_or_init(&paths)?;
    let mut runner = ShellRunner::default();

    match cli.command {
        Command::Load {
            selector,
            target,
            mounts,
            rebuild,
            no_intro,
            debug,
        } => {
            runner.debug = debug;
            tui::set_debug_mode(debug);
            let cwd = std::env::current_dir()?;

            let (class, workspace_input) = if let Some(sel) = selector {
                let class = ClassSelector::parse(&sel)?;
                let input = match target {
                    None => LoadWorkspaceInput::CurrentDir,
                    Some(t) => match classify_target(&t) {
                        TargetKind::Path { src, dst } => LoadWorkspaceInput::Path { src, dst },
                        TargetKind::Name(name) => resolve_target_name(&name, &config, &cwd)?,
                    },
                };
                (class, input)
            } else {
                // No selector — resolve agent from workspace context
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

            let opts = runtime::LoadOptions::for_load(no_intro, debug, rebuild);
            let result = runtime::load_agent(
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
            result
        }
        Command::Launch { debug } => {
            runner.debug = debug;
            tui::set_debug_mode(debug);
            let cwd = std::env::current_dir()?;
            let (class, workspace) = crate::launch::run_launch(&config, &cwd)?;

            let sensitive = crate::workspace::find_sensitive_mounts(&workspace.mounts);
            if !sensitive.is_empty() && !crate::workspace::confirm_sensitive_mounts(&sensitive)? {
                anyhow::bail!("aborted — sensitive mount paths were not confirmed");
            }

            let opts = runtime::LoadOptions::for_launch(debug);
            let result =
                runtime::load_agent(&paths, &mut config, &class, &workspace, &mut runner, &opts);
            remember_last_agent(&paths, &mut config, Some(&workspace.label), &class, &result);
            result
        }
        Command::Hardline { selector } => {
            let container = if let Some(sel) = selector {
                match Selector::parse(&sel)? {
                    Selector::Container(name) => name,
                    Selector::Class(class) => crate::instance::primary_container_name(&class),
                }
            } else {
                let cwd = std::env::current_dir()?;
                resolve_running_container_from_context(&config, &cwd, &mut runner)?
            };
            runtime::hardline_agent(&container, &mut runner)
        }
        Command::Eject {
            selector,
            all,
            purge,
        } => {
            let containers = match Selector::parse(&selector)? {
                Selector::Container(container) => vec![container],
                Selector::Class(class) => {
                    if all {
                        runtime::matching_family(
                            &class,
                            &runtime::list_managed_agent_names(&mut runner)?,
                        )
                    } else {
                        vec![crate::instance::primary_container_name(&class)]
                    }
                }
            };
            if containers.is_empty() {
                println!("No matching agents found.");
            } else {
                for container in &containers {
                    runtime::eject_agent(container, &mut runner)?;
                    if purge {
                        remove_data_dir_if_exists(&paths.data_dir.join(container))?;
                        println!("Ejected and purged {container}.");
                    } else {
                        println!("Ejected {container}.");
                    }
                }
            }
            Ok(())
        }
        Command::Exile => {
            let names = runtime::list_managed_agent_names(&mut runner)?;
            if names.is_empty() {
                println!("No agents running.");
            } else {
                for name in &names {
                    runtime::eject_agent(name, &mut runner)?;
                    println!("Ejected {name}.");
                }
            }
            Ok(())
        }
        Command::Config {
            command: config_cmd,
        } => match config_cmd {
            cli::ConfigCommand::Mount { command: mount_cmd } => match mount_cmd {
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
                    };
                    config.add_mount(&name, mount, scope.as_deref());
                    config.save(&paths)?;
                    println!("Added mount {name:?} ({scope_label}): {src} -> {dst}{ro}");
                    Ok(())
                }
                cli::MountCommand::Remove { name, scope } => {
                    if config.remove_mount(&name, scope.as_deref()) {
                        config.save(&paths)?;
                        println!("Removed mount {name:?}.");
                    } else {
                        println!("Mount {name:?} not found.");
                    }
                    Ok(())
                }
                cli::MountCommand::List => {
                    let mounts = config.list_mounts();
                    if mounts.is_empty() {
                        println!("No mounts configured.");
                    } else {
                        use tabled::settings::Style;
                        use tabled::{Table, Tabled};
                        #[derive(Tabled)]
                        struct Row {
                            #[tabled(rename = "Scope")]
                            scope: String,
                            #[tabled(rename = "Name")]
                            name: String,
                            #[tabled(rename = "Source")]
                            src: String,
                            #[tabled(rename = "Destination")]
                            dst: String,
                            #[tabled(rename = "Mode")]
                            mode: String,
                        }
                        let rows: Vec<Row> = mounts
                            .iter()
                            .map(|(scope, name, m)| Row {
                                scope: scope.clone(),
                                name: name.clone(),
                                src: tui::shorten_home(&m.src),
                                dst: m.dst.clone(),
                                mode: if m.readonly {
                                    "read-only".to_string()
                                } else {
                                    "read-write".to_string()
                                },
                            })
                            .collect();
                        let mut table = Table::new(rows);
                        table.with(Style::modern_rounded());
                        println!("{table}");
                    }
                    Ok(())
                }
            },
            cli::ConfigCommand::Trust { command: trust_cmd } => match trust_cmd {
                cli::TrustCommand::Grant { selector } => {
                    let class = ClassSelector::parse(&selector)?;
                    config.resolve_agent_source(&class)?;
                    if config.trust_agent(&class.key()) {
                        config.save(&paths)?;
                        println!("Trusted {}.", class.key());
                    } else {
                        println!("{} is already trusted.", class.key());
                    }
                    Ok(())
                }
                cli::TrustCommand::Revoke { selector } => {
                    let class = ClassSelector::parse(&selector)?;
                    if AppConfig::is_builtin_agent(&class.key()) {
                        anyhow::bail!("{} is a built-in agent and is always trusted.", class.key());
                    }
                    if config.untrust_agent(&class.key()) {
                        config.save(&paths)?;
                        println!("Revoked trust for {}.", class.key());
                    } else {
                        println!("{} is not currently trusted.", class.key());
                    }
                    Ok(())
                }
                cli::TrustCommand::List => {
                    let agents: Vec<_> = config
                        .agents
                        .iter()
                        .filter(|(_, source)| source.trusted)
                        .map(|(key, _)| key.clone())
                        .collect();
                    if agents.is_empty() {
                        println!("No trusted agents.");
                    } else {
                        for key in agents {
                            println!("{key}");
                        }
                    }
                    Ok(())
                }
            },
            cli::ConfigCommand::Auth { command: auth_cmd } => match auth_cmd {
                cli::AuthCommand::Set { mode, agent } => {
                    let parsed_mode: config::AuthForwardMode =
                        mode.parse().map_err(|e: String| anyhow::anyhow!("{e}"))?;
                    if let Some(agent_selector) = agent {
                        let class = ClassSelector::parse(&agent_selector)?;
                        config.resolve_agent_source(&class)?;
                        config.set_agent_auth_forward(&class.key(), parsed_mode);
                        config.save(&paths)?;
                        println!("Set auth forwarding for {} to {parsed_mode}.", class.key());
                    } else {
                        config.claude.auth_forward = parsed_mode;
                        config.save(&paths)?;
                        println!("Set global auth forwarding to {parsed_mode}.");
                    }
                    Ok(())
                }
                cli::AuthCommand::Show { agent } => {
                    if let Some(agent_selector) = agent {
                        let class = ClassSelector::parse(&agent_selector)?;
                        let effective = config.resolve_auth_forward_mode(&class.key());
                        println!("{effective}");
                    } else {
                        println!("{}", config.claude.auth_forward);
                    }
                    Ok(())
                }
            },
        },
        Command::Workspace { command } => match command {
            WorkspaceCommand::Create {
                name,
                workdir,
                mounts,
                no_workdir_mount,
                allowed_agents,
                default_agent,
            } => {
                let expanded_workdir = workspace::resolve_path(&workdir);
                let mut all_mounts: Vec<_> = mounts
                    .iter()
                    .map(|value| parse_mount_spec_resolved(value))
                    .collect::<Result<Vec<_>>>()?;
                if !no_workdir_mount {
                    let already_mounted = all_mounts.iter().any(|m| m.dst == expanded_workdir);
                    if !already_mounted {
                        all_mounts.insert(
                            0,
                            workspace::MountConfig {
                                src: expanded_workdir.clone(),
                                dst: expanded_workdir.clone(),
                                readonly: false,
                            },
                        );
                    }
                }
                // Pre-collapse under rule C so the create_workspace
                // post-condition sees a clean mount list. Any rule-C error
                // (readonly mismatch, etc.) surfaces here before we try to
                // write.
                let all_indexes: Vec<usize> = (0..all_mounts.len()).collect();
                let plan = workspace::plan_collapse(&all_mounts, &all_indexes)?;
                if !plan.removed.is_empty() {
                    let removed_list: Vec<String> = plan
                        .removed
                        .iter()
                        .map(|r| tui::shorten_home(&r.child.src))
                        .collect();
                    // Parent paths in a single create are all the same set; pick
                    // the first for the summary headline.
                    let parent = tui::shorten_home(&plan.removed[0].covered_by.src);
                    eprintln!(
                        "collapsed {} redundant mount(s) under {parent}: {}",
                        plan.removed.len(),
                        removed_list.join(", ")
                    );
                }
                let final_mounts = plan.kept;
                let mount_count = final_mounts.len();
                config.create_workspace(
                    &name,
                    WorkspaceConfig {
                        workdir: expanded_workdir,
                        mounts: final_mounts,
                        allowed_agents,
                        default_agent,
                        last_agent: None,
                    },
                )?;
                config.save(&paths)?;
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
                        #[tabled(rename = "Allowed Agents")]
                        allowed: String,
                        #[tabled(rename = "Default Agent")]
                        default_agent: String,
                    }
                    let rows: Vec<Row> = workspaces
                        .iter()
                        .map(|(name, ws)| Row {
                            name: (*name).to_string(),
                            workdir: tui::shorten_home(&ws.workdir),
                            mounts: ws.mounts.len(),
                            allowed: if ws.allowed_agents.is_empty() {
                                "any agent".to_string()
                            } else {
                                ws.allowed_agents.join(", ")
                            },
                            default_agent: ws
                                .default_agent
                                .as_deref()
                                .unwrap_or("none")
                                .to_string(),
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
                use tabled::settings::Style;
                use tabled::{Table, Tabled};

                #[derive(Tabled)]
                struct MountRow {
                    #[tabled(rename = "Source")]
                    src: String,
                    #[tabled(rename = "Destination")]
                    dst: String,
                    #[tabled(rename = "Mode")]
                    mode: String,
                }

                let workspace = config
                    .workspaces
                    .get(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?;

                let allowed = if workspace.allowed_agents.is_empty() {
                    "any agent".to_string()
                } else {
                    workspace.allowed_agents.join(", ")
                };
                let default_agent = workspace.default_agent.as_deref().unwrap_or("none");

                let short_workdir = tui::shorten_home(&workspace.workdir);
                let info = [
                    ("Name", name.as_str()),
                    ("Workdir", short_workdir.as_str()),
                    ("Allowed Agents", &allowed),
                    ("Default Agent", default_agent),
                ];
                let mut info_table = Table::builder(info.iter().map(|(k, v)| [*k, *v])).build();
                info_table
                    .with(Style::modern_rounded())
                    .with(tabled::settings::Remove::row(
                        tabled::settings::object::Rows::first(),
                    ));
                println!("{info_table}");

                if !workspace.mounts.is_empty() {
                    println!();
                    println!("Mounts:");
                    let mount_rows: Vec<MountRow> = workspace
                        .mounts
                        .iter()
                        .map(|m| MountRow {
                            src: tui::shorten_home(&m.src),
                            dst: tui::shorten_home(&m.dst),
                            mode: if m.readonly {
                                "read-only".to_string()
                            } else {
                                "read-write".to_string()
                            },
                        })
                        .collect();
                    let mut mount_table = Table::new(mount_rows);
                    mount_table.with(Style::modern_rounded());
                    println!("{mount_table}");
                }

                Ok(())
            }
            WorkspaceCommand::Edit {
                name,
                workdir,
                mounts,
                remove_destinations,
                no_workdir_mount,
                allowed_agents,
                remove_allowed_agents,
                default_agent,
                clear_default_agent,
                assume_yes,
                prune,
            } => {
                let upsert_mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec_resolved(value))
                    .collect::<Result<Vec<_>>>()?;

                // Build the "post-upsert list" the same way edit_workspace will
                // apply upserts: start from existing mounts (after applying
                // remove_destinations), then merge each upsert by dst.
                let current_ws = config
                    .workspaces
                    .get(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?
                    .clone();

                let mut post_upsert: Vec<workspace::MountConfig> = current_ws
                    .mounts
                    .iter()
                    .filter(|m| !remove_destinations.iter().any(|d| d == &m.dst))
                    .cloned()
                    .collect();
                if no_workdir_mount {
                    let workdir = &current_ws.workdir;
                    post_upsert.retain(|m| !(m.src == *workdir && m.dst == *workdir));
                }
                let mut new_indexes: Vec<usize> = Vec::new();
                for upsert in &upsert_mounts {
                    if let Some(pos) = post_upsert.iter().position(|m| m.dst == upsert.dst) {
                        post_upsert[pos] = upsert.clone();
                        new_indexes.push(pos);
                    } else {
                        post_upsert.push(upsert.clone());
                        new_indexes.push(post_upsert.len() - 1);
                    }
                }

                // Plan the collapse.
                let plan = workspace::plan_collapse(&post_upsert, &new_indexes)?;

                // Partition removals by origin.
                let (edit_driven, pre_existing): (Vec<_>, Vec<_>) =
                    plan.removed.iter().partition(|r| {
                        let child_idx = post_upsert
                            .iter()
                            .position(|m| m == &r.child)
                            .expect("child must appear in post_upsert list");
                        let parent_idx = post_upsert
                            .iter()
                            .position(|m| m == &r.covered_by)
                            .expect("parent must appear in post_upsert list");
                        new_indexes.contains(&child_idx) || new_indexes.contains(&parent_idx)
                    });

                // Reject pre-existing violations unless --prune.
                if !pre_existing.is_empty() && !prune {
                    let details: Vec<String> = pre_existing
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

                // If there are any collapses to apply, prompt (or bail on
                // non-TTY without --yes).
                if !plan.removed.is_empty() && !assume_yes {
                    use std::io::IsTerminal;
                    if !std::io::stdin().is_terminal() {
                        anyhow::bail!(
                            "refusing to collapse mounts without confirmation; pass --yes to proceed non-interactively"
                        );
                    }

                    if !edit_driven.is_empty() {
                        eprintln!(
                            "Adding mount(s) will subsume {} existing mount(s):",
                            edit_driven.len()
                        );
                        for r in &edit_driven {
                            eprintln!("  • {}", tui::shorten_home(&r.child.src));
                        }
                    }
                    if !pre_existing.is_empty() {
                        eprintln!(
                            "Cleaning up {} pre-existing redundant mount(s):",
                            pre_existing.len()
                        );
                        for r in &pre_existing {
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

                // Translate collapses into extra remove_destinations so
                // edit_workspace's existing remove + upsert logic produces the
                // clean set.
                let mut effective_removes = remove_destinations.clone();
                for r in &plan.removed {
                    if !effective_removes.contains(&r.child.dst) {
                        effective_removes.push(r.child.dst.clone());
                    }
                }

                // Collect what changed for the summary (preserves the existing
                // summary output, plus collapse lines).
                let mut changes: Vec<String> = Vec::new();
                if let Some(ref w) = workdir {
                    changes.push(format!("workdir → {}", tui::shorten_home(w)));
                }
                for m in &upsert_mounts {
                    if plan.removed.iter().any(|r| r.child.dst == m.dst) {
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
                for r in &plan.removed {
                    changes.push(format!(
                        "collapsed {} under {}",
                        tui::shorten_home(&r.child.src),
                        tui::shorten_home(&r.covered_by.src)
                    ));
                }
                if no_workdir_mount {
                    changes.push("removed workdir auto-mount".to_string());
                }
                for agent in &allowed_agents {
                    changes.push(format!("allowed agent {agent}"));
                }
                for agent in &remove_allowed_agents {
                    changes.push(format!("removed agent {agent}"));
                }
                if clear_default_agent {
                    changes.push("cleared default agent".to_string());
                } else if let Some(ref agent) = default_agent {
                    changes.push(format!("default agent → {agent}"));
                }

                config.edit_workspace(
                    &name,
                    WorkspaceEdit {
                        workdir: workdir.map(|w| resolve_path(&w)),
                        upsert_mounts,
                        remove_destinations: effective_removes,
                        no_workdir_mount,
                        allowed_agents_to_add: allowed_agents,
                        allowed_agents_to_remove: remove_allowed_agents,
                        default_agent: if clear_default_agent {
                            Some(None)
                        } else {
                            default_agent.map(Some)
                        },
                    },
                )?;
                config.save(&paths)?;
                println!("Updated workspace {name:?}:");
                for change in &changes {
                    println!("  - {change}");
                }
                Ok(())
            }
            WorkspaceCommand::Remove { name } => {
                config.remove_workspace(&name)?;
                config.save(&paths)?;
                println!("Removed workspace {name:?}.");
                Ok(())
            }
        },
        Command::Purge { selector, all } => match Selector::parse(&selector)? {
            Selector::Container(container) => {
                remove_data_dir_if_exists(&paths.data_dir.join(&container))?;
                println!("Purged state for {container}.");
                Ok(())
            }
            Selector::Class(class) => {
                if all {
                    runtime::purge_class_data(&paths, &class)?;
                    println!("Purged all state for {}.", class.key());
                } else {
                    let container = crate::instance::primary_container_name(&class);
                    remove_data_dir_if_exists(&paths.data_dir.join(&container))?;
                    println!("Purged state for {container}.");
                }
                Ok(())
            }
        },
    }
}

fn remove_data_dir_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_target_tilde_path() {
        let result = classify_target("~/Projects/my-app");
        assert!(matches!(
            result,
            TargetKind::Path { ref src, .. } if src == "~/Projects/my-app"
        ));
    }

    #[test]
    fn classify_target_tilde_path_with_dst() {
        let result = classify_target("~/Projects/my-app:/app");
        assert!(matches!(
            result,
            TargetKind::Path { ref src, ref dst } if src == "~/Projects/my-app" && dst == "/app"
        ));
    }

    #[test]
    fn classify_target_dot_relative_path() {
        let result = classify_target("./my-app");
        assert!(matches!(result, TargetKind::Path { .. }));
    }

    #[test]
    fn classify_target_absolute_path() {
        let result = classify_target("/tmp/my-app");
        assert!(matches!(
            result,
            TargetKind::Path { ref src, ref dst } if src == "/tmp/my-app" && dst == "/tmp/my-app"
        ));
    }

    #[test]
    fn classify_target_absolute_path_with_dst() {
        let result = classify_target("/tmp/my-app:/workspace");
        assert!(matches!(
            result,
            TargetKind::Path { ref src, ref dst } if src == "/tmp/my-app" && dst == "/workspace"
        ));
    }

    #[test]
    fn classify_target_plain_name() {
        let result = classify_target("big-monorepo");
        assert!(matches!(
            result,
            TargetKind::Name(ref name) if name == "big-monorepo"
        ));
    }

    #[test]
    fn classify_target_name_with_no_slash() {
        let result = classify_target("my-workspace");
        assert!(matches!(result, TargetKind::Name(_)));
    }

    #[test]
    fn classify_target_relative_with_slash() {
        // Contains `/` so treated as path
        let result = classify_target("sub/dir");
        assert!(matches!(result, TargetKind::Path { .. }));
    }

    #[test]
    fn resolve_target_name_workspace_only() {
        let mut config = config::AppConfig::default();
        config.workspaces.insert(
            "my-ws".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
            },
        );
        let cwd = std::env::temp_dir();
        let result = resolve_target_name("my-ws", &config, &cwd).unwrap();
        assert!(matches!(result, LoadWorkspaceInput::Saved(ref name) if name == "my-ws"));
    }

    #[test]
    fn resolve_target_name_directory_only() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("my-dir");
        std::fs::create_dir_all(&dir).unwrap();

        let config = config::AppConfig::default();
        let result = resolve_target_name("my-dir", &config, temp.path()).unwrap();
        assert!(matches!(result, LoadWorkspaceInput::Path { .. }));
    }

    #[test]
    fn resolve_target_name_neither_errors() {
        let config = config::AppConfig::default();
        let cwd = std::env::temp_dir();
        let result = resolve_target_name("nonexistent-thing", &config, &cwd);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("neither a saved workspace nor a directory"));
    }

    #[test]
    fn resolve_agent_from_context_matches_workspace_from_nested_mount_path() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let nested_dir = project_dir.join("src/bin");
        std::fs::create_dir_all(&nested_dir).unwrap();

        let mut config = config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
                last_agent: None,
            },
        );

        let resolved = resolve_agent_from_context(&config, &nested_dir).unwrap();

        assert_eq!(resolved.0.key(), "agent-smith");
        assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_string()));
    }

    #[test]
    fn resolve_agent_from_context_matches_workspace_from_host_workdir_root() {
        let temp = tempfile::tempdir().unwrap();
        let workspace_root = temp.path().join("monorepo");
        let repo_dir = workspace_root.join("jackin");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let workspace_root = workspace_root.canonicalize().unwrap();

        let mut config = config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: workspace_root.display().to_string(),
                mounts: vec![workspace::MountConfig {
                    src: repo_dir.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/jackin".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
                last_agent: None,
            },
        );

        let resolved = resolve_agent_from_context(&config, &workspace_root).unwrap();

        assert_eq!(resolved.0.key(), "agent-smith");
        assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_string()));
    }

    #[test]
    fn resolve_agent_from_context_ignores_stale_last_agent() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let nested_dir = project_dir.join("src/bin");
        std::fs::create_dir_all(&nested_dir).unwrap();

        let mut config = config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: None,
                last_agent: Some("ghost-agent".to_string()),
            },
        );

        let resolved = resolve_agent_from_context(&config, &nested_dir).unwrap();

        assert_eq!(resolved.0.key(), "agent-smith");
        assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_string()));
    }

    /// Build an `AppConfig` pre-populated with an `agent-smith` agent and a
    /// single workspace rooted at `project_dir`.
    fn config_with_workspace(
        project_dir: &Path,
        allowed_agents: Vec<String>,
        last_agent: Option<String>,
    ) -> config::AppConfig {
        let mut config = config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
            },
        );
        config.agents.insert(
            "the-architect".to_string(),
            config::AgentSource {
                git: "https://github.com/jackin-project/jackin-the-architect.git".to_string(),
                trusted: true,
                claude: None,
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                }],
                allowed_agents,
                default_agent: None,
                last_agent,
            },
        );
        config
    }

    /// `list_running_agent_names` issues two docker captures (role filter +
    /// legacy filter); supply running-agent output on the first, nothing on
    /// the second.
    fn fake_runner_with_running_agents(names: &[&str]) -> runtime::FakeRunner {
        let mut runner = runtime::FakeRunner::default();
        runner.capture_queue.push_back(names.join("\n"));
        runner.capture_queue.push_back(String::new());
        runner
    }

    #[test]
    fn resolve_running_container_from_context_picks_lone_running_agent() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let nested_dir = project_dir.join("src");
        std::fs::create_dir_all(&nested_dir).unwrap();

        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        let mut runner = fake_runner_with_running_agents(&["jackin-agent-smith"]);

        let container =
            resolve_running_container_from_context(&config, &nested_dir, &mut runner).unwrap();

        assert_eq!(container, "jackin-agent-smith");
    }

    #[test]
    fn resolve_running_container_from_context_prefers_last_agent() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let config = config_with_workspace(
            &project_dir,
            vec!["agent-smith".to_string(), "the-architect".to_string()],
            Some("the-architect".to_string()),
        );
        let mut runner =
            fake_runner_with_running_agents(&["jackin-agent-smith", "jackin-the-architect"]);

        let container =
            resolve_running_container_from_context(&config, &project_dir, &mut runner).unwrap();

        assert_eq!(container, "jackin-the-architect");
    }

    #[test]
    fn resolve_running_container_from_context_errors_when_nothing_running() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        let mut runner = fake_runner_with_running_agents(&[]);

        let err = resolve_running_container_from_context(&config, &project_dir, &mut runner)
            .unwrap_err()
            .to_string();

        assert!(err.contains("no running agents"), "got: {err}");
        assert!(err.contains("my-app"), "got: {err}");
    }

    #[test]
    fn resolve_running_container_from_context_ignores_disallowed_running_agents() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        // the-architect is running but not allowed in this workspace.
        let mut runner = fake_runner_with_running_agents(&["jackin-the-architect"]);

        let err = resolve_running_container_from_context(&config, &project_dir, &mut runner)
            .unwrap_err()
            .to_string();

        assert!(err.contains("no running agents"), "got: {err}");
    }

    #[test]
    fn resolve_running_container_from_context_errors_when_no_workspace_matches() {
        let temp = tempfile::tempdir().unwrap();
        let unrelated = temp.path().join("unrelated");
        std::fs::create_dir_all(&unrelated).unwrap();

        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();
        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        let mut runner = fake_runner_with_running_agents(&["jackin-agent-smith"]);

        let err = resolve_running_container_from_context(&config, &unrelated, &mut runner)
            .unwrap_err()
            .to_string();

        assert!(err.contains("no saved workspace matches"), "got: {err}");
    }

    #[test]
    fn remember_last_agent_persists_successful_loads() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let mut config = config::AppConfig::default();
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: temp.path().display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
            },
        );

        remember_last_agent(
            &paths,
            &mut config,
            Some("my-app"),
            &ClassSelector::new(None, "agent-smith"),
            &Ok(()),
        );

        assert_eq!(
            config
                .workspaces
                .get("my-app")
                .and_then(|workspace| workspace.last_agent.as_deref()),
            Some("agent-smith")
        );
    }

    #[test]
    fn remember_last_agent_skips_failed_loads() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let mut config = config::AppConfig::default();
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: temp.path().display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
            },
        );

        remember_last_agent(
            &paths,
            &mut config,
            Some("my-app"),
            &ClassSelector::new(None, "agent-smith"),
            &Err(anyhow::anyhow!("load failed")),
        );

        assert_eq!(
            config
                .workspaces
                .get("my-app")
                .and_then(|workspace| workspace.last_agent.as_deref()),
            None
        );
    }

    /// Simulates `jackin workspace create jackin --workdir jackin --mount sibling-project`
    /// from a parent directory. Both relative workdir and mount must be resolved to
    /// absolute paths so that `create_workspace` validation passes.
    #[test]
    fn workspace_create_resolves_relative_workdir_and_mounts() {
        let temp = tempfile::tempdir().unwrap();
        let workdir_dir = temp.path().join("jackin");
        let mount_dir = temp.path().join("sibling-project");
        std::fs::create_dir_all(&workdir_dir).unwrap();
        std::fs::create_dir_all(&mount_dir).unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let expanded_workdir = workspace::resolve_path("jackin");
        let mount = parse_mount_spec_resolved("sibling-project").unwrap();

        let mut config = config::AppConfig::default();
        let result = config.create_workspace(
            "jackin",
            WorkspaceConfig {
                workdir: expanded_workdir.clone(),
                mounts: vec![
                    workspace::MountConfig {
                        src: expanded_workdir.clone(),
                        dst: expanded_workdir.clone(),
                        readonly: false,
                    },
                    mount,
                ],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
            },
        );

        std::env::set_current_dir(original_cwd).unwrap();

        result.unwrap();
        let ws = config.workspaces.get("jackin").unwrap();
        assert!(ws.workdir.starts_with('/'));
        assert!(!ws.workdir.contains(".."));
        assert!(ws.mounts.iter().all(|m| m.src.starts_with('/')));
    }

    /// Simulates `jackin workspace create jackin --workdir . --mount ../jackin-agent-smith`
    /// from inside the project directory. Dot-workdir and parent-relative mount must both
    /// resolve to clean absolute paths.
    #[test]
    fn workspace_create_resolves_dot_workdir_and_dotdot_mount() {
        let temp = tempfile::tempdir().unwrap();
        let workdir_dir = temp.path().join("jackin");
        let sibling_dir = temp.path().join("jackin-agent-smith");
        std::fs::create_dir_all(&workdir_dir).unwrap();
        std::fs::create_dir_all(&sibling_dir).unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&workdir_dir).unwrap();

        let expanded_workdir = workspace::resolve_path(".");
        let mount = parse_mount_spec_resolved("../jackin-agent-smith").unwrap();

        let mut config = config::AppConfig::default();
        let result = config.create_workspace(
            "jackin",
            WorkspaceConfig {
                workdir: expanded_workdir.clone(),
                mounts: vec![
                    workspace::MountConfig {
                        src: expanded_workdir.clone(),
                        dst: expanded_workdir.clone(),
                        readonly: false,
                    },
                    mount.clone(),
                ],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
            },
        );

        std::env::set_current_dir(original_cwd).unwrap();

        result.unwrap();
        let ws = config.workspaces.get("jackin").unwrap();
        assert!(ws.workdir.starts_with('/'));
        assert!(!ws.workdir.contains(".."));
        assert!(!mount.src.contains(".."), "mount src must not contain '..'");
        assert!(mount.src.ends_with("/jackin-agent-smith"));
    }

    /// Simulates `jackin workspace create my-app --workdir /tmp/app` (default behavior).
    /// The workdir must be auto-mounted as the first mount.
    #[test]
    fn workspace_create_auto_mounts_workdir_by_default() {
        let temp = tempfile::tempdir().unwrap();
        let workdir_dir = temp.path().join("my-app");
        std::fs::create_dir_all(&workdir_dir).unwrap();

        let expanded_workdir = workdir_dir.display().to_string();

        // Simulate default behavior: no_workdir_mount = false
        let no_workdir_mount = false;
        let mut all_mounts: Vec<workspace::MountConfig> = vec![];
        if !no_workdir_mount {
            let already_mounted = all_mounts.iter().any(|m| m.dst == expanded_workdir);
            if !already_mounted {
                all_mounts.insert(
                    0,
                    workspace::MountConfig {
                        src: expanded_workdir.clone(),
                        dst: expanded_workdir.clone(),
                        readonly: false,
                    },
                );
            }
        }

        let mut config = config::AppConfig::default();
        config
            .create_workspace(
                "my-app",
                WorkspaceConfig {
                    workdir: expanded_workdir.clone(),
                    mounts: all_mounts,
                    allowed_agents: vec![],
                    default_agent: None,
                    last_agent: None,
                },
            )
            .unwrap();

        let ws = config.workspaces.get("my-app").unwrap();
        assert_eq!(ws.mounts.len(), 1);
        assert_eq!(ws.mounts[0].src, expanded_workdir);
        assert_eq!(ws.mounts[0].dst, expanded_workdir);
        assert!(!ws.mounts[0].readonly);
    }

    /// Simulates `jackin workspace create monorepo --workdir /workspace --no-workdir-mount
    /// --mount /tmp/src:/workspace`. The workdir must NOT be auto-mounted.
    #[test]
    fn workspace_create_no_workdir_mount_skips_auto_mount() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let src_path = src_dir.display().to_string();

        // Simulate --no-workdir-mount with explicit mount
        let no_workdir_mount = true;
        let mut all_mounts = vec![workspace::MountConfig {
            src: src_path.clone(),
            dst: "/workspace".to_string(),
            readonly: false,
        }];
        if !no_workdir_mount {
            // This block should NOT execute
            all_mounts.insert(
                0,
                workspace::MountConfig {
                    src: "/workspace".to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                },
            );
        }

        let mut config = config::AppConfig::default();
        config
            .create_workspace(
                "monorepo",
                WorkspaceConfig {
                    workdir: "/workspace".to_string(),
                    mounts: all_mounts,
                    allowed_agents: vec![],
                    default_agent: None,
                    last_agent: None,
                },
            )
            .unwrap();

        let ws = config.workspaces.get("monorepo").unwrap();
        assert_eq!(ws.mounts.len(), 1, "should only have the explicit mount");
        assert_eq!(ws.mounts[0].src, src_path);
        assert_eq!(ws.mounts[0].dst, "/workspace");
    }

    /// When the workdir is already covered by an explicit --mount, the auto-mount
    /// should be skipped even without --no-workdir-mount.
    #[test]
    fn workspace_create_skips_auto_mount_when_workdir_already_mounted() {
        let temp = tempfile::tempdir().unwrap();
        let workdir_dir = temp.path().join("project");
        std::fs::create_dir_all(&workdir_dir).unwrap();

        let expanded_workdir = workdir_dir.display().to_string();

        // Simulate: user explicitly mounts workdir via --mount
        let no_workdir_mount = false;
        let mut all_mounts = vec![workspace::MountConfig {
            src: expanded_workdir.clone(),
            dst: expanded_workdir.clone(),
            readonly: true, // user chose read-only
        }];
        if !no_workdir_mount {
            let already_mounted = all_mounts.iter().any(|m| m.dst == expanded_workdir);
            if !already_mounted {
                all_mounts.insert(
                    0,
                    workspace::MountConfig {
                        src: expanded_workdir.clone(),
                        dst: expanded_workdir.clone(),
                        readonly: false,
                    },
                );
            }
        }

        let mut config = config::AppConfig::default();
        config
            .create_workspace(
                "project",
                WorkspaceConfig {
                    workdir: expanded_workdir.clone(),
                    mounts: all_mounts,
                    allowed_agents: vec![],
                    default_agent: None,
                    last_agent: None,
                },
            )
            .unwrap();

        let ws = config.workspaces.get("project").unwrap();
        assert_eq!(ws.mounts.len(), 1, "no duplicate mount should be added");
        assert!(ws.mounts[0].readonly, "original mount properties preserved");
    }

    /// Simulates `jackin workspace edit jackin --mount sibling-dev` where the mount
    /// is a relative directory name. The resolved mount must pass validation.
    #[test]
    fn workspace_edit_resolves_relative_mount() {
        let temp = tempfile::tempdir().unwrap();
        let workdir_dir = temp.path().join("jackin");
        let dev_dir = temp.path().join("jackin-dev");
        std::fs::create_dir_all(&workdir_dir).unwrap();
        std::fs::create_dir_all(&dev_dir).unwrap();

        let workdir_abs = workdir_dir.display().to_string();

        let mut config = config::AppConfig::default();
        config
            .create_workspace(
                "jackin",
                WorkspaceConfig {
                    workdir: workdir_abs.clone(),
                    mounts: vec![workspace::MountConfig {
                        src: workdir_abs.clone(),
                        dst: workdir_abs.clone(),
                        readonly: false,
                    }],
                    allowed_agents: vec![],
                    default_agent: None,
                    last_agent: None,
                },
            )
            .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let mount = parse_mount_spec_resolved("jackin-dev").unwrap();

        let result = config.edit_workspace(
            "jackin",
            WorkspaceEdit {
                upsert_mounts: vec![mount.clone()],
                ..WorkspaceEdit::default()
            },
        );

        std::env::set_current_dir(original_cwd).unwrap();

        result.unwrap();
        let ws = config.workspaces.get("jackin").unwrap();
        assert_eq!(ws.mounts.len(), 2);
        assert!(ws.mounts[1].src.starts_with('/'));
        assert!(ws.mounts[1].src.ends_with("/jackin-dev"));
    }

    /// Simulates `jackin workspace edit my-app --no-workdir-mount` to remove the
    /// auto-mounted workdir after workspace creation.
    #[test]
    fn workspace_edit_no_workdir_mount_removes_auto_mount() {
        let temp = tempfile::tempdir().unwrap();
        let workdir_dir = temp.path().join("my-app");
        let extra_dir = temp.path().join("extra");
        std::fs::create_dir_all(&workdir_dir).unwrap();
        std::fs::create_dir_all(&extra_dir).unwrap();

        let workdir_abs = workdir_dir.display().to_string();
        let extra_abs = extra_dir.display().to_string();

        let mut config = config::AppConfig::default();
        // Create workspace with auto-mounted workdir + an extra mount
        config
            .create_workspace(
                "my-app",
                WorkspaceConfig {
                    workdir: workdir_abs.clone(),
                    mounts: vec![
                        workspace::MountConfig {
                            src: workdir_abs.clone(),
                            dst: workdir_abs.clone(),
                            readonly: false,
                        },
                        workspace::MountConfig {
                            src: extra_abs.clone(),
                            dst: workdir_abs.clone() + "/extra",
                            readonly: false,
                        },
                    ],
                    allowed_agents: vec![],
                    default_agent: None,
                    last_agent: None,
                },
            )
            .unwrap();

        assert_eq!(config.workspaces.get("my-app").unwrap().mounts.len(), 2);

        // Now remove the workdir auto-mount
        config
            .edit_workspace(
                "my-app",
                WorkspaceEdit {
                    no_workdir_mount: true,
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap();

        let ws = config.workspaces.get("my-app").unwrap();
        assert_eq!(ws.mounts.len(), 1, "auto-mount should be removed");
        assert_eq!(ws.mounts[0].dst, workdir_abs.clone() + "/extra");
    }

    /// `--no-workdir-mount` on edit should fail if there is no auto-mounted workdir.
    #[test]
    fn workspace_edit_no_workdir_mount_fails_when_no_auto_mount() {
        let temp = tempfile::tempdir().unwrap();
        let src_dir = temp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let src_abs = src_dir.display().to_string();

        let mut config = config::AppConfig::default();
        // Create workspace that was originally made with --no-workdir-mount
        config
            .create_workspace(
                "monorepo",
                WorkspaceConfig {
                    workdir: "/workspace".to_string(),
                    mounts: vec![workspace::MountConfig {
                        src: src_abs,
                        dst: "/workspace".to_string(),
                        readonly: false,
                    }],
                    allowed_agents: vec![],
                    default_agent: None,
                    last_agent: None,
                },
            )
            .unwrap();

        let err = config
            .edit_workspace(
                "monorepo",
                WorkspaceEdit {
                    no_workdir_mount: true,
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(
            err.to_string().contains("no auto-mounted workdir found"),
            "expected clear error, got: {err}"
        );
    }
}
