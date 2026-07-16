// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace subcommand dispatch — extracted from `app::run` to keep mod.rs focused.

use anyhow::Result;

use crate::cli::{self, WorkspaceCommand};
use crate::workspace::{
    self, WorkspaceConfig, WorkspaceEdit, parse_mount_spec_resolved, resolve_path,
};
use jackin_config::AppConfig;
use jackin_core::{JackinPaths, WorkspaceName};
use jackin_docker::ShellRunner;
use jackin_docker::docker_client::BollardDockerClient;

struct WorkspaceCreateParams {
    name: String,
    workdir: String,
    mounts: Vec<String>,
    allowed_roles: Vec<String>,
    default_role: Option<String>,
    default_agent: Option<jackin_core::Agent>,
    mount_isolation: Vec<(String, jackin_core::MountIsolation)>,
    keep_awake: bool,
    git_pull: bool,
}

/// CLI field bundle for `workspace edit` — keeps handler signatures under
/// clippy's argument thresholds while preserving arm behavior.
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors clap flags on WorkspaceCommand::Edit; grouping would obscure CLI parity"
)]
struct WorkspaceEditParams {
    name: String,
    workdir: Option<String>,
    mounts: Vec<String>,
    remove_destinations: Vec<String>,
    no_workdir_mount: bool,
    allowed_roles: Vec<String>,
    remove_allowed_roles: Vec<String>,
    default_role: Option<String>,
    clear_default_role: bool,
    default_agent: Option<jackin_core::Agent>,
    clear_default_agent: bool,
    assume_yes: bool,
    prune: bool,
    mount_isolation: Vec<(String, jackin_core::MountIsolation)>,
    delete_isolated_state: bool,
    keep_awake: bool,
    no_keep_awake: bool,
    git_pull: bool,
    no_git_pull: bool,
}

struct PreparedWorkspaceEdit {
    plan: jackin_config::WorkspaceEditPlan,
    upsert_mounts: Vec<workspace::MountConfig>,
    keep_awake_change: Option<bool>,
    git_pull_change: Option<bool>,
    current_ws: WorkspaceConfig,
}

pub(super) async fn handle(
    command: WorkspaceCommand,
    config: &mut AppConfig,
    paths: &JackinPaths,
    debug: bool,
) -> Result<()> {
    let mut runner = ShellRunner { debug };
    let connect_docker = || BollardDockerClient::connect();

    match command {
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
        } => handle_workspace_create(
            paths,
            WorkspaceCreateParams {
                name,
                workdir,
                mounts,
                allowed_roles,
                default_role,
                default_agent,
                mount_isolation,
                keep_awake,
                git_pull,
            },
        ),
        WorkspaceCommand::List(list_args) => handle_workspace_list(config, list_args),
        WorkspaceCommand::Show(show_args) => handle_workspace_show(config, show_args),
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
            handle_workspace_edit(
                config,
                paths,
                &mut runner,
                connect_docker,
                WorkspaceEditParams {
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
                },
            )
            .await
        }
        WorkspaceCommand::Prune { name, assume_yes } => {
            handle_workspace_prune(config, paths, name, assume_yes)
        }
        WorkspaceCommand::Remove { name } => handle_workspace_remove(paths, name),
        WorkspaceCommand::Env(env_cmd) => handle_workspace_env(config, paths, env_cmd),
        WorkspaceCommand::ClaudeToken(action) => super::handle_claude_token(paths, config, action),
    }
}

fn handle_workspace_create(paths: &JackinPaths, params: WorkspaceCreateParams) -> Result<()> {
    let WorkspaceCreateParams {
        name,
        workdir,
        mounts,
        allowed_roles,
        default_role,
        default_agent,
        mount_isolation,
        keep_awake,
        git_pull,
    } = params;

    let expanded_workdir = resolve_path(&workdir);
    let parsed_mounts = mounts
        .iter()
        .map(|value| parse_mount_spec_resolved(value).map_err(anyhow::Error::from))
        .collect::<Result<Vec<_>>>()?;
    let mut plan = workspace::planner::plan_create(&parsed_mounts)?;
    workspace::planner::apply_isolation_overrides(&mut plan.final_mounts, &mount_isolation)?;
    if !plan.collapsed.is_empty() {
        let removed_list: Vec<String> = plan
            .collapsed
            .iter()
            .map(|r| jackin_core::shorten_home(&r.child.src))
            .collect();
        // Parent paths in a single create are all the same set; pick
        // the first for the summary headline.
        let parent = jackin_core::shorten_home(&plan.collapsed[0].covered_by.src);
        eprintln!(
            "collapsed {} redundant mount(s) under {parent}: {}",
            plan.collapsed.len(),
            removed_list.join(", ")
        );
    }
    let mount_count = plan.final_mounts.len();
    let ws = WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: expanded_workdir,
        mounts: plan.final_mounts,
        allowed_roles,
        default_role,
        default_agent,
        last_role: None,
        env: std::collections::BTreeMap::new(),
        roles: std::collections::BTreeMap::new(),
        keep_awake: workspace::KeepAwakeConfig {
            enabled: keep_awake,
        },
        claude: None,
        codex: None,
        amp: None,
        kimi: None,
        opencode: None,
        grok: None,
        github: None,
        git_pull_on_entry: git_pull,
        runtime: jackin_config::WorkspaceRuntimeConfig::default(),
        dirty_exit_policy: None,
        docker: None,
    };
    let mut editor = jackin_config::ConfigEditor::open(paths)?;
    editor.create_workspace(
        &WorkspaceName::parse(&name).map_err(anyhow::Error::from)?,
        ws,
    )?;
    editor.save()?;
    println!(
        "Created workspace {name:?} (workdir: {}, {mount_count} mount(s)).",
        jackin_core::shorten_home(&workdir)
    );
    Ok(())
}

fn handle_workspace_list(config: &AppConfig, list_args: cli::WorkspaceFormatArgs) -> Result<()> {
    let workspaces = config.list_workspaces();
    let json_format = list_args.format == "json";

    if json_format {
        let data: Vec<serde_json::Value> = workspaces
            .iter()
            .map(|(name, ws)| {
                serde_json::json!({
                    "name": name,
                    "workdir": ws.workdir,
                    "mounts": ws.mounts.len(),
                    "allowed_roles": ws.allowed_roles,
                    "default_role": ws.default_role,
                    "default_agent": ws.resolved_agent().slug(),
                })
            })
            .collect();
        let envelope = serde_json::json!({
            "schema_version": "v1",
            "data": data,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
    } else if workspaces.is_empty() {
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
                name: (*name).to_owned(),
                workdir: jackin_core::shorten_home(&ws.workdir),
                mounts: ws.mounts.len(),
                allowed: if ws.allowed_roles.is_empty() {
                    "any role".to_owned()
                } else {
                    ws.allowed_roles.join(", ")
                },
                default_role: ws.default_role.as_deref().unwrap_or("none").to_owned(),
                agent: ws.resolved_agent().slug().to_owned(),
            })
            .collect();
        let mut table = Table::new(rows);
        table.with(Style::modern());
        println!("{table}");
        println!();
        jackin_launch::output::hint("Run ", "jackin workspace show <name>", " for details.");
    }
    Ok(())
}

fn handle_workspace_show(config: &AppConfig, show_args: cli::WorkspaceShowArgs) -> Result<()> {
    let name = &show_args.name;
    let workspace =
        config.require_workspace(&WorkspaceName::parse(name).map_err(anyhow::Error::from)?)?;
    if cli::format::OutputFormat::parse(&show_args.fmt.format) == cli::format::OutputFormat::Json {
        let mounts: Vec<serde_json::Value> = workspace
            .mounts
            .iter()
            .map(|m| {
                serde_json::json!({
                    "src": m.src,
                    "dst": m.dst,
                    "readonly": m.readonly,
                    "isolation": m.isolation.to_string(),
                })
            })
            .collect();
        let envelope = serde_json::json!({
            "schema_version": "v1",
            "data": {
                "name": name,
                "workdir": workspace.workdir,
                "mounts": mounts,
                "allowed_roles": workspace.allowed_roles,
                "default_role": workspace.default_role,
                "default_agent": workspace.resolved_agent().slug(),
            }
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
    } else {
        print!("{}", super::render_workspace_show(config, name, workspace));
    }
    Ok(())
}

fn prepare_workspace_edit(
    config: &AppConfig,
    params: &WorkspaceEditParams,
) -> Result<PreparedWorkspaceEdit> {
    let name = params.name.as_str();
    // Map paired flags to Option<bool>: None = no change.
    // Mutual exclusion is enforced at parse time by clap's
    // `conflicts_with`, so at most one of the two is true.
    let git_pull_change = if params.git_pull {
        Some(true)
    } else if params.no_git_pull {
        Some(false)
    } else {
        None
    };
    let keep_awake_change = if params.keep_awake {
        Some(true)
    } else if params.no_keep_awake {
        Some(false)
    } else {
        None
    };
    let upsert_mounts = params
        .mounts
        .iter()
        .map(|value| parse_mount_spec_resolved(value).map_err(anyhow::Error::from))
        .collect::<Result<Vec<_>>>()?;

    let current_ws = config
        .require_workspace(&WorkspaceName::parse(name).map_err(anyhow::Error::from)?)?
        .clone();

    let plan = workspace::planner::plan_edit(
        &current_ws,
        &upsert_mounts,
        &params.remove_destinations,
        params.no_workdir_mount,
    )?;

    // Reject pre-existing violations unless --prune.
    if !plan.pre_existing_collapses.is_empty() && !params.prune {
        let details: Vec<String> = plan
            .pre_existing_collapses
            .iter()
            .map(|r| {
                format!(
                    "{} covered by {}",
                    jackin_core::shorten_home(&r.child.src),
                    jackin_core::shorten_home(&r.covered_by.src),
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
    if !all_collapses.is_empty() && !params.assume_yes {
        crate::prompt::require_interactive_stdin(
            "refusing to collapse mounts without confirmation; pass --yes to proceed non-interactively",
        )?;

        if !plan.edit_driven_collapses.is_empty() {
            eprintln!(
                "Adding mount(s) will subsume {} existing mount(s):",
                plan.edit_driven_collapses.len()
            );
            for r in &plan.edit_driven_collapses {
                eprintln!("  • {}", jackin_core::shorten_home(&r.child.src));
            }
        }
        if !plan.pre_existing_collapses.is_empty() {
            eprintln!(
                "Cleaning up {} pre-existing redundant mount(s):",
                plan.pre_existing_collapses.len()
            );
            for r in &plan.pre_existing_collapses {
                eprintln!("  • {}", jackin_core::shorten_home(&r.child.src));
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

    Ok(PreparedWorkspaceEdit {
        plan,
        upsert_mounts,
        keep_awake_change,
        git_pull_change,
        current_ws,
    })
}

fn summarize_workspace_edit(
    prepared: &PreparedWorkspaceEdit,
    params: &WorkspaceEditParams,
) -> Vec<String> {
    let all_collapses: Vec<&workspace::Removal> = prepared
        .plan
        .edit_driven_collapses
        .iter()
        .chain(prepared.plan.pre_existing_collapses.iter())
        .collect();

    // Collect what changed for the summary (preserves the existing
    // summary output, plus collapse lines).
    let mut changes: Vec<String> = Vec::new();
    if let Some(w) = &params.workdir {
        changes.push(format!("workdir → {}", jackin_core::shorten_home(w)));
    }
    for m in &prepared.upsert_mounts {
        if all_collapses.iter().any(|r| r.child.dst == m.dst) {
            continue;
        }
        if m.src == m.dst {
            changes.push(format!("added mount {}", jackin_core::shorten_home(&m.src)));
        } else {
            changes.push(format!(
                "added mount {} → {}",
                jackin_core::shorten_home(&m.src),
                jackin_core::shorten_home(&m.dst)
            ));
        }
    }
    for dst in &params.remove_destinations {
        changes.push(format!("removed mount {}", jackin_core::shorten_home(dst)));
    }
    for r in &all_collapses {
        changes.push(format!(
            "collapsed {} under {}",
            jackin_core::shorten_home(&r.child.src),
            jackin_core::shorten_home(&r.covered_by.src)
        ));
    }
    if params.no_workdir_mount {
        changes.push("removed workdir auto-mount".to_owned());
    }
    for role in &params.allowed_roles {
        changes.push(format!("allowed role {role}"));
    }
    for role in &params.remove_allowed_roles {
        changes.push(format!("removed role {role}"));
    }
    if params.clear_default_role {
        changes.push("cleared default role".to_owned());
    } else if let Some(role) = &params.default_role {
        changes.push(format!("default role → {role}"));
    }
    if params.clear_default_agent {
        changes.push("cleared default agent".to_owned());
    } else if let Some(agent) = params.default_agent {
        changes.push(format!("default agent → {}", agent.slug()));
    }
    if let Some(v) = prepared.keep_awake_change {
        changes.push(format!(
            "keep-awake → {}",
            if v { "enabled" } else { "disabled" }
        ));
    }
    if let Some(v) = prepared.git_pull_change {
        changes.push(format!(
            "git-pull-on-entry → {}",
            if v { "enabled" } else { "disabled" }
        ));
    }
    changes
}

async fn apply_workspace_edit(
    paths: &JackinPaths,
    runner: &mut ShellRunner,
    connect_docker: impl Fn() -> Result<BollardDockerClient>,
    params: WorkspaceEditParams,
    prepared: PreparedWorkspaceEdit,
    changes: &[String],
) -> Result<()> {
    let name = params.name.as_str();
    let PreparedWorkspaceEdit {
        plan,
        upsert_mounts,
        keep_awake_change,
        git_pull_change,
        current_ws,
    } = prepared;

    // Build the prospective mount list (mirrors edit_workspace's
    // merge order) so we can check for source drift on any mount
    // that has preserved isolated state on disk.
    let mut prospective_mounts: Vec<workspace::MountConfig> = current_ws
        .mounts
        .iter()
        .filter(|m| !plan.effective_removals.iter().any(|d| d == &m.dst))
        .cloned()
        .collect();
    if params.no_workdir_mount {
        let workdir_path = &current_ws.workdir;
        prospective_mounts.retain(|m| !(m.src == *workdir_path && m.dst == *workdir_path));
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
    // Drift detection only needs Docker when isolation records
    // exist. Connecting first ensures the daemon is reachable
    // before we query it; skip the connection entirely when there
    // is nothing to check (common in fresh or test environments).
    let wn = WorkspaceName::parse(name).map_err(anyhow::Error::from)?;
    let has_records =
        jackin_runtime::isolation::state::list_records_for_workspace(&paths.data_dir, &wn)
            .is_ok_and(|r| !r.is_empty());
    let detection = if has_records {
        let docker = connect_docker()?;
        jackin_runtime::runtime::drift::detect_workspace_edit_drift(
            paths,
            &wn,
            &prospective_mounts,
            &docker,
        )
        .await?
    } else {
        jackin_runtime::runtime::drift::DriftDetection::default()
    };
    if !detection.running_containers.is_empty() {
        anyhow::bail!(
            "cannot edit workspace `{name}` while these containers are running with isolated state: {}; eject them first",
            detection.running_containers.join(", ")
        );
    }
    if !detection.stopped_records.is_empty() {
        if !params.delete_isolated_state {
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
            jackin_runtime::isolation::cleanup::force_cleanup_isolated(rec, &container_dir, runner)
                .await?;
        }
    }

    let mut editor = jackin_config::ConfigEditor::open(paths)?;
    editor.edit_workspace(
        &WorkspaceName::parse(name).map_err(anyhow::Error::from)?,
        WorkspaceEdit {
            workdir: params.workdir.map(|w| resolve_path(&w)),
            upsert_mounts,
            remove_destinations: plan.effective_removals,
            no_workdir_mount: params.no_workdir_mount,
            allowed_roles_to_add: params.allowed_roles,
            allowed_roles_to_remove: params.remove_allowed_roles,
            default_role: if params.clear_default_role {
                Some(None)
            } else {
                params.default_role.map(Some)
            },
            default_agent: if params.clear_default_agent {
                Some(None)
            } else {
                params.default_agent.map(Some)
            },
            mount_isolation_overrides: params.mount_isolation,
            keep_awake_enabled: keep_awake_change,
            git_pull_on_entry_enabled: git_pull_change,
        },
    )?;
    editor.save()?;
    println!("Updated workspace {name:?}:");
    for change in changes {
        println!("  - {change}");
    }
    Ok(())
}

async fn handle_workspace_edit(
    config: &AppConfig,
    paths: &JackinPaths,
    runner: &mut ShellRunner,
    connect_docker: impl Fn() -> Result<BollardDockerClient>,
    params: WorkspaceEditParams,
) -> Result<()> {
    let prepared = prepare_workspace_edit(config, &params)?;
    let changes = summarize_workspace_edit(&prepared, &params);
    apply_workspace_edit(paths, runner, connect_docker, params, prepared, &changes).await
}

fn handle_workspace_prune(
    config: &AppConfig,
    paths: &JackinPaths,
    name: String,
    assume_yes: bool,
) -> Result<()> {
    let current_ws = config
        .require_workspace(&WorkspaceName::parse(&name).map_err(anyhow::Error::from)?)?
        .clone();

    // All existing mounts; nothing new.
    let plan = workspace::plan_collapse(&current_ws.mounts, &[])?;
    if plan.removed.is_empty() {
        println!("Workspace {name:?} has no redundant mounts.");
        return Ok(());
    }

    if !assume_yes {
        crate::prompt::require_interactive_stdin(
            "refusing to collapse mounts without confirmation; pass --yes to proceed non-interactively",
        )?;
        eprintln!(
            "Will remove {} redundant mount(s) from workspace {name:?}:",
            plan.removed.len()
        );
        for r in &plan.removed {
            eprintln!(
                "  • {} (covered by {})",
                jackin_core::shorten_home(&r.child.src),
                jackin_core::shorten_home(&r.covered_by.src),
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

    let remove_dsts: Vec<String> = plan.removed.iter().map(|r| r.child.dst.clone()).collect();
    let mut editor = jackin_config::ConfigEditor::open(paths)?;
    editor.edit_workspace(
        &WorkspaceName::parse(&name).map_err(anyhow::Error::from)?,
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

fn handle_workspace_remove(paths: &JackinPaths, name: String) -> Result<()> {
    let mut editor = jackin_config::ConfigEditor::open(paths)?;
    editor.remove_workspace(&WorkspaceName::parse(&name).map_err(anyhow::Error::from)?)?;
    editor.save()?;
    println!("Removed workspace {name:?}.");
    Ok(())
}

fn handle_workspace_env(
    config: &mut AppConfig,
    paths: &JackinPaths,
    env_cmd: cli::WorkspaceEnvCommand,
) -> Result<()> {
    match env_cmd {
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
            if jackin_core::is_reserved(&key) {
                anyhow::bail!(
                    "env name {key:?} is reserved by the jackin runtime and cannot be set"
                );
            }
            config.require_workspace(
                &WorkspaceName::parse(&workspace).map_err(anyhow::Error::from)?,
            )?;
            if let Some(ref agent_key) = role
                && !config.roles.contains_key(agent_key)
            {
                anyhow::bail!(
                    "role {agent_key:?} is not registered; register it with \
                         `jackin role register` (or run a command that resolves it) \
                         before setting role-scoped env vars"
                );
            }
            let env_value = super::config_cmd::resolve_env_value_for_cli(&value)?;
            let scope = super::workspace_env_scope(workspace, role);
            let mut editor = jackin_config::ConfigEditor::open(paths)?;
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
            let ws = config.require_workspace(
                &WorkspaceName::parse(&workspace).map_err(anyhow::Error::from)?,
            )?;
            // CLAUDE_CODE_OAUTH_TOKEN under oauth_token mode is owned
            // by the claude-token orchestrator; an unset here would
            // silently break auth at the next launch.
            if key == jackin_env::CLAUDE_OAUTH_TOKEN_ENV
                && role.is_none()
                && ws.claude.as_ref().map(|c| c.auth_forward)
                    == Some(jackin_config::AuthForwardMode::OAuthToken)
            {
                anyhow::bail!(
                    "CLAUDE_CODE_OAUTH_TOKEN is managed by \
                         `jackin workspace claude-token` — use \
                         `jackin workspace claude-token revoke {workspace}` \
                         to clear it"
                );
            }
            let scope = super::workspace_env_scope(workspace, role);
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
        cli::WorkspaceEnvCommand::List { workspace, role } => {
            let ws = config.require_workspace(
                &WorkspaceName::parse(&workspace).map_err(anyhow::Error::from)?,
            )?;
            let vars: Vec<(String, String)> = role.as_ref().map_or_else(
                || {
                    ws.env
                        .iter()
                        .map(|(k, v)| (k.clone(), v.as_display_str().to_owned()))
                        .collect()
                },
                |a| {
                    ws.roles.get(a).map_or_else(Vec::new, |ov| {
                        ov.env
                            .iter()
                            .map(|(k, v)| (k.clone(), v.as_display_str().to_owned()))
                            .collect()
                    })
                },
            );
            super::config_cmd::print_env_table(&vars);
            Ok(())
        }
    }
}
