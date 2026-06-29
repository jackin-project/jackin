//! Purge and Prune command handlers — extracted from `app::run`.

use anyhow::{Context, Result};
use owo_colors::OwoColorize;

use crate::cli::PruneCommand;
use crate::cli::cleanup::PurgeArgs;
use jackin_core::JackinPaths;
use jackin_core::Selector;
use jackin_docker::ShellRunner;
use jackin_docker::docker_client::BollardDockerClient;
use jackin_runtime::runtime;

use super::{resolve_instance_reference, resolve_role_to_container};

pub(super) async fn handle_purge(
    args: PurgeArgs,
    paths: &JackinPaths,
    runner: &mut ShellRunner,
    connect_docker: impl FnOnce() -> Result<BollardDockerClient>,
) -> Result<()> {
    let PurgeArgs { selector, all } = args;
    crate::preflight::preflight(crate::preflight::CheckName::preflight_required(), paths).await?;
    let docker = connect_docker()?;
    if let Some(container) = resolve_instance_reference(paths, &selector)? {
        if all {
            anyhow::bail!("--all applies only to role selectors, not instance IDs");
        }
        runtime::purge_container_state(paths, &container, &docker, runner).await?;
        println!("Purged state for {container}.");
        return Ok(());
    }

    match Selector::parse(&selector)? {
        Selector::Container(container) => {
            runtime::purge_container_state(paths, &container, &docker, runner).await?;
            println!("Purged state for {container}.");
            Ok(())
        }
        Selector::Role(class) => {
            if all {
                runtime::purge_class_data(paths, &class, &docker, runner).await?;
                println!("Purged all state for {}.", class.key());
            } else {
                let container = resolve_role_to_container(&class, &docker).await?;
                runtime::purge_container_state(paths, &container, &docker, runner).await?;
                println!("Purged state for {container}.");
            }
            Ok(())
        }
    }
}

pub(super) async fn handle_prune(
    cmd: PruneCommand,
    paths: &JackinPaths,
    runner: &mut ShellRunner,
    connect_docker: impl FnOnce() -> Result<BollardDockerClient>,
) -> Result<()> {
    match cmd {
        PruneCommand::Roles => runtime::prune_roles(paths),
        PruneCommand::Cache => runtime::prune_cache(paths),
        PruneCommand::Images => {
            let docker = connect_docker()?;
            runtime::prune_images(&docker).await
        }
        PruneCommand::Instances(args) => {
            let docker = connect_docker()?;
            if args.all {
                runtime::prune_all_instances(paths, &docker, runner).await
            } else {
                runtime::prune_instances(paths, &docker, runner).await
            }
        }
        PruneCommand::System(args) => {
            let docker = connect_docker()?;
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
            let prune_instances_result = if args.all {
                runtime::prune_all_instances(paths, &docker, runner)
                    .await
                    .context("prune instances")
            } else {
                runtime::prune_instances(paths, &docker, runner)
                    .await
                    .context("prune instances")
            };
            let results = [
                prune_instances_result,
                runtime::prune_images(&docker).await.context("prune images"),
                runtime::prune_roles(paths).context("prune roles"),
                runtime::prune_diagnostics(paths).context("prune diagnostics"),
                runtime::prune_cache(paths).context("prune cache"),
            ];
            let errors: Vec<anyhow::Error> = results.into_iter().filter_map(Result::err).collect();
            if errors.is_empty() {
                if args.all {
                    runtime::prune_jackin_home(paths);
                }
                Ok(())
            } else {
                for err in &errors {
                    eprintln!("{} {err:#}", "error:".red().bold());
                }
                anyhow::bail!("{} prune step(s) failed", errors.len())
            }
        }
    }
}
