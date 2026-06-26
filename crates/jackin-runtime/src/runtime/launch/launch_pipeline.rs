//! Role load pipeline: public entry points and the full launch-to-attach sequence.

use crate::instance::{
    DockerResources, InstanceManifest, InstanceStatus, NewInstanceManifest, PrepareResolvers,
    RoleState,
};
use anyhow::Context;
use jackin_config::AppConfig;
use jackin_core::CommandRunner;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_docker::docker_client::DockerApi;

use super::launch_slot::{
    claim_container_name, claim_known_container_name, github_env_declarations_for_mode,
    resolve_github_env_map, verify_credential_env_present, verify_github_token_present,
};
use super::trust::{inject_workspace_mise_env, seed_codex_project_trust};
use crate::runtime::attach::{
    AgentSessionInventory, ContainerState, hardline_agent, inspect_agent_sessions,
    start_or_hardline_agent, start_or_reconnect_capsule_client,
};
use crate::runtime::naming::{image_name, image_name_for_branch};
use crate::runtime::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

// Boxed future required: load_role calls itself recursively via
// RestoreResolution::RebuildRelatedRole — async fn recursion is not allowed.
pub fn load_role<'a>(
    paths: &'a JackinPaths,
    config: &'a mut AppConfig,
    selector: &'a RoleSelector,
    workspace: &'a jackin_config::ResolvedWorkspace,
    docker: &'a impl DockerApi,
    runner: &'a mut impl CommandRunner,
    opts: &'a super::LoadOptions,
) -> std::pin::Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>> {
    Box::pin(load_role_with(
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        opts,
        |_, _| anyhow::bail!("role trust prompt requires the rich launch dialog"),
        |_, _, _| anyhow::bail!("branch trust prompt requires the rich launch dialog"),
    ))
}

#[cfg(test)]
fn git_pull_program(opts: &super::LoadOptions) -> std::path::PathBuf {
    opts.git_program
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("git"))
}

#[cfg(not(test))]
fn git_pull_program(_opts: &super::LoadOptions) -> std::path::PathBuf {
    std::path::PathBuf::from("git")
}

async fn restore_current_role_now(
    paths: &JackinPaths,
    container: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    steps: &mut super::StepCounter,
    start_first: bool,
) -> anyhow::Result<()> {
    steps.finish_progress();
    let load_result = if start_first {
        start_or_hardline_agent(paths, container, docker, runner).await
    } else {
        hardline_agent(paths, container, docker, runner).await
    };
    super::render_exit(paths, docker).await;
    load_result
}

pub async fn resolve_supported_agents_for_console(
    paths: &JackinPaths,
    config: &AppConfig,
    selector: &RoleSelector,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<jackin_core::agent::Agent>> {
    // Lookup-only: the actual launch path uses
    // `AppConfig::resolve_role_source` which synthesizes + inserts a
    // RoleSource for unregistered namespaced selectors. That mutation
    // is for the launch (which persists trust), not for a transient
    // agent-list query that discards the config.
    let source = config
        .roles
        .get(&selector.key())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("unknown role selector {}", selector.key()))?;
    // Cached manifest is sufficient because the supported-agent set
    // rarely changes between fetches; the real launch re-fetches and
    // re-validates. Saves a git round trip per role-row Enter.
    let cached = jackin_manifest::repo::CachedRepo::new(paths, selector);
    if cached.repo_dir.join(".git").is_dir() {
        match jackin_manifest::load_role_manifest(&cached.repo_dir) {
            Ok(manifest) => return Ok(manifest.supported_agents()),
            Err(error) => jackin_diagnostics::debug_log!(
                "console",
                "cached manifest for {} present but failed to parse ({error:#}); refetching",
                selector.key()
            ),
        }
    } else {
        jackin_diagnostics::debug_log!(
            "console",
            "no cached repo for {}; falling back to git fetch",
            selector.key()
        );
    }
    let (_, validated_repo, _repo_lock) = resolve_agent_repo_with(
        paths,
        selector,
        &source.git,
        runner,
        RepoResolveOptions::non_interactive(),
        || Ok(false),
    )
    .await?;
    Ok(validated_repo.manifest.supported_agents())
}

/// Instrument the full launch pipeline so every stage appears as a
/// child span in the diagnostics run log so stage events carry real `span_id` correlation.
#[tracing::instrument(
    skip_all,
    fields(role = %selector.key())
)]
#[expect(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(crate) async fn load_role_with(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &RoleSelector,
    workspace: &jackin_config::ResolvedWorkspace,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    opts: &super::LoadOptions,
    confirm_trust_for_test: impl FnOnce(&RoleSelector, &jackin_config::RoleSource) -> anyhow::Result<()>,
    confirm_branch_for_test: impl FnOnce(
        &RoleSelector,
        &jackin_config::RoleSource,
        &str,
    ) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // Pre-launch garbage collection is independent from git identity probes.
    let ((), git) = tokio::join!(
        crate::runtime::cleanup::gc_orphaned_resources(docker),
        crate::runtime::identity::load_git_identity(runner)
    );

    // `app::run` claims the first-entry boundary immediately before a real
    // launch so the two-screen intro only plays from an empty construct. Direct
    // test/internal callers still need the elapsed-time marker for the last-exit
    // outro, so this idempotently writes one if the app layer did not.
    crate::runtime::universe::mark_start(
        paths,
        crate::runtime::universe::StartKind::ResumeExisting,
    );

    // `load_role` receives a `ResolvedWorkspace` (mounts + workdir),
    // not a name. Recover the name by matching workdir, mirroring the
    // identification rule used by `jackin workspace show`.
    let workspace_name = config
        .workspaces
        .iter()
        .find(|(_, w)| w.workdir == workspace.workdir)
        .map(|(name, _)| name.clone());

    let mut steps = super::StepCounter::new(&selector.name);
    if let Some(run) = jackin_diagnostics::active_run() {
        let mut progress = crate::runtime::progress::LaunchProgress::new(
            run,
            std::env::var_os("JACKIN_NO_MOTION").is_some(),
            crate::runtime::progress::host_terminal(),
            env!("JACKIN_VERSION"),
        )?;
        progress.started(crate::runtime::progress::LaunchIdentity {
            role: selector.name.clone(),
            agent: opts
                .agent
                .or(workspace.default_agent)
                .map_or_else(|| "resolving".to_owned(), |agent| agent.slug().to_owned()),
            target_kind: super::launch_target_kind(workspace_name.as_deref()),
            target_label: super::launch_target_label(workspace_name.as_deref(), workspace),
            mounts: super::launch_mount_lines(workspace),
            image: None,
            container: None,
        });
        progress.stage_done(
            crate::runtime::progress::LaunchStage::Identity,
            "resolved operator",
        );
        steps.start_progress(progress);
    }

    let sensitive = jackin_config::find_sensitive_mounts(&workspace.mounts);
    if !sensitive.is_empty() {
        let prompt = super::sensitive_mount_prompt(&sensitive);
        let confirmed = if let Some(progress) = steps.progress_mut() {
            progress.confirm_prompt(prompt)?
        } else {
            anyhow::bail!("sensitive mount confirmation requires the rich launch dialog")
        };
        if !confirmed {
            anyhow::bail!("aborted — sensitive mount paths were not confirmed");
        }
    }

    let role_key = selector.key();
    let selected_agent_before_role = opts.agent.or(workspace.default_agent);
    let mut early_restore_agent = None;
    // `--rebuild` is an explicit "force a fresh image" request, so it must not
    // take the attach/start/recreate fast paths — those short-circuit before
    // `decide_agent_image` and would silently skip the rebuild. Falling through
    // routes the launch to the normal pipeline, where `decide_agent_image`
    // returns `ExplicitRebuild` and the build always runs. Container-name
    // collisions are handled downstream by `claim_container_name`: a running
    // session is left intact (a fresh, rebuilt instance is created alongside
    // it), while a stopped/crashed/missing container is reclaimed and recreated
    // from the rebuilt image.
    let early_restore_container = if opts.restore_container_base.is_none()
        && opts.role_branch.is_none()
        && !opts.rebuild
    {
        if let Some(agent) = selected_agent_before_role {
            match super::resolve_current_restore_candidate_timed(
                paths,
                workspace_name.as_deref(),
                workspace.label.as_str(),
                &workspace.workdir,
                &role_key,
                agent,
                docker,
            )
            .await?
            {
                Some(super::RestoreResolution::AttachCurrentRole(container)) => {
                    jackin_diagnostics::debug_log!(
                        "restore",
                        "attaching current running instance {container} before role repo, credentials, and image prep"
                    );
                    return restore_current_role_now(
                        paths, &container, docker, runner, &mut steps, false,
                    )
                    .await;
                }
                Some(super::RestoreResolution::StartCurrentRole(container)) => {
                    jackin_diagnostics::debug_log!(
                        "restore",
                        "starting current stopped instance {container} before role repo, credentials, and image prep"
                    );
                    return restore_current_role_now(
                        paths, &container, docker, runner, &mut steps, true,
                    )
                    .await;
                }
                Some(super::RestoreResolution::RecreateCurrentRole(container)) => {
                    jackin_diagnostics::debug_log!(
                        "restore",
                        "recreating missing current instance {container} after role repo resolution"
                    );
                    Some(container)
                }
                Some(_) | None => None,
            }
        } else {
            match super::resolve_unselected_current_restore_candidate_with_agent_timed(
                paths,
                workspace_name.as_deref(),
                workspace.label.as_str(),
                &workspace.workdir,
                &role_key,
                docker,
            )
            .await?
            {
                Some(super::UnselectedCurrentRestoreResolution {
                    resolution: super::RestoreResolution::AttachCurrentRole(container),
                    ..
                }) => {
                    jackin_diagnostics::debug_log!(
                        "restore",
                        "attaching single-agent current instance {container} before role repo, credentials, and image prep"
                    );
                    return restore_current_role_now(
                        paths, &container, docker, runner, &mut steps, false,
                    )
                    .await;
                }
                Some(super::UnselectedCurrentRestoreResolution {
                    resolution: super::RestoreResolution::StartCurrentRole(container),
                    ..
                }) => {
                    jackin_diagnostics::debug_log!(
                        "restore",
                        "starting single-agent current instance {container} before role repo, credentials, and image prep"
                    );
                    return restore_current_role_now(
                        paths, &container, docker, runner, &mut steps, true,
                    )
                    .await;
                }
                Some(super::UnselectedCurrentRestoreResolution {
                    resolution: super::RestoreResolution::RecreateCurrentRole(container),
                    agent,
                }) => {
                    early_restore_agent = Some(agent);
                    jackin_diagnostics::debug_log!(
                        "restore",
                        "recreating single-agent missing current instance {container} after role repo resolution"
                    );
                    Some(container)
                }
                Some(_) | None => None,
            }
        }
    } else {
        None
    };

    if let Some(container) = opts.restore_container_base.as_ref() {
        jackin_diagnostics::active_timing_started(
            "restore",
            "explicit_restore_container",
            Some(container),
        );
        let docker_state = docker.inspect_container_state(container).await;
        jackin_diagnostics::active_timing_done(
            "restore",
            "explicit_restore_container",
            Some(docker_state.short_label().as_str()),
        );
        match docker_state {
            ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
                super::emit_launch_plan(
                    super::LaunchPlan::AttachExisting,
                    "explicit_restore_container_running",
                    Some(container),
                );
                jackin_diagnostics::debug_log!(
                    "restore",
                    "attaching explicit restore container {container} before role repo, credentials, and image prep"
                );
                return restore_current_role_now(
                    paths, container, docker, runner, &mut steps, false,
                )
                .await;
            }
            ContainerState::Stopped { .. } | ContainerState::Created => {
                super::emit_launch_plan(
                    super::LaunchPlan::StartStopped,
                    "explicit_restore_container_startable",
                    Some(container),
                );
                jackin_diagnostics::debug_log!(
                    "restore",
                    "starting explicit restore container {container} before role repo, credentials, and image prep"
                );
                return restore_current_role_now(
                    paths, container, docker, runner, &mut steps, true,
                )
                .await;
            }
            ContainerState::InspectUnavailable(reason) => {
                anyhow::bail!(
                    "{}",
                    crate::runtime::attach::docker_unavailable_msg(
                        &format!("inspect explicit restore container `{container}`"),
                        &reason,
                    )
                );
            }
            ContainerState::NotFound | ContainerState::Removing | ContainerState::Dead => {}
        }
    }

    let (source, is_new, restore_source_override) = super::resolve_launch_role_source(
        config,
        selector,
        opts.restore_role_source_git.as_deref(),
    )?;

    // Step 1: Resolve role identity (clone or update repo)
    steps.next("Resolving role identity").await?;

    let mut confirm_repo_removal = || {
        if let Some(progress) = steps.progress_mut() {
            return progress
                .confirm_prompt("Remove the cached repo and re-clone from the configured source?");
        }
        anyhow::bail!("cached repo recovery prompt requires the rich launch dialog")
    };
    jackin_diagnostics::active_timing_started(
        "role",
        "repo_refresh",
        Some(selector.key().as_str()),
    );
    let repo_result = resolve_agent_repo_with(
        paths,
        selector,
        &source.git,
        runner,
        RepoResolveOptions::interactive(opts.debug).with_branch(opts.role_branch.as_deref()),
        &mut confirm_repo_removal,
    )
    .await;
    let (cached_repo, validated_repo, repo_lock) = match repo_result {
        Ok(repo) => {
            jackin_diagnostics::active_timing_done("role", "repo_refresh", Some("validated"));
            repo
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done("role", "repo_refresh", Some("error"));
            return Err(error);
        }
    };

    // Trust gate: prompt the operator before running an untrusted third-party role
    let newly_trusted = if source.trusted {
        false
    } else {
        let confirmed = if let Some(progress) = steps.progress_mut() {
            progress.confirm_role_trust(selector.key(), source.git.clone())?
        } else {
            confirm_trust_for_test(selector, &source)?;
            true
        };
        if !confirmed {
            anyhow::bail!(
                "role source \"{selector}\" not trusted — aborting.\n\
                 To trust it later, run `jackin config trust grant {selector}` or try loading again."
            );
        }
        // Mutate the in-memory copy so callers downstream see the trust
        // without a reload; persist via editor below.
        if let Some(entry) = config.roles.get_mut(&selector.key()) {
            entry.trusted = true;
        }
        true
    };

    if !restore_source_override && (is_new || newly_trusted) {
        let mut editor = jackin_config::ConfigEditor::open(paths)?;
        if let Some(role_source) = config.roles.get(&selector.key()) {
            editor.upsert_agent_source(&selector.key(), role_source);
        }
        editor.set_agent_trust(&selector.key(), true);
        *config = editor.save()?;
    }

    let agent_display_name = validated_repo.manifest.display_name(&selector.name);
    steps.role_name.clone_from(&agent_display_name);

    let supported_agents = validated_repo.manifest.supported_agents();
    let agent = match opts
        .agent
        .or(workspace.default_agent)
        .or(early_restore_agent)
    {
        Some(a) => a,
        None if supported_agents.len() == 1 => supported_agents[0],
        None if supported_agents.len() >= 2 => {
            let labels: Vec<String> = supported_agents
                .iter()
                .map(|a| a.slug().to_owned())
                .collect();
            if let Some(progress) = steps.progress_mut() {
                let selection = progress.select_choice("Choose launch agent", labels)?;
                supported_agents[selection]
            } else {
                anyhow::bail!(
                    "role \"{}\" supports multiple agents ({:?}); load requires the rich launch dialog for agent selection, or pass --agent / set workspace `default_agent`",
                    selector.key(),
                    supported_agents
                        .iter()
                        .map(|a| a.slug())
                        .collect::<Vec<_>>()
                )
            }
        }
        None if supported_agents.is_empty() => anyhow::bail!(
            "role \"{}\" declares no supported agents in its manifest",
            selector.key()
        ),
        None => anyhow::bail!(
            "role \"{}\" supports multiple agents ({:?}); pass --agent, set workspace `default_agent`, or use the rich launch dialog",
            selector.key(),
            supported_agents
                .iter()
                .map(|a| a.slug())
                .collect::<Vec<_>>()
        ),
    };
    super::validate_agent_supported(selector, &validated_repo.manifest, agent)?;

    // Branch trust gate: fires even for already-trusted roles because the
    // operator trusted the default branch, not this unreviewed PR branch.
    if let Some(branch) = opts.role_branch.as_deref() {
        let prompt = format!(
            "Role `{selector}` is being loaded from unmerged branch `{branch}`.\n\
             Its Dockerfile and scripts may differ from the trusted main branch.\n\
             Have you reviewed the branch diff and verified it is safe to build?"
        );
        let confirmed = if let Some(progress) = steps.progress_mut() {
            progress.confirm_prompt(prompt)?
        } else {
            confirm_branch_for_test(selector, &source, branch)?;
            true
        };
        if !confirmed {
            anyhow::bail!(
                "branch \"{branch}\" not confirmed — aborting.\n\
                 Review the Dockerfile and scripts on that branch before loading it."
            );
        }
    }

    let restore_container = if early_restore_container.is_some() {
        early_restore_container
    } else if let Some(container) = opts.restore_container_base.as_ref() {
        Some(container.clone())
    } else if opts.rebuild {
        // `--rebuild` skips the early gate above (it is `&& !opts.rebuild`), so
        // a forced rebuild actually falls through to *this* resolution. Without
        // the same guard here, `resolve_restore_candidate` would still return
        // `AttachCurrentRole`/`StartCurrentRole` and `return` straight into the
        // existing container — silently skipping the build the operator asked
        // for. Leave `restore_container` `None` so the normal pipeline runs
        // `decide_agent_image` -> `ExplicitRebuild` and always rebuilds;
        // `claim_container_name` reconciles any name collision downstream.
        None
    } else {
        match super::resolve_restore_candidate(
            paths,
            workspace_name.as_deref(),
            workspace.label.as_str(),
            &workspace.workdir,
            &role_key,
            agent,
            docker,
            steps.progress_mut(),
        )
        .await?
        {
            super::RestoreResolution::StartFresh => None,
            super::RestoreResolution::AttachCurrentRole(container) => {
                jackin_diagnostics::debug_log!(
                    "restore",
                    "attaching current running instance {container} before credentials and image prep"
                );
                return restore_current_role_now(
                    paths, &container, docker, runner, &mut steps, false,
                )
                .await;
            }
            super::RestoreResolution::StartCurrentRole(container) => {
                jackin_diagnostics::debug_log!(
                    "restore",
                    "starting current stopped instance {container} before credentials and image prep"
                );
                return restore_current_role_now(
                    paths, &container, docker, runner, &mut steps, true,
                )
                .await;
            }
            super::RestoreResolution::RecreateCurrentRole(container) => {
                jackin_diagnostics::debug_log!(
                    "restore",
                    "recreating missing current instance {container} with normal image decision"
                );
                Some(container)
            }
            super::RestoreResolution::RestoreCurrentRole(container) => Some(container),
            super::RestoreResolution::RecoverRelatedRole(container) => {
                steps.finish_progress();
                let load_result = hardline_agent(paths, &container, docker, runner)
                    .await
                    .map(|()| container);
                match load_result {
                    Ok(_) => {
                        super::render_exit(paths, docker).await;
                        return Ok(());
                    }
                    Err(error) => {
                        super::render_exit(paths, docker).await;
                        return Err(error);
                    }
                }
            }
            super::RestoreResolution::RebuildRelatedRole(manifest) => {
                steps.finish_progress();
                let selector = RoleSelector::parse(&manifest.role_key)?;
                let related_opts = super::related_restore_load_options(opts, &manifest)?;
                let load_result = load_role(
                    paths,
                    config,
                    &selector,
                    workspace,
                    docker,
                    runner,
                    &related_opts,
                )
                .await
                .map(|()| manifest.container_base);
                match load_result {
                    Ok(_) => {
                        super::render_exit(paths, docker).await;
                        return Ok(());
                    }
                    Err(error) => {
                        super::render_exit(paths, docker).await;
                        return Err(error);
                    }
                }
            }
        }
    };

    if workspace.git_pull_on_entry {
        let sources = super::git_pull_sources(workspace);
        if let Some(progress) = steps.progress_mut() {
            if sources.is_empty() {
                jackin_diagnostics::active_timing_started("workspace", "git_pull_on_entry", None);
                jackin_diagnostics::active_timing_done(
                    "workspace",
                    "git_pull_on_entry",
                    Some("skipped_no_git_repos"),
                );
                progress.stage_skipped(
                    crate::runtime::progress::LaunchStage::Workspace,
                    "no mounted git repositories",
                );
            } else {
                jackin_diagnostics::active_timing_started(
                    "workspace",
                    "git_pull_on_entry",
                    Some(&format!("{} repo(s)", sources.len())),
                );
                progress.stage_started(
                    crate::runtime::progress::LaunchStage::Workspace,
                    format!("polling {} workspace repositories", sources.len()),
                );
                let debug = opts.debug;
                let git_program = git_pull_program(opts);
                let pull = tokio::task::spawn_blocking(move || {
                    super::pull_git_sources_with_git(sources, debug, &git_program, false)
                });
                let results = progress
                    .while_waiting(async move {
                        pull.await
                            .map_err(|error| anyhow::anyhow!("joining git pull worker: {error}"))
                    })
                    .await?;
                let (ok, failed) = super::record_git_pull_results(&results);
                let detail = if failed == 0 {
                    format!("{ok} repositories current")
                } else {
                    format!("{ok} repositories current; {failed} failed")
                };
                jackin_diagnostics::active_timing_done(
                    "workspace",
                    "git_pull_on_entry",
                    Some(&detail),
                );
                progress.stage_done(crate::runtime::progress::LaunchStage::Workspace, detail);
            }
        } else if !sources.is_empty() {
            jackin_diagnostics::active_timing_started(
                "workspace",
                "git_pull_on_entry",
                Some(&format!("{} repo(s)", sources.len())),
            );
            // Run the blocking git pulls on a blocking-pool thread so the
            // single-threaded executor is never parked on the join.
            let debug = opts.debug;
            let git_program = git_pull_program(opts);
            let results = tokio::task::spawn_blocking(move || {
                super::pull_git_sources_with_git(sources, debug, &git_program, true)
            })
            .await
            .map_err(|error| anyhow::anyhow!("joining git pull worker: {error}"))?;
            super::print_git_pull_results(&results);
            let (ok, failed) = super::record_git_pull_results(&results);
            let detail = if failed == 0 {
                format!("{ok} repositories current")
            } else {
                format!("{ok} repositories current; {failed} failed")
            };
            jackin_diagnostics::active_timing_done("workspace", "git_pull_on_entry", Some(&detail));
        }
    }
    let restoring = restore_container.is_some();
    let (container_name, _name_lock) = if let Some(container_name) = restore_container {
        claim_known_container_name(paths, &container_name, docker).await?
    } else {
        claim_container_name(paths, workspace_name.as_deref(), selector, docker).await?
    };

    // Preliminary panel name only. The authoritative, commit-tagged image name
    // (`jk_<role>:<sha>`) is resolved in the image decision below, which is the
    // single place that runs the role-SHA git capture — the launch path does not
    // pay for an extra `git rev-parse` just to render the panel up front.
    let image_tag = opts.role_branch.as_deref().map_or_else(
        || image_name(selector, None),
        |b| image_name_for_branch(selector, b, None),
    );
    if let Some(progress) = steps.progress_mut() {
        progress.update_identity(crate::runtime::progress::LaunchIdentity {
            role: agent_display_name.clone(),
            agent: agent.slug().to_owned(),
            target_kind: super::launch_target_kind(workspace_name.as_deref()),
            target_label: super::launch_target_label(workspace_name.as_deref(), workspace),
            mounts: super::launch_mount_lines(workspace),
            image: Some(image_tag.clone()),
            container: Some(container_name.clone()),
        });
        progress.stage_done(
            crate::runtime::progress::LaunchStage::Role,
            "trusted source",
        );
    }

    // Decide whether the selected image is already runnable before touching
    // operator/manifest/GitHub env. Creating a fresh container still needs
    // credentials later, but a warm recipe hit should be visible before any
    // unrelated secret graph can block the launch.
    let rebuild = opts.rebuild;
    if let Some(progress) = steps.progress_mut() {
        progress.stage_started(
            crate::runtime::progress::LaunchStage::Construct,
            "verifying construct",
        );
        progress.stage_done(crate::runtime::progress::LaunchStage::Construct, "online");
    }
    steps.next("Preparing derived image").await?;
    let mut repo_lock = Some(repo_lock);
    let image_decision = crate::runtime::image::decide_role_image(
        paths,
        selector,
        &cached_repo,
        &validated_repo,
        rebuild,
        opts.role_branch.as_deref(),
        docker,
        runner,
    )
    .await?;

    if let Some(progress) = steps.progress_mut() {
        progress.stage_started(
            crate::runtime::progress::LaunchStage::Credentials,
            "resolving launch credentials",
        );
    }

    // Resolve operator env layers (global / role / workspace /
    // workspace × role) before manifest env. Operator-provided values
    // preseed matching manifest variables, so a configured value does
    // not ask the operator the same question again.
    //
    // The operator env resolver takes two injection seams:
    //   * `op_runner`  — resolves `op://...` references (production:
    //     `OpCli::new()`; tests: a mock `OpRunner` constructed directly).
    //   * `host_env`   — resolves `$NAME` / `${NAME}` references
    //     (production: `|name| std::env::var(name).ok()`; tests: a
    //     closure over a `BTreeMap` seeded by the test).
    //
    // Both seams are carried on `LoadOptions` as optional fields. When
    // unset (the production default), `resolve_operator_env` is called,
    // which wires in the real `OpCli` and the real host env. When set
    // (tests only), `resolve_operator_env_with` is called with the
    // supplied seams, so tests never need to mutate `std::env` and the
    // crate-level `unsafe_code = "forbid"` lint stays intact.
    let auth_mode = jackin_config::resolve_mode(
        config,
        agent,
        workspace_name.as_deref().unwrap_or(""),
        &role_key,
    );
    // Resolve every credential any agent the role can run might read from the
    // container env, plus generic operator vars. The selected agent is one of
    // these; sibling agents share the same container env and an ApiKey/OAuth
    // tab reads its key from that env at `docker run`, so gating a supported
    // agent's key out would start that tab without auth. Only credentials for
    // agents the role cannot launch are skipped.
    let credential_agents = validated_repo.manifest.supported_agents();
    let operator_env_needed = |key: &str| credential_key_needed_for_role(&credential_agents, key);
    let operator_env = if jackin_env::has_operator_env_matching(
        config,
        Some(&selector.key()),
        workspace_name.as_deref(),
        operator_env_needed,
    ) {
        jackin_diagnostics::active_timing_started("credentials", "operator_env", None);
        let operator_env_result = if opts.op_runner.is_none() && opts.host_env.is_none() {
            // Offload `op` CLI calls to the blocking pool so the tokio render
            // thread stays responsive during 1Password lookups (Defect 43).
            let config_clone = config.clone();
            let selector_key = selector.key().clone();
            let workspace_key = workspace_name.as_deref().map(String::from);
            let credential_agents = credential_agents.clone();
            tokio::task::spawn_blocking(move || {
                jackin_env::resolve_operator_env_matching(
                    &config_clone,
                    Some(&selector_key),
                    workspace_key.as_deref(),
                    |key| credential_key_needed_for_role(&credential_agents, key),
                )
            })
            .await
            .map_err(|e| anyhow::anyhow!("env resolver panicked: {e}"))?
        } else {
            let default_runner = jackin_env::OpCli::new();
            let runner: &dyn jackin_env::OpRunner =
                opts.op_runner.as_deref().unwrap_or(&default_runner);
            let host_env_fn = |name: &str| -> Result<String, std::env::VarError> {
                opts.host_env.as_ref().map_or_else(
                    || std::env::var(name),
                    |map| map.get(name).cloned().ok_or(std::env::VarError::NotPresent),
                )
            };
            jackin_env::resolve_operator_env_with_matching(
                config,
                Some(&selector.key()),
                workspace_name.as_deref(),
                runner,
                host_env_fn,
                operator_env_needed,
            )
        };
        match operator_env_result {
            Ok(env) => {
                jackin_diagnostics::active_timing_done(
                    "credentials",
                    "operator_env",
                    Some(&format!("{} vars", env.len())),
                );
                env
            }
            Err(error) => {
                jackin_diagnostics::active_timing_done(
                    "credentials",
                    "operator_env",
                    Some("error"),
                );
                return Err(error);
            }
        }
    } else {
        jackin_diagnostics::active_timing_started("credentials", "operator_env", None);
        jackin_diagnostics::active_timing_done("credentials", "operator_env", Some("skipped"));
        std::collections::BTreeMap::new()
    };

    // Resolve env vars (interactive prompts happen here, before build)
    jackin_diagnostics::active_timing_started("credentials", "manifest_env", None);
    let manifest_env: std::collections::BTreeMap<_, _> = validated_repo
        .manifest
        .env
        .iter()
        .filter(|(key, _)| credential_key_needed_for_role(&credential_agents, key))
        .map(|(key, decl)| (key.clone(), decl.clone()))
        .collect();
    let manifest_env_skipped = manifest_env.is_empty();
    let manifest_resolved_result = if manifest_env_skipped {
        Ok(jackin_env::ResolvedEnv { vars: vec![] })
    } else {
        let prompter = super::LaunchEnvPrompter::new(steps.progress_mut());
        jackin_env::resolve_env_with_overrides(&manifest_env, &prompter, &operator_env)
    };
    let manifest_resolved = match manifest_resolved_result {
        Ok(env) => {
            jackin_diagnostics::active_timing_done(
                "credentials",
                "manifest_env",
                Some(&manifest_env_timing_detail(
                    manifest_env_skipped,
                    env.vars.len(),
                )),
            );
            env
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done("credentials", "manifest_env", Some("error"));
            return Err(error);
        }
    };

    // Overlay the operator env map on top of the manifest env: operator
    // wins on conflicts (so a workspace-scoped `OPERATOR_TOKEN` overrides
    // a manifest default, which is the whole point of letting operators
    // supply env at launch time). Reserved names are filtered out in
    // the docker-run construction below.
    let mut merged_vars: Vec<(String, String)> = manifest_resolved.vars;
    for (k, v) in &operator_env {
        if let Some(slot) = merged_vars.iter_mut().find(|(mk, _)| mk == k) {
            slot.1.clone_from(v);
        } else {
            merged_vars.push((k.clone(), v.clone()));
        }
    }
    inject_workspace_mise_env(&mut merged_vars, workspace);

    // On-demand credential bindings (jackin-exec). These were filtered out of
    // launch-time resolution above (never `op read` at launch); here we surface
    // only their NAMES to the agent via `JACKIN_EXEC_BINDINGS` — an
    // always-available var the entrypoint turns into a system-prompt block. The
    // full (name, kind, source) triples flow host-side to the credential
    // resolver via `capsule_config.exec_bindings` below.
    let exec_bindings: Vec<jackin_protocol::ExecBinding> = jackin_env::collect_on_demand_bindings(
        config,
        Some(role_key.as_str()),
        workspace_name.as_deref(),
    )
    .into_iter()
    .map(|(name, kind, source)| jackin_protocol::ExecBinding { name, kind, source })
    .collect();
    if !exec_bindings.is_empty() {
        let names = super::exec_binding_names(&exec_bindings);
        merged_vars.retain(|(k, _)| k != "JACKIN_EXEC_BINDINGS");
        merged_vars.push(("JACKIN_EXEC_BINDINGS".to_owned(), names));
    }
    let resolved_env = jackin_env::ResolvedEnv { vars: merged_vars };

    // Launch-time diagnostic: emit a single compact line summarising
    // the operator env that will be injected. In normal mode we show
    // counts only ("3 refs resolved"); in --debug mode we show each
    // key → layer/reference kind ("OPERATOR_TOKEN: op://Personal/...
    // from workspace \"big-monorepo\"") — never values.
    if !operator_env.is_empty() {
        jackin_env::print_launch_diagnostic(
            config,
            Some(&selector.key()),
            workspace_name.as_deref(),
            &operator_env,
            opts.debug,
        );
    }
    if let Some(progress) = steps.progress_mut() {
        progress.stage_done(
            crate::runtime::progress::LaunchStage::Credentials,
            "resolved",
        );
    }

    let selected_refresh_reason = match &image_decision {
        crate::runtime::image::ImageDecision::RefreshInBackground { reason, .. } => Some(*reason),
        crate::runtime::image::ImageDecision::Reuse { .. }
        | crate::runtime::image::ImageDecision::BuildFromPublished { .. }
        | crate::runtime::image::ImageDecision::BuildFromWorkspace { .. } => None,
    };

    let load_result: anyhow::Result<String> = async {
        // Step 2: Prepare runtime assets and build the derived image when the
        // earlier image decision proved the local recipe is missing/stale.
        let (image, selected_image_reused) = match image_decision {
            decision @ (
                crate::runtime::image::ImageDecision::Reuse { .. }
                | crate::runtime::image::ImageDecision::RefreshInBackground { .. }
            ) => {
                let (image, materialization_reason) = match decision {
                    crate::runtime::image::ImageDecision::Reuse { image } => {
                        (image, "recipe_hash_match")
                    }
                    crate::runtime::image::ImageDecision::RefreshInBackground { image, reason } => {
                        (image, reason.as_str())
                    }
                    _ => unreachable!(),
                };
                super::emit_image_materialization_plan(
                    true,
                    materialization_reason,
                    restoring,
                    &container_name,
                );
                drop(repo_lock.take());
                if let Some(progress) = steps.progress_mut() {
                    progress.stage_skipped(
                        crate::runtime::progress::LaunchStage::AgentBinaries,
                        "image reused",
                    );
                    progress.stage_done(
                        crate::runtime::progress::LaunchStage::DerivedImage,
                        "reused local image",
                    );
                }
                (image, true)
            }
            build_decision @ (
                crate::runtime::image::ImageDecision::BuildFromPublished { .. }
                | crate::runtime::image::ImageDecision::BuildFromWorkspace { .. }
            ) => {
                let (reason, role_git_sha, build_source, build_base_image_override) =
                    match build_decision {
                    crate::runtime::image::ImageDecision::BuildFromPublished {
                        reason,
                        role_git_sha,
                        base_image,
                    } => (
                        reason,
                        role_git_sha,
                        format!("published image {base_image}"),
                        Some(base_image),
                    ),
                    crate::runtime::image::ImageDecision::BuildFromWorkspace {
                        reason,
                        role_git_sha,
                    } => (
                        reason,
                        role_git_sha,
                        "workspace Dockerfile".to_owned(),
                        None,
                    ),
                    crate::runtime::image::ImageDecision::Reuse { .. }
                    | crate::runtime::image::ImageDecision::RefreshInBackground { .. } => {
                        unreachable!()
                    }
                };
                super::emit_image_materialization_plan(
                    false,
                    reason.as_str(),
                    restoring,
                    &container_name,
                );
                jackin_diagnostics::debug_log!(
                    "image",
                    "derived image build required from {}: {}",
                    build_source,
                    reason.as_str(),
                );
                steps.next("Preparing runtime binaries").await?;
                // Prepare every agent the role supports, not just the selected
                // one: the running container hosts a multiplexer where the
                // operator can open a new tab for ANY supported agent, and that
                // tab execs the agent CLI inside this same container. Baking
                // only the selected agent makes sibling tabs crash on a missing
                // binary. The selected agent still drives the version label and
                // the foreground session; the others must simply be present.
                let image_agents = validated_repo.manifest.supported_agents();
                let runtime_binaries = if let Some(progress) = steps.progress_mut() {
                    crate::runtime::image::prepare_runtime_binaries_for_agents(
                        paths,
                        &validated_repo,
                        &image_agents,
                        Some(progress),
                    )
                    .await?
                } else {
                    crate::runtime::image::prepare_runtime_binaries_for_agents(
                        paths,
                        &validated_repo,
                        &image_agents,
                        None,
                    )
                    .await?
                };
                steps.next("Preparing derived image").await?;
                let repo_lock = repo_lock
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("repo lock already consumed"))?;
                let image = if let Some(progress) = steps.progress_mut() {
                    crate::runtime::image::build_agent_image(
                        paths,
                        selector,
                        &cached_repo,
                        &validated_repo,
                        agent,
                        runtime_binaries,
                        rebuild,
                        reason,
                        build_base_image_override.as_deref(),
                        opts.debug,
                        opts.role_branch.as_deref(),
                        docker,
                        runner,
                        repo_lock,
                        role_git_sha.as_deref(),
                        Some(progress),
                    )
                    .await?
                } else {
                    crate::runtime::image::build_agent_image(
                        paths,
                        selector,
                        &cached_repo,
                        &validated_repo,
                        agent,
                        runtime_binaries,
                        rebuild,
                        reason,
                        build_base_image_override.as_deref(),
                        opts.debug,
                        opts.role_branch.as_deref(),
                        docker,
                        runner,
                        repo_lock,
                        role_git_sha.as_deref(),
                        None,
                    )
                    .await?
                };
                (image, false)
            }
        };
        let container_state = paths.data_dir.join(&container_name);
        let adopted_sidecar = super::adopt_prewarmed_dind_sidecar(paths, docker).await;
        let resources = adopted_sidecar.as_ref().map_or_else(
            || DockerResources::from_container_name(&container_name),
            |sidecar| DockerResources {
                role_container: container_name.clone(),
                dind_container: sidecar.sidecar.dind.clone(),
                network: sidecar.sidecar.network.clone(),
                certs_volume: sidecar.sidecar.certs_volume.clone(),
            },
        );
        let network = resources.network.clone();
        let dind = resources.dind_container.clone();
        let certs_volume = resources.certs_volume.clone();
        // Arm cleanup immediately after adoption, before any fallible step.
        // When a prewarmed DinD sidecar was adopted, its container, network,
        // and certs volume are already *running* and the on-disk prewarm state
        // was deleted (`adopt_prewarmed_dind_sidecar` calls
        // `remove_prewarmed_dind_state`), so nothing re-adopts them. Any early
        // `?`/`return Err` between here and the start of the launch proper
        // (status write, credential preflights, GitHub-token preflight — a
        // missing token is a routine operator error) would otherwise orphan a
        // live privileged container with no record. `LoadCleanup::run` is
        // best-effort: removing the not-yet-created role container is a no-op.
        // For a fresh (non-adopted) launch the sidecar is not started until
        // after this point, so there is nothing to leak in the gap.
        let socket_dir = paths.jackin_home.join("sockets").join(&container_name);
        let mut cleanup = super::LoadCleanup::new(
            container_name.clone(),
            dind.clone(),
            certs_volume.clone(),
            network.clone(),
            socket_dir,
        );
        let host_workdir_fingerprint = super::manifest_host_workdir_fingerprint(workspace);
        let new_manifest = InstanceManifest::new(NewInstanceManifest {
            container_base: &container_name,
            workspace_name: workspace_name.as_deref(),
            workspace_label: workspace.label.as_str(),
            workdir: &workspace.workdir,
            host_workdir_fingerprint: &host_workdir_fingerprint,
            role_key: &role_key,
            role_display_name: &agent_display_name,
            agent_runtime: agent,
            role_source_git: &source.git,
            role_source_ref: opts.role_branch.as_deref(),
            image_tag: &image,
            docker: DockerResources {
                role_container: container_name.clone(),
                dind_container: dind.clone(),
                network: network.clone(),
                certs_volume: certs_volume.clone(),
            },
        });
        // `read_optional` already separates "manifest absent" (fall back
        // to `new_manifest` and re-record the recovered identity) from
        // "manifest unreadable" (must surface — the operator either
        // repairs the file or purges the recorded state).
        let mut instance_manifest = if restoring {
            match InstanceManifest::read_optional(&container_state).with_context(|| {
                format!(
                    "restoring container `{container_name}`: existing manifest is unreadable; \
                     repair or remove the file, or run `jackin eject {container_name} --purge` to discard the recorded identity"
                )
            }) {
                Ok(Some(existing)) => existing,
                Ok(None) => new_manifest,
                Err(error) => {
                    cleanup.run(docker).await;
                    return Err(error);
                }
            }
        } else {
            new_manifest
        };
        if let Err(error) = super::write_instance_status(
            paths,
            &container_state,
            &mut instance_manifest,
            InstanceStatus::Active,
        ) {
            cleanup.run(docker).await;
            return Err(error);
        }

        // Modes that inject a credential require the well-known env
        // var to resolve to a non-empty value; fail fast with an
        // actionable structured error so the operator sees the
        // problem before we spend time starting the network and DinD
        // sidecar. Sync / Ignore short-circuit inside the helper.
        //
        // Build the per-layer mode-resolution and env-layer traces
        // here (in the caller) so the structured error carries the
        // full picture. The helpers mirror the layers walked by
        // `jackin_config::resolve_mode` and
        // `operator_env::build_attributed_layers` respectively.
        let workspace_name_str = workspace_name.as_deref().unwrap_or("");
        let mode_resolution = super::build_mode_resolution(config, agent, workspace_name_str, &role_key);
        let env_layers = agent
            .required_env_var(auth_mode)
            .map_or_else(Vec::new, |env_var| {
                super::build_env_layer_states(config, workspace_name_str, &role_key, env_var)
            });
        if let Err(error) = verify_credential_env_present(
            agent,
            auth_mode,
            &operator_env,
            &mode_resolution,
            &env_layers,
            workspace_name_str,
            &role_key,
        ) {
            cleanup.run(docker).await;
            return Err(error.into());
        }

        // Resolve the GitHub-auth axis. Layered like the per-agent
        // resolver but with no agent dimension — `.config/gh/` is
        // shared by every agent in the container.
        let github_mode = jackin_config::resolve_github_mode(config, workspace_name_str, &role_key);
        let github_env_decls =
            jackin_config::build_github_env_layers(config, workspace_name_str, &role_key);
        let github_required_env_decls =
            github_env_declarations_for_mode(&github_env_decls, github_mode);
        // Resolve `[…github.env]` only under modes that consume it.
        // `Sync` and `Token` both seed `GH_TOKEN` / `GH_HOST` /
        // `GH_ENTERPRISE_TOKEN` from the resolved map (Token also
        // pre-flight-checks `GH_TOKEN`). `Ignore` exports nothing, so
        // we skip the resolve to avoid unnecessary `op://` shellouts
        // — note this also defers `op://` validation errors under
        // Ignore until the operator flips back to a non-Ignore mode.
        // Other keys in `[github.env]` are not injected anywhere by the
        // runtime; leaving them unresolved keeps unrelated secret refs out of
        // the foreground launch credential graph.
        //
        // Failures are aggregated and surfaced as a structured error
        // so a missing op-CLI doesn't produce N parallel anyhows.
        jackin_diagnostics::active_timing_started("credentials", "github_env", None);
        let github_env_skipped = github_required_env_decls.is_empty();
        let github_resolved_env_result = if github_env_skipped {
            Ok(std::collections::BTreeMap::new())
        } else {
            resolve_github_env_map(&github_required_env_decls, opts)
        };
        let github_resolved_env = match github_resolved_env_result {
            Ok(env) => {
                let detail = if matches!(github_mode, jackin_config::GithubAuthMode::Ignore) {
                    "skipped_ignore".to_owned()
                } else if github_env_skipped {
                    "skipped_no_required_keys".to_owned()
                } else {
                    format!("{} vars", env.len())
                };
                jackin_diagnostics::active_timing_done(
                    "credentials",
                    "github_env",
                    Some(&detail),
                );
                env
            }
            Err(error) => {
                jackin_diagnostics::active_timing_done("credentials", "github_env", Some("error"));
                cleanup.run(docker).await;
                return Err(error);
            }
        };
        let github_ctx = crate::instance::GithubAuthContext {
            mode: github_mode,
            token: github_resolved_env
                .get(jackin_core::env_model::GH_TOKEN_ENV_NAME)
                .cloned(),
        };

        // Token-mode pre-flight: GH_TOKEN must resolve to a non-empty
        // value before we spend time starting DinD.
        if let Err(error) = verify_github_token_present(
            github_mode,
            github_ctx.token.as_deref(),
            workspace_name_str,
            role_key.as_str(),
        ) {
            cleanup.run(docker).await;
            return Err(error);
        }

        // Token/env preflights are complete, so the per-instance sidecar can
        // start while role-state auth is prepared. This preserves fail-fast
        // missing-token behavior but removes the old auth-then-DinD serial wait.
        // DinD startup races role_state_future via tokio::select!; the later
        // join with workspace materialization further overlaps sidecar readiness
        // with mount setup.
        if let Some(progress) = steps.progress_mut() {
            progress.stage_started(
                crate::runtime::progress::LaunchStage::Network,
                "wiring private network",
            );
        }
        let sidecar_container = container_name.clone();
        let sidecar_network = network.clone();
        let sidecar_dind = dind.clone();
        let sidecar_certs_volume = certs_volume.clone();
        let sidecar = async move {
            if adopted_sidecar.is_some() {
                Ok(())
            } else {
                super::run_dind_sidecar_headless(
                    &sidecar_container,
                    &sidecar_network,
                    &sidecar_dind,
                    &sidecar_certs_volume,
                    docker,
                )
                .await
            }
        };
        let mut sidecar = std::pin::pin!(sidecar);
        let mut early_sidecar_result: Option<anyhow::Result<()>> = None;

        // Per-supported-agent mode resolution — each agent in
        // `manifest.supported_agents()` honors its own configured
        // `auth_forward`. Passing the selected agent's mode would wipe
        // sibling agents' durable state when modes diverge.
        //
        // RoleState::prepare is sync and may call `gh` CLI, macOS keychain
        // (`security`), and filesystem copies. Wrap in spawn_blocking so the
        // tokio render thread keeps polling the cockpit rain while auth runs.
        // All inputs are cloned to satisfy the 'static + Send bound.
        jackin_diagnostics::active_timing_started("credentials", "role_state_prepare", None);
        let paths_owned = paths.clone();
        let container_name_owned = container_name.clone();
        let manifest_owned = validated_repo.manifest.clone();
        let config_owned = config.clone();
        let workspace_name_owned = workspace_name_str.to_owned();
        let role_key_owned = role_key.clone();
        let github_ctx_owned = github_ctx.clone();
        let role_state_future = async move {
            tokio::task::spawn_blocking(move || {
                let resolve_mode = |a: jackin_core::agent::Agent| {
                    jackin_config::resolve_mode(
                        &config_owned,
                        a,
                        &workspace_name_owned,
                        &role_key_owned,
                    )
                };
                // Each agent may have an operator-configured sync-source-dir override
                // that replaces host_home for auth sync.
                let resolve_sync_src = |a: jackin_core::agent::Agent| {
                    jackin_config::resolve_sync_source_dir(
                        &config_owned,
                        a,
                        &workspace_name_owned,
                        &role_key_owned,
                    )
                };
                // Provision every supported agent's home/auth state, not just
                // the selected one. The container's per-agent home dirs are
                // bind-mounted once at `docker run`; a later `hardline --new
                // --agent <sibling>` tab reads its auth from that mount, so a
                // sibling whose state was skipped here would start unauthenticated
                // with no way to add the mount after the container is running.
                let provision_agents = manifest_owned.supported_agents();
                RoleState::prepare_for_agents(
                    &paths_owned,
                    &container_name_owned,
                    &manifest_owned,
                    &PrepareResolvers {
                        auth_modes: &resolve_mode,
                        sync_source_dirs: &resolve_sync_src,
                    },
                    &github_ctx_owned,
                    &paths_owned.home_dir,
                    agent,
                    &provision_agents,
                )
            })
            .await
            .map_err(|e| anyhow::anyhow!("RoleState::prepare task panicked: {e}"))?
        };
        let mut role_state_future = std::pin::pin!(role_state_future);
        // Race the overlapped sidecar/auth prep against the cancel token, like
        // every other long-running launch step (cf. `docker build`). Without
        // this, Ctrl+C is ignored for the tens of seconds the blocking auth
        // prep spends in `gh` / the macOS keychain. On cancel, `while_waiting`
        // returns `LaunchCancelled`, which flows into the `Err` arm below and
        // runs `cleanup` — tearing down any already-started sidecar.
        let select_role_state = async {
            tokio::select! {
                result = &mut sidecar => {
                    early_sidecar_result = Some(result);
                    (&mut role_state_future).await
                }
                result = &mut role_state_future => result,
            }
        };
        let role_state_result = if let Some(progress) = steps.progress_mut() {
            progress.while_waiting(select_role_state).await
        } else {
            select_role_state.await
        };
        let (state, _auth_outcome) = match role_state_result {
            Ok(prepared) => {
                jackin_diagnostics::active_timing_done(
                    "credentials",
                    "role_state_prepare",
                    Some("prepared"),
                );
                prepared
            }
            Err(error) => {
                jackin_diagnostics::active_timing_done(
                    "credentials",
                    "role_state_prepare",
                    Some("error"),
                );
                cleanup.run(docker).await;
                return Err(error);
            }
        };
        emit_auth_provision_launch_plan(&state, &container_name);
        // The sidecar (adopted or freshly started above) is now running, so a
        // bare `?` here would leak the container/network/volume. Route trust
        // seeding through cleanup like the role-state and sidecar arms.
        if let Err(error) = seed_codex_project_trust(&state, workspace) {
            cleanup.run(docker).await;
            return Err(error);
        }

        if agent != jackin_core::agent::Agent::Codex {
            let _expiry_days = workspace_name
                .as_deref()
                .filter(|_| auth_mode == jackin_config::AuthForwardMode::OAuthToken)
                .and_then(|ws| {
                    match jackin_env::expiry_days_for_launch(paths, ws) {
                        Ok(days) => days,
                        Err(e) => {
                            let message = format!(
                                "[jackin] note: token expiry cache for workspace {ws:?} \
                                 is unreadable ({e}); re-run \
                                 `jackin workspace claude-token setup {ws}` to refresh."
                            );
                            if let Some(run) = jackin_diagnostics::active_run() {
                                run.compact("auth", &message);
                            }
                            None
                        }
                    }
                });
        }
        if let Some(run) = jackin_diagnostics::active_run() {
            run.compact("auth", &format!("{agent} auth resolved via {auth_mode}"));
        }

        // GitHub auth summary line — agent-neutral. The breadcrumb walks
        // the [github.env] layers (NOT the regular operator-env tree)
        // because the proposal documents [github.env] as the canonical
        // place for GH_TOKEN. Falling back to lookup_operator_env_raw
        // would render bare "GH_TOKEN" when the operator follows the
        // docs.
        {
            let gh_token_key = jackin_core::env_model::GH_TOKEN_ENV_NAME;
            if let Some(run) = jackin_diagnostics::active_run() {
                if matches!(github_mode, jackin_config::GithubAuthMode::Ignore) {
                    run.compact("github_auth", "GitHub auth ignored by auth_forward=ignore");
                } else {
                    let token_breadcrumb = github_env_decls.get(gh_token_key).map_or_else(
                        || gh_token_key.to_owned(),
                        |value| {
                            super::auth_token_source_reference(
                                gh_token_key,
                                Some(value.as_display_str()),
                            )
                        },
                    );
                    run.compact(
                        "github_auth",
                        &format!("resolved GitHub auth from {token_breadcrumb}"),
                    );
                }
            }
        }

        // Materialize workspace mounts while the already-started
        // Docker-in-Docker sidecar finishes becoming ready. The sidecar path
        // uses DockerApi only, and workspace materialization is still the only
        // side that needs the mutable CommandRunner seam. Shared mounts pass through;
        // worktree-isolated mounts get a per-container `git worktree`
        // staged on the host. Must run AFTER `RoleState::prepare` (so the
        // per-container state directory exists) and BEFORE the docker run
        // command is assembled (so the docker `-v` flags reflect the
        // per-mount bind sources).
        let interactive = true;
        let workspace_label = workspace.label.as_str();
        jackin_diagnostics::debug_log!(
            "isolation",
            "load_role: invoking materialize_workspace for container {container_name} (interactive={interactive}, force={force})",
            force = opts.force,
        );
        if let Some(progress) = steps.progress_mut() {
            progress.stage_started(
                crate::runtime::progress::LaunchStage::Workspace,
                "materializing workspace",
            );
        }
        let materialize_preflight = crate::isolation::materialize::PreflightContext {
            workspace_name: workspace_label.to_owned(),
            force: opts.force,
            interactive,
        };
        let materialize = crate::isolation::materialize::materialize_workspace(
            workspace,
            &container_state,
            &role_key,
            &container_name,
            workspace_label,
            &materialize_preflight,
            runner,
        );
        jackin_diagnostics::active_timing_started("workspace", "materialize_workspace", None);
        let materialize_wait = async {
            if let Some(progress) = steps.progress_mut() {
                progress.while_waiting(materialize).await
            } else {
                materialize.await
            }
        };
        let sidecar_wait = async {
            if let Some(result) = early_sidecar_result {
                result
            } else {
                (&mut sidecar).await
            }
        };
        // TODO(launch-worktree-leak-on-sidecar-fail): `join!` runs
        // materialization to completion even if the sidecar already failed, so
        // a worktree-isolated mount can leave a staged worktree that
        // `LoadCleanup` does not unstage. See TODO.md "Follow-ups".
        let (sidecar_result, materialize_result) = tokio::join!(sidecar_wait, materialize_wait);
        drop(sidecar);
        if let Some(progress) = steps.progress_mut() {
            progress.stage_done(crate::runtime::progress::LaunchStage::Network, "isolated");
        }
        if let Err(error) = sidecar_result {
            if let Err(status_err) = super::write_instance_status(
                paths,
                &container_state,
                &mut instance_manifest,
                InstanceStatus::FailedSetup,
            ) {
                let message = format!(
                    "jackin: warning: failed to mark FailedSetup for {container_name} \
                     after sidecar error: {status_err:#}; on-disk status may be stale"
                );
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("status", &message);
                }
            }
            cleanup.run(docker).await;
            return Err(error);
        }
        let materialized = match materialize_result {
            Ok(materialized) => {
                jackin_diagnostics::active_timing_done(
                    "workspace",
                    "materialize_workspace",
                    Some("materialized"),
                );
                materialized
            }
            Err(error) => {
                jackin_diagnostics::active_timing_done(
                    "workspace",
                    "materialize_workspace",
                    Some("error"),
                );
                if let Err(status_err) = super::write_instance_status(
                    paths,
                    &container_state,
                    &mut instance_manifest,
                    InstanceStatus::FailedSetup,
                ) {
                    let message = format!(
                        "jackin: warning: failed to mark FailedSetup for {container_name} \
                         after workspace materialization error: {status_err:#}; on-disk status may be stale"
                    );
                    if let Some(run) = jackin_diagnostics::active_run() {
                        run.compact("status", &message);
                    }
                }
                cleanup.run(docker).await;
                return Err(error);
            }
        };
        if let Some(progress) = steps.progress_mut() {
            progress.stage_done(crate::runtime::progress::LaunchStage::Workspace, "materialized");
        }

        let mut launch_config = super::capsule_config(
            selector,
            &workspace.workdir,
            &validated_repo.manifest,
            opts.initial_provider(),
        );
        // Carry the on-demand credential bindings to the host resolver, which
        // the launch path starts once the per-container socket dir exists.
        launch_config.exec_bindings = exec_bindings;

        // Backend dispatch. A per-workspace `[runtime].backend` or the host
        // `[runtime].default_backend` routes this launch to the apple-container
        // backend instead of Docker. Everything above (role resolution, image
        // build, env resolution, mount materialization, capsule config) is
        // backend-neutral; only the container lifecycle below is Docker-specific.
        //
        // The apple-container VM boots its own kernel and runs rootless DinD
        // inside, so the Docker DinD sidecar / private network / certs volume
        // provisioned by the shared path above are unused here — tear them down
        // before handing off so they do not leak. (The empirical Phase 0 gate —
        // see the apple-container roadmap item — moves this branch ahead of the
        // sidecar so it is never started; it cannot be validated without macOS
        // 26 ARM hardware, so for now the sidecar is started and immediately
        // reclaimed.)
        if super::resolve_backend(config, workspace_name.as_deref())
            == crate::apple_container_client::BACKEND_NAME
        {
            cleanup.run(docker).await;
            let mount_pairs = super::build_workspace_mount_pairs(&materialized);
            return crate::runtime::apple_container::launch(
                crate::runtime::apple_container::AppleContainerLaunch {
                    paths,
                    container_name: &container_name,
                    image: &image,
                    workspace_name: workspace_name.as_deref(),
                    workspace_label: workspace.label.as_str(),
                    workdir: &workspace.workdir,
                    role_key: &role_key,
                    role_display_name: &agent_display_name,
                    agent,
                    role_source_git: &source.git,
                    role_source_ref: opts.role_branch.as_deref(),
                    image_tag: &image,
                    env_pairs: &resolved_env.vars,
                    mount_pairs: &mount_pairs,
                    host_workdir_fingerprint: &host_workdir_fingerprint,
                    capsule_config: &launch_config,
                    debug: opts.debug,
                },
            )
            .await
            .map(|()| container_name.clone());
        }

        let ctx = super::LaunchContext {
            container_name: &container_name,
            image: &image,
            network: &network,
            dind: &dind,
            selector,
            agent_display_name: &agent_display_name,
            workspace: &materialized,
            state: &state,
            git: &git,
            debug: opts.debug,
            git_coauthor_trailer: config.git.coauthor_trailer,
            git_dco: config.git.dco,
            agent,
            capsule_config: &launch_config,
            resolved_env: &resolved_env,
            github_env: &github_resolved_env,
            paths,
            selected_image_refresh: selected_refresh_reason.map(|reason| super::SelectedImageRefresh {
                role_git: &source.git,
                branch_override: opts.role_branch.as_deref(),
                reason,
            }),
            sibling_prewarm: super::SiblingPrewarm {
                role_git: &source.git,
                branch_override: opts.role_branch.as_deref(),
                validated_repo: &validated_repo,
                selected_image_reused,
            },
            sibling_auth_prewarm: super::SiblingAuthPrewarm {
                manifest: &validated_repo.manifest,
                config,
                workspace_name: workspace_name_str,
                role_key: &role_key,
            },
        };
        let launch_result = super::launch_role_runtime(&ctx, &mut steps, docker, runner).await;
        if launch_result.is_err() {
            // FailedSetup write error must not abort cleanup; surface to stderr
            // so the operator sees the on-disk status is stale (Active) and
            // that `jackin inspect` / `hardline` may report misleading state.
            if let Err(status_err) = super::write_instance_status(
                paths,
                &container_state,
                &mut instance_manifest,
                InstanceStatus::FailedSetup,
            ) {
                let message = format!(
                    "jackin: warning: failed to mark FailedSetup for {container_name} \
                     after launch error: {status_err:#}; on-disk status may be stale"
                );
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("status", &message);
                }
            }
            cleanup.run(docker).await;
        }
        launch_result?;
        // Launch succeeded. From here on the cleanup struct is reused
        // to tear down docker resources at session end (clean exit,
        // crash, NotFound, etc.); the host-side socket dir + Capsule
        // launch config stay behind for operator inspection and get
        // swept by the next explicit `jackin eject` / Purge.
        cleanup.keep_socket_dir();
        super::write_instance_status(
            paths,
            &container_state,
            &mut instance_manifest,
            InstanceStatus::Running,
        )?;

        // Finalize per-mount isolation worktrees BEFORE the container teardown
        // decision below: clean exits without dirty/unpushed state get their
        // worktrees swept; dirty state is preserved through the rich cleanup
        // dialog. A `ReturnToAgent` choice restarts + re-attaches the container
        // exactly once so the operator can address the dirty state inside the
        // role, then the safe cleanup is retried.
        let interactive_finalize = true;
        let mut prompt = crate::isolation::finalize::RichCleanupPrompt;
        let outcome = super::inspect_attach_outcome(docker, &container_name).await?;
        super::write_instance_attach_outcome(paths, &container_state, &mut instance_manifest, outcome)?;
        let mut decision = crate::isolation::finalize::finalize_foreground_session(
            &container_name,
            &paths.data_dir.join(&container_name),
            outcome,
            interactive_finalize,
            &mut prompt,
            docker,
            runner,
        ).await?;
        super::write_preserved_status_if_applicable(
            decision,
            paths,
            &container_state,
            &mut instance_manifest,
        )?;
        if matches!(
            decision,
            crate::isolation::finalize::FinalizeDecision::ReturnToAgent
        ) {
            // Restart detached, then attach through the jackin-capsule client
            // socket. Attaching `docker start -ai` to PID 1 would only show
            // daemon logs, not the multiplexer UI the operator needs to fix
            // the preserved worktree. We do not loop further: if the operator
            // still leaves dirty state, the second pass will fall back to
            // Preserved and exit normally.
            start_or_reconnect_capsule_client(paths, &container_name, docker, runner).await?;
            let outcome2 = super::inspect_attach_outcome(docker, &container_name).await?;
            super::write_instance_attach_outcome(
                paths,
                &container_state,
                &mut instance_manifest,
                outcome2,
            )?;
            decision = crate::isolation::finalize::finalize_foreground_session(
                &container_name,
                &paths.data_dir.join(&container_name),
                outcome2,
                interactive_finalize,
                &mut prompt,
                docker,
                runner,
            ).await?;
            super::write_preserved_status_if_applicable(
                decision,
                paths,
                &container_state,
                &mut instance_manifest,
            )?;
        }

        // Classify how the interactive session ended and tear down DinD/network
        // unless the container is still running with active sessions (detach):
        //  - Running + active sessions → user detached (Ctrl-B D). Keep DinD so
        //                               `jackin hardline` can reconnect.
        //  - Running + no sessions → agent exited; Capsule cleanup lag or stale socket.
        //                            Tear down same as Stopped/0 regardless of
        //                            preserved isolation state — worktrees live on
        //                            the host and are accessible without DinD.
        //  - Stopped / 0 → user exited cleanly. Tear down.
        //  - Stopped / ≠0 or OOM-killed → crash. Tear down; DinD is no longer
        //                                  needed once the container has exited.
        //  - NotFound + Preserved → removed externally during finalization.
        //                           Tear down DinD/network; status on disk stands.
        //  - NotFound → removed externally. Tear down.
        //  - InspectUnavailable → Docker unreachable; keep everything alive.
        let is_preserved = matches!(
            decision,
            crate::isolation::finalize::FinalizeDecision::Preserved
        );
        #[allow(clippy::match_same_arms)]
        match docker.inspect_container_state(&container_name).await {
            ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
                if is_preserved {
                    // Finalize saw sessions at check-time (detach). Re-check: sessions
                    // may have ended in the interval between finalize and this inspect.
                    let sessions =
                        inspect_agent_sessions(docker, &container_name, &ContainerState::Running).await;
                    if let AgentSessionInventory::Unavailable(ref reason) = sessions {
                        jackin_diagnostics::debug_log!(
                            "instance",
                            "inspect_agent_sessions unavailable for {container_name}: {reason}; \
                             treating conservatively as sessions-present (container preserved)",
                        );
                    }
                    let no_sessions =
                        matches!(&sessions, AgentSessionInventory::Sessions(v) if v.is_empty());
                    if no_sessions {
                        super::write_instance_status(
                            paths,
                            &container_state,
                            &mut instance_manifest,
                            InstanceStatus::CleanExited,
                        )?;
                        cleanup.run(docker).await;
                    } else {
                        cleanup.disarm();
                    }
                } else {
                    // Finalize already confirmed no sessions (Capsule still running after
                    // clean exit). Skip the redundant re-query and tear down.
                    super::write_instance_status(
                        paths,
                        &container_state,
                        &mut instance_manifest,
                        InstanceStatus::CleanExited,
                    )?;
                    cleanup.run(docker).await;
                }
            }
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } if is_preserved => {
                cleanup.run(docker).await;
            }
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } => {
                super::write_instance_status(
                    paths,
                    &container_state,
                    &mut instance_manifest,
                    InstanceStatus::CleanExited,
                )?;
                cleanup.run(docker).await;
            }
            ContainerState::Stopped { .. }
            | ContainerState::Created
            | ContainerState::Removing
            | ContainerState::Dead => {
                super::write_instance_status(
                    paths,
                    &container_state,
                    &mut instance_manifest,
                    InstanceStatus::Crashed,
                )?;
                cleanup.run(docker).await;
            }
            ContainerState::InspectUnavailable(reason) => {
                cleanup.disarm();
                anyhow::bail!(
                    "{}",
                    crate::runtime::attach::docker_unavailable_msg(
                        &format!("inspect container `{container_name}` after the session"),
                        &reason,
                    )
                );
            }
            ContainerState::NotFound if is_preserved => {
                jackin_diagnostics::debug_log!(
                    "instance",
                    "container {container_name} not found after session with Preserved decision; \
                     removed externally during finalization — tearing down DinD/network, \
                     preserved status on disk stands",
                );
                cleanup.run(docker).await;
            }
            ContainerState::NotFound => {
                super::write_instance_status(
                    paths,
                    &container_state,
                    &mut instance_manifest,
                    InstanceStatus::CleanExited,
                )?;
                cleanup.run(docker).await;
            }
        }

        Ok(container_name)
    }.await;

    match load_result {
        Ok(_) => {
            super::render_exit(paths, docker).await;
            Ok(())
        }
        Err(error) => {
            let failed_stage = steps
                .current_stage
                .unwrap_or(crate::runtime::progress::LaunchStage::Capsule);
            let run = jackin_diagnostics::active_run();
            let final_error = super::launch_failure_cli_error(failed_stage, &error, run.as_deref());
            if let Some(progress) = steps.progress_mut() {
                progress
                    .stage_failed(crate::runtime::progress::LaunchFailure {
                        title: super::launch_failure_title(failed_stage, &error, run.as_deref()),
                        summary: super::short_launch_diagnosis(
                            failed_stage,
                            &error,
                            run.as_deref(),
                        ),
                        detail: Some(format!("{error:#}")),
                        next_step: None,
                        stage: failed_stage,
                        diagnostics_path: None,
                        command_output_path: None,
                    })
                    .await;
            }
            // Stop the cockpit render task and release the rich surface before
            // the exit warp writes to the terminal. A pre-attach failure returns
            // before the success path's pre-handoff teardown runs, so without
            // this the background task keeps drawing frames over the warp.
            steps.finish_progress();
            super::render_exit(paths, docker).await;
            Err(final_error)
        }
    }
}

pub(crate) fn emit_auth_provision_launch_plan(state: &RoleState, container: &str) {
    if state.auth_outcomes.is_empty() {
        return;
    }
    let outcomes = state
        .auth_outcomes
        .iter()
        .map(|(agent, outcome)| (agent.slug(), outcome.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let detail = serde_json::json!({
        "plan": "AuthProvision",
        "reason": "credential_outcomes",
        "container": container,
        "agents": outcomes,
    })
    .to_string();
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "launch_plan",
            "credentials",
            "agent credential outcomes",
            Some(&detail),
        );
    }
}

pub(crate) fn manifest_env_timing_detail(skipped: bool, vars: usize) -> String {
    if skipped {
        "skipped".to_owned()
    } else {
        format!("{vars} vars")
    }
}

/// Whether a credential env key must be resolved before launch.
///
/// Resolves a key when it is a generic operator/manifest variable, or when any
/// agent the role can run could read it from the container env in one of its
/// auth modes. The container env is set once at `docker run` and shared by
/// every agent tab; an `ApiKey`/`OAuthToken` tab reads its key from that env,
/// so a credential needed by *any* supported agent is resolved up front rather
/// than gated to the initially-selected agent — otherwise switching tabs would
/// start that agent without its key. Only credentials belonging solely to
/// agents this role cannot launch are skipped. Used for both operator-env and
/// manifest-env refs so the two call sites cannot drift.
pub(crate) fn credential_key_needed_for_role(
    supported_agents: &[jackin_core::agent::Agent],
    key: &str,
) -> bool {
    if !known_agent_credential_env(key) {
        return true;
    }
    supported_agents.iter().copied().any(|agent| {
        agent
            .supported_modes()
            .iter()
            .filter_map(|mode| agent.required_env_var(*mode))
            .any(|credential_key| credential_key == key)
    })
}

fn known_agent_credential_env(key: &str) -> bool {
    jackin_core::agent::Agent::ALL
        .iter()
        .copied()
        .flat_map(|agent| {
            agent
                .supported_modes()
                .iter()
                .filter_map(move |mode| agent.required_env_var(*mode))
        })
        .any(|credential_key| credential_key == key)
}
