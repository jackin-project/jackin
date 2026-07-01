//! Role load pipeline: public entry points and the full launch-to-attach sequence.

use crate::instance::{InstanceManifest, InstanceStatus, RoleState};
use jackin_config::AppConfig;
use jackin_core::CommandRunner;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_docker::docker_client::DockerApi;

use super::launch_slot::{claim_container_name, claim_known_container_name};
use super::trust::inject_workspace_mise_env;
use crate::runtime::attach::{ContainerState, hardline_agent, start_or_hardline_agent};
use crate::runtime::naming::{image_name, image_name_for_branch};
use crate::runtime::repo_cache::{RepoResolveOptions, resolve_agent_repo_with};

mod launch_core;

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
/// Prefix each validation error with its source tag (`config`/`workspace`/
/// `role`/`merged`) for the operator-facing message.
pub(super) fn tag_errors<E: std::fmt::Display>(tag: &str, errors: Vec<E>) -> Vec<String> {
    errors
        .into_iter()
        .map(|error| format!("  - [{tag}] {error}"))
        .collect()
}

/// Validate one source's docker grants, tagged for the operator-facing message.
pub(super) fn tagged_grant_errors(
    tag: &str,
    grants: &crate::runtime::docker_profile::DockerGrants,
) -> Vec<String> {
    tag_errors(tag, crate::runtime::docker_profile::validate_grants(grants))
}

/// Bail with the standard "docker grants validation failed" message when any
/// tagged errors were collected; no-op otherwise.
pub(super) fn bail_on_grant_errors(errors: Vec<String>) -> anyhow::Result<()> {
    if errors.is_empty() {
        return Ok(());
    }
    anyhow::bail!("docker grants validation failed:\n{}", errors.join("\n"))
}

#[tracing::instrument(
    skip_all,
    fields(role = %selector.key())
)]
#[allow(
    clippy::too_many_lines,
    reason = "Top-level launch pipeline that drives run_launch_core with preflight \
              validation, image-materialization, env resolution, and post-launch \
              cleanup. Body extraction follows the same deferred-parallel-pass \
              plan as launch_role_runtime + run_launch_core — helpers \
              `prepare_launch_inputs`, `materialize_role_image`, `resolve_launch_\
              env`, `invoke_run_launch_core`, and `cleanup_post_launch` to land \
              in a follow-up slice. Until that slice lands, the inline shape \
              preserves captured-locals across phases."
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
        .contains_key(workspace.name.as_str())
        .then(|| workspace.name.clone());

    let mut steps = super::StepCounter::new(&selector.name);
    if let Some(run) = jackin_diagnostics::active_run() {
        #[cfg(test)]
        let mut progress = crate::runtime::progress::LaunchProgress::for_test(run);
        #[cfg(not(test))]
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

    // Pinned role SHA from the stored launch recipe (D7/Tier 3).  Set in the
    // `RecreateCurrentRole` arm below when the manifest has a recorded SHA.
    let mut restore_pinned_sha: Option<String> = None;
    let restore_container = if early_restore_container.is_some() {
        early_restore_container
    } else if let Some(container) = opts.restore_container_base.as_ref() {
        Some(container.clone())
    } else if opts.rebuild {
        // `--rebuild` skips the early gate above (it is `&& !opts.rebuild`), so
        // a forced rebuild actually falls through to *this* resolution. Without
        // the same guard here, `resolve_restore_candidate` would still return
        // `StartCurrentRole`/`RecreateCurrentRole` and `return` straight into the
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
                // D7: extract pinned recipe so Tier 3 rebuild uses the original
                // role SHA rather than current HEAD of the cached repo.
                let container_state = paths.data_dir.join(&container);
                if let Ok(Some(stored)) = InstanceManifest::read_optional(&container_state) {
                    restore_pinned_sha = stored.role_git_sha;
                }
                Some(container)
            }
            super::RestoreResolution::RestoreCurrentRole(container) => {
                // D7: same pinned-SHA extraction as RecreateCurrentRole.
                let container_state = paths.data_dir.join(&container);
                if let Ok(Some(stored)) = InstanceManifest::read_optional(&container_state) {
                    restore_pinned_sha = stored.role_git_sha;
                }
                Some(container)
            }
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
            // D21: operator deleted a candidate from the launch dialog.
            // Purge its state then fall through to StartFresh (None).
            super::RestoreResolution::PurgeAndRestartFresh(container) => {
                // Best-effort: a failed purge leaves stale state the next prune
                // reaps, but trace it so a delete-then-launch that didn't clean
                // up is diagnosable rather than silent.
                if let Err(err) = crate::runtime::cleanup::purge_container_state(
                    paths, &container, docker, runner,
                )
                .await
                {
                    jackin_diagnostics::debug_log!(
                        "instance",
                        "purge after launch-dialog delete failed for {container}: {err}; \
                         state will be removed on next prune",
                    );
                }
                None
            }
        }
    };

    // D7: skip git pull when restoring — restore replays the pinned recipe;
    // pulling would advance the role repo past the pinned SHA.
    if restore_container.is_none() && workspace.git_pull_on_entry {
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
    let repo_lock = Some(repo_lock);
    let image_decision = crate::runtime::image::decide_role_image(
        paths,
        selector,
        &cached_repo,
        &validated_repo,
        rebuild,
        opts.role_branch.as_deref(),
        restore_pinned_sha.as_deref(),
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
    let credential_agents = supported_agents.clone();
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
    let exec_bindings = jackin_env::collect_on_demand_bindings(
        config,
        Some(role_key.as_str()),
        workspace_name.as_deref(),
    );
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

    // D7: capture recipe fields before image_decision is consumed by match below.
    let recipe_role_git_sha = image_decision.role_git_sha();
    let recipe_base_image_ref = image_decision.base_image_ref().map(ToOwned::to_owned);

    let selected_refresh_reason = match &image_decision {
        crate::runtime::image::ImageDecision::RefreshInBackground { reason, .. } => Some(*reason),
        crate::runtime::image::ImageDecision::Reuse { .. }
        | crate::runtime::image::ImageDecision::BuildFromPublished { .. }
        | crate::runtime::image::ImageDecision::BuildFromWorkspace { .. } => None,
    };

    // Validate the selected backend up front, before any container-lifecycle
    // work (DinD sidecar, network, certs) runs — a config typo fails closed
    // here instead of after paying for Docker provisioning. `Backend` is `Copy`,
    // so the dispatch below reads the cached value.
    let backend = super::resolve_backend(config, workspace_name.as_deref())?;

    let load_result: anyhow::Result<String> =
        launch_core::run_launch_core(launch_core::LaunchCore {
            paths,
            config,
            selector,
            workspace,
            docker,
            runner,
            opts,
            git,
            workspace_name,
            steps: &mut steps,
            role_key,
            agent_display_name,
            agent,
            supported_agents,
            cached_repo,
            validated_repo,
            source,
            auth_mode,
            backend,
            image_decision,
            repo_lock,
            restoring,
            container_name,
            exec_bindings,
            recipe_role_git_sha,
            recipe_base_image_ref,
            selected_refresh_reason,
            resolved_env,
            rebuild,
            restore_pinned_sha,
            operator_env,
        })
        .await;

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

/// D9: purge per-instance data, the name-claim lock, and the index row inline on
/// a clean terminal outcome so no manual prune is needed. If the purge itself
/// fails, fall back to stamping `CleanExited` so the next prune removes the row.
/// Shared by the clean-exit and `NotFound` arms, which differ only in `context`.
pub(super) async fn purge_or_mark_clean_exited(
    paths: &JackinPaths,
    container_name: &str,
    state_dir: &std::path::Path,
    manifest: &mut InstanceManifest,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    context: &str,
) -> anyhow::Result<()> {
    if let Err(err) =
        crate::runtime::cleanup::purge_container_state(paths, container_name, docker, runner).await
    {
        jackin_diagnostics::debug_log!(
            "instance",
            "inline cleanup after {context} failed for {container_name}: {err}; \
             state will be removed on next prune",
        );
        super::write_instance_status(paths, state_dir, manifest, InstanceStatus::CleanExited)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
