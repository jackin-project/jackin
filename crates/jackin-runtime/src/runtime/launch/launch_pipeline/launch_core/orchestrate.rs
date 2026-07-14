// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Phase chain body for `run_launch_core` (typed `#[must_use]` handoffs).

use super::super::launch_phases::{
    CleanupClassified, EnvironmentResolved, GrantsValidated, ImageMaterialized,
    ImagePhaseClassified, InstancePrepared, RuntimeLaunched, SessionFinalized, TrustSeeded,
    WorkspaceMaterialized,
};
use super::super::{emit_auth_provision_launch_plan, purge_or_mark_clean_exited};
use super::LaunchCore;
use jackin_core::CommandRunner;
use jackin_core::WorkspaceName;
use jackin_docker::docker_client::DockerApi;

use anyhow::Context;
use std::future::Future;
use std::pin::Pin;

use super::super::super::trust::seed_codex_project_trust;
use crate::instance::{
    DockerResources, InstanceManifest, InstanceStatus, NewInstanceManifest, PrepareResolvers,
    RoleState,
};
use crate::runtime::attach::{
    AgentSessionInventory, ContainerState, inspect_agent_sessions,
    start_or_reconnect_capsule_client,
};

use super::super::super::launch_slot::{
    github_env_declarations_for_mode, resolve_github_env_map, verify_credential_env_present,
    verify_github_token_present,
};

async fn poll_sidecar_while<T, F, S>(
    work: F,
    mut sidecar: Pin<&mut S>,
    early_sidecar_result: &mut Option<anyhow::Result<()>>,
) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
    S: Future<Output = anyhow::Result<()>>,
{
    if early_sidecar_result.is_some() {
        return work.await;
    }

    let mut work = std::pin::pin!(work);
    tokio::select! {
        biased;
        result = sidecar.as_mut() => {
            *early_sidecar_result = Some(result);
            work.await
        }
        result = &mut work => result,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Phase chain body: individual phases are typed tokens; further \
              per-phase file split is follow-up once harness coverage is green."
)]
#[expect(
    clippy::cognitive_complexity,
    reason = "Branching tracks the ten launch phases; typed handoffs already \
              mark phase boundaries for the compiler."
)]
pub(super) async fn run_launch_phases<D, R>(ctx: LaunchCore<'_, D, R>) -> anyhow::Result<String>
where
    D: DockerApi,
    R: CommandRunner,
{
    // Destructure captured names so verbatim original block statements work.
    let LaunchCore {
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        opts,
        git,
        workspace_name,
        mut steps,
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
        mut repo_lock,
        restoring,
        container_name,
        exec_bindings,
        recipe_role_git_sha,
        recipe_base_image_ref,
        selected_refresh_reason,
        resolved_env,
        rebuild,
        restore_pinned_sha: _,
        operator_env,
        git_pull_join,
        ..
    } = ctx;
    let container_state = paths.data_dir.join(&container_name);
    let adopted_sidecar = super::super::super::adopt_prewarmed_dind_sidecar(paths, docker).await;
    let adopted_sidecar_was_used = adopted_sidecar.is_some();
    let resources = adopted_sidecar.as_ref().map_or_else(
        || DockerResources::from_container_name(&container_name),
        |sidecar| DockerResources {
            role_container: container_name.clone(),
            dind_container: Some(sidecar.sidecar.dind.clone()),
            network: sidecar.sidecar.network.clone(),
            certs_volume: Some(sidecar.sidecar.certs_volume.clone()),
        },
    );
    let network = resources.network.clone();
    // Adoption-aware: when a prewarmed sidecar was adopted, the role connects
    // to (and teardown must remove) the adopted DinD container, not the
    // role-default name. `resources.dind_container` is always `Some` — set
    // from the adopted sidecar or `from_container_name`.
    let dind = resources
        .dind_container
        .clone()
        .unwrap_or_else(|| crate::instance::naming::dind_container_name(&container_name));
    let certs_volume = resources
        .certs_volume
        .clone()
        .unwrap_or_else(|| crate::instance::naming::dind_certs_volume(&container_name));
    // Arm cleanup immediately after adoption, before grant validation.
    // When a prewarmed DinD sidecar was adopted, its container, network,
    // and certs volume are already *running* and the on-disk prewarm state
    // was deleted (`adopt_prewarmed_dind_sidecar` calls
    // `remove_prewarmed_dind_state`), so nothing re-adopts them. Any early
    // `?`/`return Err` between here and the start of the launch proper
    // would otherwise orphan a live privileged container with no record.
    // `LoadCleanup::run` is best-effort: removing the not-yet-created role
    // container is a no-op. For a fresh launch the sidecar is not started
    // until later, so there is nothing to leak in the gap.
    let socket_dir = paths.jackin_home.join("sockets").join(&container_name);
    let mut cleanup = super::super::super::LoadCleanup::new(
        container_name.clone(),
        dind.clone(),
        certs_volume.clone(),
        network.clone(),
        socket_dir,
    );
    // Phase: grants validated (typestate). Failure → cleanup only (suite A).
    let grants_validated: GrantsValidated =
        match super::super::launch_phases::validate_launch_grants(
            super::super::launch_phases::GrantPhaseInput {
                config,
                workspace_label: workspace.label.as_str(),
                workspace_docker: None,
                opts_docker_profile: opts.docker_profile,
                selector,
                role_manifest: &validated_repo.manifest,
            },
        ) {
            Ok(validated) => validated,
            Err(error) => {
                super::super::launch_phases::cleanup_after_grant_failure(&cleanup, docker).await;
                return Err(error);
            }
        };
    let effective_grants = grants_validated.effective_grants;
    let resolved_profile = (
        grants_validated.resolved_profile,
        grants_validated.profile_source,
    );
    let dind_started = grants_validated.dind_started;
    // Phase: image decision classified (typestate; pure, no Docker I/O).
    let image_phase: ImagePhaseClassified =
        super::super::launch_phases::classify_image_phase(&image_decision);
    // Start the sidecar future before image materialization so network/DinD
    // setup can make progress while runtime binaries and Docker build run.
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
    let sidecar_dind_grant = effective_grants.dind;
    let sidecar_network_disabled =
        crate::runtime::docker_profile::network_disabled(&effective_grants);
    let role_network_internal =
        crate::runtime::docker_profile::role_network_internal(resolved_profile.0);
    let sidecar = async move {
        if adopted_sidecar.is_some() {
            Ok(())
        } else if dind_started {
            super::super::super::run_dind_sidecar_headless(
                &sidecar_container,
                &sidecar_network,
                &sidecar_dind,
                &sidecar_certs_volume,
                sidecar_dind_grant,
                docker,
            )
            .await
        } else if sidecar_network_disabled {
            Ok(())
        } else {
            super::super::super::create_role_network(
                &sidecar_container,
                &sidecar_network,
                role_network_internal,
                docker,
            )
            .await
        }
    };
    let mut sidecar = std::pin::pin!(sidecar);
    let mut early_sidecar_result: Option<anyhow::Result<()>> = None;

    // Step 2: Prepare runtime assets and build the derived image when the
    // earlier image decision proved the local recipe is missing/stale.
    let (image, selected_image_reused) = match (image_phase.class, image_decision) {
        (
            super::super::launch_phases::ImagePhaseClass::ReuseOrBackgroundRefresh,
            decision @ (crate::runtime::image::ImageDecision::Reuse { .. }
            | crate::runtime::image::ImageDecision::RefreshInBackground { .. }),
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
            super::super::super::emit_image_materialization_plan(
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
            debug_assert!(image_phase.selected_image_reused);
            (image, true)
        }
        (
            super::super::launch_phases::ImagePhaseClass::BuildRequired,
            build_decision @ (crate::runtime::image::ImageDecision::BuildFromPublished { .. }
            | crate::runtime::image::ImageDecision::BuildFromWorkspace { .. }),
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
            super::super::super::emit_image_materialization_plan(
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
            let image_agents = supported_agents.clone();
            let runtime_binaries_result = poll_sidecar_while(
                async {
                    if let Some(progress) = steps.progress_mut() {
                        crate::runtime::image::prepare_runtime_binaries_for_agents(
                            paths,
                            &validated_repo,
                            &image_agents,
                            Some(progress),
                        )
                        .await
                    } else {
                        crate::runtime::image::prepare_runtime_binaries_for_agents(
                            paths,
                            &validated_repo,
                            &image_agents,
                            None,
                        )
                        .await
                    }
                },
                sidecar.as_mut(),
                &mut early_sidecar_result,
            )
            .await;
            let runtime_binaries = match runtime_binaries_result {
                Ok(runtime_binaries) => runtime_binaries,
                Err(error) => {
                    cleanup.run(docker).await;
                    return Err(error);
                }
            };
            steps.next("Preparing derived image").await?;
            let Some(repo_lock) = repo_lock.take() else {
                cleanup.run(docker).await;
                return Err(anyhow::anyhow!("repo lock already consumed"));
            };
            let image_result = poll_sidecar_while(
                async {
                    if let Some(progress) = steps.progress_mut() {
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
                        .await
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
                        .await
                    }
                },
                sidecar.as_mut(),
                &mut early_sidecar_result,
            )
            .await;
            let image = match image_result {
                Ok(image) => image,
                Err(error) => {
                    cleanup.run(docker).await;
                    return Err(error);
                }
            };
            debug_assert!(!image_phase.selected_image_reused);
            (image, false)
        }
        _ => {
            // Class and decision variants must stay in lock-step.
            cleanup.run(docker).await;
            return Err(anyhow::anyhow!(
                "internal: image phase class does not match ImageDecision variant"
            ));
        }
    };
    let image_mat: ImageMaterialized = ImageMaterialized {
        image,
        selected_image_reused,
    };
    let ImageMaterialized {
        image,
        selected_image_reused,
    } = image_mat;

    let host_workdir_fingerprint =
        super::super::super::manifest_host_workdir_fingerprint(workspace);
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
            dind_container: dind_started.then(|| dind.clone()),
            network: network.clone(),
            certs_volume: dind_started.then(|| certs_volume.clone()),
        },
        // D7: pin the launch recipe for faithful restore.
        role_git_sha: recipe_role_git_sha,
        base_image_ref: recipe_base_image_ref,
        base_image_digest: None, // D16: populated when Docker reports digest post-build
        supported_agents: supported_agents.clone(),
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
    if let Err(error) = super::super::super::write_instance_status(
        paths,
        &container_state,
        &mut instance_manifest,
        InstanceStatus::Active,
    ) {
        cleanup.run(docker).await;
        return Err(error);
    }

    let prepared: InstancePrepared = InstancePrepared {
        instance_manifest,
        container_state: container_state.clone(),
        host_workdir_fingerprint: host_workdir_fingerprint.clone(),
    };
    let InstancePrepared {
        mut instance_manifest,
        container_state,
        host_workdir_fingerprint,
    } = prepared;

    // Modes that inject a credential require the well-known env
    // var to resolve to a non-empty value; fail fast with an
    // actionable structured error so the operator sees the
    // problem before container startup. The network/DinD sidecar may already
    // be warming in parallel with image materialization, so these errors route
    // through cleanup. Sync / Ignore short-circuit inside the helper.
    //
    // Build the per-layer mode-resolution and env-layer traces
    // here (in the caller) so the structured error carries the
    // full picture. The helpers mirror the layers walked by
    // `jackin_config::resolve_mode` and
    // `operator_env::build_attributed_layers` respectively.
    let workspace_name_str = workspace_name.as_deref().unwrap_or("");
    let workspace_opt = if workspace_name_str.is_empty() {
        None
    } else {
        Some(WorkspaceName::parse(workspace_name_str).map_err(anyhow::Error::from)?)
    };
    // Ad-hoc / path launches have no saved workspace key; still need a
    // display token for AuthCredentialMissing messaging.
    let workspace_for_verify = match workspace_opt.as_ref() {
        Some(ws) => ws.clone(),
        None => WorkspaceName::parse("adhoc").map_err(anyhow::Error::from)?,
    };
    let mode_resolution = super::super::super::build_mode_resolution(
        config,
        agent,
        workspace_opt.as_ref(),
        &role_key,
    );
    let env_layers = agent
        .required_env_var(auth_mode)
        .map_or_else(Vec::new, |env_var| {
            super::super::super::build_env_layer_states(
                config,
                workspace_opt.as_ref(),
                &role_key,
                env_var,
            )
        });
    if let Err(error) = verify_credential_env_present(
        agent,
        auth_mode,
        &operator_env,
        &mode_resolution,
        &env_layers,
        &workspace_for_verify,
        &role_key,
    ) {
        cleanup.run(docker).await;
        return Err(error.into());
    }

    // Resolve the GitHub-auth axis. Layered like the per-agent
    // resolver but with no agent dimension — `.config/gh/` is
    // shared by every agent in the container.
    let github_mode = jackin_config::resolve_github_mode(config, workspace_opt.as_ref(), &role_key);
    let github_env_decls =
        jackin_config::build_github_env_layers(config, workspace_opt.as_ref(), &role_key);
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
            jackin_diagnostics::active_timing_done("credentials", "github_env", Some(&detail));
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
            .get(jackin_core::GH_TOKEN_ENV_NAME)
            .cloned(),
    };

    // Token-mode pre-flight: GH_TOKEN must resolve to a non-empty
    // value before container startup. Sidecar resources may already be warming
    // in parallel and are cleaned up on failure.
    if let Err(error) = verify_github_token_present(
        github_mode,
        github_ctx.token.as_deref(),
        &workspace_for_verify,
        role_key.as_str(),
    ) {
        cleanup.run(docker).await;
        return Err(error);
    }

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
    let workspace_opt_owned = workspace_opt.clone();
    let role_key_owned = role_key.clone();
    let github_ctx_owned = github_ctx.clone();
    let role_state_future = async move {
        tokio::task::spawn_blocking(move || {
            let resolve_mode = |a: jackin_core::Agent| {
                jackin_config::resolve_mode(
                    &config_owned,
                    a,
                    workspace_opt_owned.as_ref(),
                    &role_key_owned,
                )
            };
            // Each agent may have an operator-configured sync-source-dir override
            // that replaces host_home for auth sync.
            let resolve_sync_src = |a: jackin_core::Agent| {
                jackin_config::resolve_sync_source_dir(
                    &config_owned,
                    a,
                    workspace_opt_owned.as_ref(),
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
        if early_sidecar_result.is_some() {
            (&mut role_state_future).await
        } else {
            tokio::select! {
                result = &mut sidecar => {
                    early_sidecar_result = Some(result);
                    (&mut role_state_future).await
                }
                result = &mut role_state_future => result,
            }
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
    let env_res: EnvironmentResolved = EnvironmentResolved {
        state,
        github_resolved_env,
        workspace_name_str: workspace_name_str.to_owned(),
        workspace_opt: workspace_opt.clone(),
        github_mode,
        github_env_decls: github_env_decls.clone(),
    };
    let EnvironmentResolved {
        state,
        github_resolved_env,
        workspace_name_str,
        workspace_opt,
        github_mode,
        github_env_decls,
    } = env_res;
    // The sidecar (adopted or freshly started above) is now running, so a
    // bare `?` here would leak the container/network/volume. Route trust
    // seeding through cleanup like the role-state and sidecar arms.
    if let Err(error) = seed_codex_project_trust(&state, workspace) {
        cleanup.run(docker).await;
        return Err(error);
    }
    let _trust: TrustSeeded = TrustSeeded;

    if agent != jackin_core::Agent::Codex {
        let _expiry_days = workspace_opt
            .as_ref()
            .filter(|_| auth_mode == jackin_config::AuthForwardMode::OAuthToken)
            .and_then(|ws| match jackin_env::expiry_days_for_launch(paths, ws) {
                Ok(days) => days,
                Err(e) => {
                    let message = format!(
                        "[jackin] note: token expiry cache for workspace {ws} \
                                 is unreadable ({e}); re-run \
                                 `jackin workspace claude-token setup {ws}` to refresh."
                    );
                    if let Some(run) = jackin_diagnostics::active_run() {
                        run.compact("auth", &message);
                    }
                    None
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
        let gh_token_key = jackin_core::GH_TOKEN_ENV_NAME;
        if let Some(run) = jackin_diagnostics::active_run() {
            if matches!(github_mode, jackin_config::GithubAuthMode::Ignore) {
                run.compact("github_auth", "GitHub auth ignored by auth_forward=ignore");
            } else {
                let token_breadcrumb = github_env_decls.get(gh_token_key).map_or_else(
                    || gh_token_key.to_owned(),
                    |value| {
                        super::super::super::auth_token_source_reference(
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
    // Path/display label (may be a workdir path for ad-hoc workspaces) — not
    // the config-stem WorkspaceName used for saved-workspace identity.
    let workspace_label = workspace
        .as_workspace_label()
        .map_err(anyhow::Error::from)?;
    jackin_diagnostics::debug_log!(
        "isolation",
        "load_role: invoking materialize_workspace for container {container_name} (interactive={interactive}, force={force})",
        force = opts.force,
    );
    if let Some(git_pull_join) = git_pull_join {
        super::super::finish_deferred_git_pull(git_pull_join, steps).await?;
    }
    if let Some(progress) = steps.progress_mut() {
        progress.stage_started(
            crate::runtime::progress::LaunchStage::Workspace,
            "materializing workspace",
        );
    }
    let materialize_preflight = crate::isolation::materialize::PreflightContext {
        workspace_label: workspace_label.clone(),
        force: opts.force,
        interactive,
    };
    let materialize = crate::isolation::materialize::materialize_workspace(
        workspace,
        &container_state,
        &role_key,
        &container_name,
        &workspace_label,
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
        if let Err(status_err) = super::super::super::write_instance_status(
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
            super::super::launch_phases::mark_failed_setup_then_cleanup(
                paths,
                &container_state,
                &container_name,
                &mut instance_manifest,
                &cleanup,
                docker,
                "workspace materialization error",
            )
            .await;
            return Err(error);
        }
    };
    if let Some(progress) = steps.progress_mut() {
        progress.stage_done(
            crate::runtime::progress::LaunchStage::Workspace,
            "materialized",
        );
    }

    let dirty_exit_policy =
        config.resolve_dirty_exit_policy(config.workspaces.get(workspace_label.as_str()));
    // The in-capsule dirty-exit modal assesses every isolated worktree/clone
    // mount; `shared` mounts are host-owned and never checked.
    let isolated_worktrees = materialized
        .mounts
        .iter()
        .filter(|mount| !mount.isolation.is_shared())
        .map(|mount| mount.dst.clone())
        .collect();
    let mut launch_config = super::super::super::capsule_config(
        selector,
        &workspace.workdir,
        &validated_repo.manifest,
        opts.initial_provider(),
        dirty_exit_policy.as_str(),
        isolated_worktrees,
    );
    // Carry the on-demand credential bindings to the host resolver, which
    // the launch path starts once the per-container socket dir exists.
    launch_config.exec_bindings = exec_bindings;
    let ws_mat: WorkspaceMaterialized = WorkspaceMaterialized {
        materialized,
        launch_config,
    };
    let WorkspaceMaterialized {
        materialized,
        launch_config,
    } = ws_mat;

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
    // Exhaustive match (not an `if`) so a future backend variant is a
    // compile error here instead of silently taking the Docker path.
    match backend {
        super::super::super::Backend::Docker => {}
        super::super::super::Backend::AppleContainer => {
            cleanup.run(docker).await;
            let mount_pairs = super::super::super::build_workspace_mount_pairs(&materialized);
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
    }

    let reuse_staleness_sentinel = (selected_image_reused
        && crate::runtime::image::reuse_needs_background_staleness_check(
            paths,
            &validated_repo,
            &image,
        ))
    .then_some(
        super::super::super::launch_runtime::ReuseStalenessSentinel {
            role_git: &source.git,
            branch_override: opts.role_branch.as_deref(),
            image: &image,
        },
    );

    let ctx = super::super::super::LaunchContext {
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
        profile: resolved_profile.0,
        profile_source: resolved_profile.1,
        grants: &effective_grants,
        paths,
        selected_image_refresh: selected_refresh_reason.map(|reason| {
            super::super::super::SelectedImageRefresh {
                role_git: &source.git,
                branch_override: opts.role_branch.as_deref(),
                reason,
            }
        }),
        reuse_staleness_sentinel,
        sidecar_prewarm_replenish: if adopted_sidecar_was_used {
            super::super::super::SidecarPrewarmReplenish::AfterAttach
        } else {
            super::super::super::SidecarPrewarmReplenish::None
        },
        sibling_prewarm: super::super::super::SiblingPrewarm {
            role_git: &source.git,
            branch_override: opts.role_branch.as_deref(),
            validated_repo: &validated_repo,
            selected_image_reused,
        },
        sibling_auth_prewarm: super::super::super::SiblingAuthPrewarm {
            manifest: &validated_repo.manifest,
            config,
            workspace_name: &workspace_name_str,
            role_key: &role_key,
        },
    };
    #[expect(
        clippy::needless_borrow,
        reason = "documented residual allow; prefer expect when site is lint-true"
    )]
    let launch_result =
        super::super::super::launch_role_runtime(&ctx, &mut steps, docker, runner).await;
    if launch_result.is_err() {
        // FailedSetup write error must not abort cleanup; surface to stderr
        // so the operator sees the on-disk status is stale (Active) and
        // that `jackin inspect` / `hardline` may report misleading state.
        if let Err(status_err) = super::super::super::write_instance_status(
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
    let _launched: RuntimeLaunched = RuntimeLaunched;
    // Launch succeeded. From here on the cleanup struct is reused
    // to tear down docker resources at session end (clean exit,
    // crash, NotFound, etc.); the host-side socket dir + Capsule
    // launch config stay behind for operator inspection and get
    // swept by the next explicit `jackin eject` / Purge.
    cleanup.keep_socket_dir();

    // Post-success finalization: status writes, attach-outcome inspect, and
    // foreground finalize. On any error reclaim DinD/network/certs while
    // cleanup is still armed — bare `?` here used to return before the
    // teardown match and orphan those resources.
    let decision = {
        let finalize_result: anyhow::Result<crate::isolation::finalize::FinalizeDecision> = async {
            super::super::super::write_instance_status(
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
            // The dirty-exit decision is made in-capsule (the dirty-exit modal) and
            // recorded in exit-action.json; the host only executes it — no host dialog.
            let mut prompt = crate::isolation::finalize::ExitActionPrompt {
                state_dir: paths.data_dir.join(&container_name).join("state"),
            };
            let dirty_exit_policy = config.resolve_dirty_exit_policy(
                workspace_name
                    .as_deref()
                    .and_then(|n| config.workspaces.get(n)),
            );
            let outcome =
                super::super::super::inspect_attach_outcome(docker, &container_name).await?;
            super::super::super::write_instance_attach_outcome(
                paths,
                &container_state,
                &mut instance_manifest,
                outcome,
            )?;
            let mut decision = crate::isolation::finalize::finalize_foreground_session(
                &container_name,
                &paths.data_dir.join(&container_name),
                outcome,
                interactive_finalize,
                dirty_exit_policy,
                &mut prompt,
                docker,
                runner,
            )
            .await?;
            super::super::super::write_preserved_status_if_applicable(
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
                let outcome2 =
                    super::super::super::inspect_attach_outcome(docker, &container_name).await?;
                super::super::super::write_instance_attach_outcome(
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
                    dirty_exit_policy,
                    &mut prompt,
                    docker,
                    runner,
                )
                .await?;
                super::super::super::write_preserved_status_if_applicable(
                    decision,
                    paths,
                    &container_state,
                    &mut instance_manifest,
                )?;
            }
            Ok(decision)
        }
        .await;

        match finalize_result {
            Ok(decision) => decision,
            Err(err) => {
                // A post-success finalization step failed. cleanup is still armed
                // (the region never runs/disarms it); reclaim DinD/network/certs
                // rather than orphaning them, consistent with the teardown arms.
                cleanup.run(docker).await;
                return Err(err);
            }
        }
    };
    let finalized: SessionFinalized = SessionFinalized { decision };
    let SessionFinalized { decision } = finalized;

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
                    super::super::super::write_instance_status(
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
                super::super::super::write_instance_status(
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
            cleanup.run(docker).await;
            purge_or_mark_clean_exited(
                paths,
                &container_name,
                &container_state,
                &mut instance_manifest,
                docker,
                runner,
                "clean exit",
            )
            .await?;
        }
        ContainerState::Stopped { .. }
        | ContainerState::Created
        | ContainerState::Removing
        | ContainerState::Dead => {
            super::super::super::write_instance_status(
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
            cleanup.run(docker).await;
            // D9: container already gone — purge local state inline.
            purge_or_mark_clean_exited(
                paths,
                &container_name,
                &container_state,
                &mut instance_manifest,
                docker,
                runner,
                "NotFound clean exit",
            )
            .await?;
        }
    }

    let classified: CleanupClassified = CleanupClassified {
        container_name: container_name.clone(),
    };
    Ok(classified.container_name)
}
