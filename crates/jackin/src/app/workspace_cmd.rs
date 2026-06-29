//! Workspace subcommand dispatch — extracted from `app::run` to keep mod.rs focused.

use anyhow::Result;

use crate::cli::{self, WorkspaceCommand};
use crate::workspace::{
    self, WorkspaceConfig, WorkspaceEdit, parse_mount_spec_resolved, resolve_path,
};
use jackin_config::AppConfig;
use jackin_core::JackinPaths;
use jackin_docker::ShellRunner;
use jackin_docker::docker_client::BollardDockerClient;

#[expect(
    clippy::too_many_lines,
    reason = "tracked in codebase-health-enforcement"
)]
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
        } => {
            let expanded_workdir = resolve_path(&workdir);
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
            editor.create_workspace(&name, ws)?;
            editor.save()?;
            println!(
                "Created workspace {name:?} (workdir: {}, {mount_count} mount(s)).",
                jackin_core::shorten_home(&workdir)
            );
            Ok(())
        }
        WorkspaceCommand::List(list_args) => {
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
                jackin_tui::output::hint("Run ", "jackin workspace show <name>", " for details.");
            }
            Ok(())
        }
        WorkspaceCommand::Show(show_args) => {
            let name = &show_args.name;
            let workspace = config.require_workspace(name)?;
            if cli::format::OutputFormat::parse(&show_args.fmt.format)
                == cli::format::OutputFormat::Json
            {
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
            if !all_collapses.is_empty() && !assume_yes {
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

            // Collect what changed for the summary (preserves the existing
            // summary output, plus collapse lines).
            let mut changes: Vec<String> = Vec::new();
            if let Some(ref w) = workdir {
                changes.push(format!("workdir → {}", jackin_core::shorten_home(w)));
            }
            for m in &upsert_mounts {
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
            for dst in &remove_destinations {
                changes.push(format!("removed mount {}", jackin_core::shorten_home(dst)));
            }
            for r in &all_collapses {
                changes.push(format!(
                    "collapsed {} under {}",
                    jackin_core::shorten_home(&r.child.src),
                    jackin_core::shorten_home(&r.covered_by.src)
                ));
            }
            if no_workdir_mount {
                changes.push("removed workdir auto-mount".to_owned());
            }
            for role in &allowed_roles {
                changes.push(format!("allowed role {role}"));
            }
            for role in &remove_allowed_roles {
                changes.push(format!("removed role {role}"));
            }
            if clear_default_role {
                changes.push("cleared default role".to_owned());
            } else if let Some(ref role) = default_role {
                changes.push(format!("default role → {role}"));
            }
            if clear_default_agent {
                changes.push("cleared default agent".to_owned());
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
            // Drift detection only needs Docker when isolation records
            // exist. Connecting first ensures the daemon is reachable
            // before we query it; skip the connection entirely when there
            // is nothing to check (common in fresh or test environments).
            let has_records = jackin_runtime::isolation::state::list_records_for_workspace(
                &paths.data_dir,
                &name,
            )
            .is_ok_and(|r| !r.is_empty());
            let detection = if has_records {
                let docker = connect_docker()?;
                crate::runtime::drift::detect_workspace_edit_drift(
                    paths,
                    &name,
                    &prospective_mounts,
                    &docker,
                )
                .await?
            } else {
                crate::runtime::drift::DriftDetection::default()
            };
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
                    jackin_runtime::isolation::cleanup::force_cleanup_isolated(
                        rec,
                        &container_dir,
                        &mut runner,
                    )
                    .await?;
                }
            }

            let mut editor = jackin_config::ConfigEditor::open(paths)?;
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

            let remove_dsts: Vec<String> =
                plan.removed.iter().map(|r| r.child.dst.clone()).collect();
            let mut editor = jackin_config::ConfigEditor::open(paths)?;
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
            let mut editor = jackin_config::ConfigEditor::open(paths)?;
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
                if jackin_core::env_model::is_reserved(&key) {
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
                let ws = config.require_workspace(&workspace)?;
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
                let ws = config.require_workspace(&workspace)?;
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
        },
        WorkspaceCommand::ClaudeToken(action) => super::handle_claude_token(paths, config, action),
    }
}
