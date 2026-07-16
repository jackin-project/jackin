// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `LaunchContext` + prewarm types + `spawn_sibling_auth_prewarm` + `launch_role_runtime`
//! (+ host passthrough + debug env helpers) extracted from launch coordinator (File1).
//! All items `pub(crate)` re-exported from the coordinator to preserve `super::` / `use super::*` .

#![expect(
    private_interfaces,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]

use anyhow::Context;
use jackin_config::AppConfig;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_core::{CommandRunner, RunOptions};
use jackin_docker::docker_client::DockerApi;

use crate::instance::naming::dind_certs_volume;
use crate::instance::{PrepareResolvers, RoleState};
use crate::runtime::identity::GitIdentity;

use super::progress_helpers::StepCounter;

use super::attach_failure_error;
use super::auth_error::{
    NO_PROXY_LOWER, NO_PROXY_UPPER, append_no_proxy_host, is_proxy_env_name, push_env_if_present,
};
use super::build_workspace_mount_strings;
use super::diagnose_with_state;
use super::exit_diagnosis::{ExitPhase, diagnose_premature_exit};
use crate::runtime::progress::launch_output;

pub(crate) struct LaunchContext<'a> {
    pub(crate) container_name: &'a str,
    pub(crate) image: &'a str,
    pub(crate) network: &'a str,
    pub(crate) dind: &'a str,
    pub(crate) selector: &'a RoleSelector,
    pub(crate) agent_display_name: &'a str,
    pub(crate) workspace: &'a crate::isolation::materialize::MaterializedWorkspace,
    pub(crate) state: &'a RoleState,
    pub(crate) git: &'a GitIdentity,
    pub(crate) debug: bool,
    pub(crate) git_coauthor_trailer: bool,
    pub(crate) git_dco: bool,
    pub(crate) agent: jackin_core::Agent,
    pub(crate) capsule_config: &'a jackin_protocol::CapsuleConfig,
    pub(crate) resolved_env: &'a jackin_env::ResolvedEnv,
    pub(crate) profile: crate::runtime::docker_profile::DockerSecurityProfile,
    pub(crate) profile_source: crate::runtime::docker_profile::ProfileSource,
    pub(crate) grants: &'a crate::runtime::docker_profile::EffectiveGrants,
    /// Resolved `[…github.env]` map (post `op://` + `$NAME`
    /// resolution). `GH_TOKEN` carries the token in the launcher's
    /// preferred env-injection path; `GH_HOST` and
    /// `GH_ENTERPRISE_TOKEN` are forwarded as-is when set so GHE
    /// targets work end to end.
    pub(crate) github_env: &'a std::collections::BTreeMap<String, String>,
    /// Required so `launch_role_runtime` can fire the `keep_awake`
    /// reconciler between `docker run -d` and the foreground `docker
    /// attach`. Without that mid-flight call, caffeinate would never
    /// spawn for an interactive `jackin load`: the post-launch
    /// reconcile in `app::Command::Load` only runs after attach
    /// returns, by which time the container has stopped and the
    /// `keep_awake` count is back to zero.
    pub(crate) paths: &'a JackinPaths,
    pub(crate) selected_image_refresh: Option<SelectedImageRefresh<'a>>,
    pub(crate) reuse_staleness_sentinel: Option<ReuseStalenessSentinel<'a>>,
    pub(crate) sidecar_prewarm_replenish: SidecarPrewarmReplenish,
    pub(crate) sibling_prewarm: SiblingPrewarm<'a>,
    pub(crate) sibling_auth_prewarm: SiblingAuthPrewarm<'a>,
}

pub(crate) struct SelectedImageRefresh<'a> {
    pub(crate) role_git: &'a str,
    pub(crate) branch_override: Option<&'a str>,
    pub(crate) reason: crate::runtime::image::ImageInvalidationReason,
}

pub(crate) struct ReuseStalenessSentinel<'a> {
    pub(crate) role_git: &'a str,
    pub(crate) branch_override: Option<&'a str>,
    pub(crate) image: &'a str,
}

pub(crate) enum SidecarPrewarmReplenish {
    None,
    AfterAttach,
}

pub(crate) struct SiblingPrewarm<'a> {
    pub(crate) role_git: &'a str,
    pub(crate) branch_override: Option<&'a str>,
    pub(crate) validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    pub(crate) selected_image_reused: bool,
}

pub(crate) struct SiblingAuthPrewarm<'a> {
    pub(crate) manifest: &'a jackin_manifest::RoleManifest,
    pub(crate) config: &'a AppConfig,
    pub(crate) workspace_name: &'a str,
    pub(crate) role_key: &'a str,
}

pub(crate) fn spawn_sibling_auth_prewarm(
    paths: &JackinPaths,
    container_name: &str,
    prewarm: &SiblingAuthPrewarm<'_>,
    selected_agent: jackin_core::Agent,
) -> Option<tokio::task::JoinHandle<()>> {
    let active_run = jackin_diagnostics::active_run_for_paths(paths);
    let sibling_agents = prewarm
        .manifest
        .supported_agents()
        .into_iter()
        .filter(|agent| *agent != selected_agent)
        .collect::<Vec<_>>();
    if sibling_agents.is_empty() {
        if let Some(run) = &active_run {
            run.compact(
                "sibling_auth_prewarm_skipped",
                &format!("no sibling agents for selected agent {selected_agent}"),
            );
        }
        return None;
    }

    let paths_owned = paths.clone();
    let home_dir = paths.home_dir.clone();
    let container_name = container_name.to_owned();
    let manifest = prewarm.manifest.clone();
    let config = prewarm.config.clone();
    let workspace_name = prewarm.workspace_name.to_owned();
    let role_key = prewarm.role_key.to_owned();
    let agents = sibling_agents
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if let Some(run) = &active_run {
        run.compact(
            "sibling_auth_prewarm_started",
            &format!(
                "prewarming {} sibling auth slots for selected agent {selected_agent}: {}",
                sibling_agents.len(),
                agents.join(", ")
            ),
        );
    }
    let timing_detail = agents.join(",");
    if let Some(run) = &active_run {
        let reason = format!("sibling_auth_prewarm:{timing_detail}");
        let detail = serde_json::json!({
            "plan": "PrewarmOnly",
            "reason": reason,
            "container": null,
        })
        .to_string();
        run.stage(
            "launch_plan",
            jackin_diagnostics::DiagnosticStage::Restore,
            "selected launch plan PrewarmOnly",
            Some(&detail),
        );
        run.timing_started(
            jackin_diagnostics::DiagnosticStage::Credentials,
            "sibling_auth_prewarm",
            Some(&timing_detail),
        );
    }

    Some(jackin_telemetry::spawn::joined_blocking(move || {
        let resolve_mode = |a: jackin_core::Agent| {
            let ws = jackin_core::WorkspaceName::parse(&workspace_name).ok();
            jackin_config::resolve_mode(&config, a, ws.as_ref(), &role_key)
        };
        let resolve_sync_src = |a: jackin_core::Agent| {
            let ws = jackin_core::WorkspaceName::parse(&workspace_name).ok();
            jackin_config::resolve_sync_source_dir(&config, a, ws.as_ref(), &role_key)
        };
        let result = RoleState::prewarm_auth_for_agents(
            &paths_owned,
            &container_name,
            &manifest,
            &PrepareResolvers {
                auth_modes: &resolve_mode,
                sync_source_dirs: &resolve_sync_src,
            },
            &home_dir,
            &sibling_agents,
        );
        let timing_done = match &result {
            Ok(count) => format!("{count} slots"),
            Err(error) => format!("failed: {error}"),
        };
        if let Some(run) = &active_run {
            run.timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                "sibling_auth_prewarm",
                Some(&timing_done),
            );
        }

        if let Some(run) = active_run {
            match result {
                Ok(count) => run.compact(
                    "sibling_auth_prewarm_done",
                    &format!(
                        "prewarmed {count} sibling auth slots for selected agent {selected_agent}"
                    ),
                ),
                Err(error) => run.compact(
                    "sibling_auth_prewarm_failed",
                    &format!(
                        "sibling auth prewarm failed for selected agent {selected_agent}: {error}"
                    ),
                ),
            }
        }
    }))
}

/// Launch the role container after the caller has prepared the private network
/// and `DinD` sidecar.
#[expect(
    clippy::too_many_lines,
    reason = "Launch pipeline coordinator with three distinct phases (profile \
              validation + apparmor probe + run/launch/teardown sequence). \
              Body extraction is a dedicated parallel-pass slice — helpers \
              `validate_launch_profile`, `probe_apparmor_layer`, `execute_\
              docker_run_sequence`, and `finalize_role_session` to land in a \
              follow-up PR. Until then, the inline shape preserves the captured- \
              locals across phases without param-struct boilerplate. Per the R6 \
              burn-down strategy: while this `#[allow]` is recorded, the \
              deferred body-extraction slice remains tracked as a roadmap item."
)]
#[expect(
    clippy::cognitive_complexity,
    reason = "Same justification as the too_many_lines allow: launch pipeline \
              branching depth tracks the bring-up / phase / teardown sequence \
              branching, not algorithmic complexity. Body extraction follows the \
              same deferred-parallel-pass plan."
)]
pub(crate) async fn launch_role_runtime(
    ctx: &LaunchContext<'_>,
    steps: &mut StepCounter,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let LaunchContext {
        container_name,
        image,
        network,
        dind,
        selector,
        agent_display_name,
        workspace,
        state,
        git,
        debug,
        git_coauthor_trailer,
        git_dco,
        agent,
        capsule_config,
        resolved_env,
        profile,
        profile_source,
        grants,
        github_env,
        paths,
        selected_image_refresh,
        reuse_staleness_sentinel,
        sidecar_prewarm_replenish,
        sibling_prewarm,
        sibling_auth_prewarm,
    } = ctx;

    let certs_volume = dind_certs_volume(container_name);
    let dind_enabled = crate::runtime::docker_profile::dind_enabled(grants);
    let network_disabled = crate::runtime::docker_profile::network_disabled(grants);

    let cgroup_version = crate::runtime::docker_profile::probe_cgroup_version();
    if let Some(warning) =
        crate::runtime::docker_profile::validate_cgroup_for_profile(*profile, cgroup_version)
            .map_err(|msg| anyhow::anyhow!(msg))?
    {
        // Always-on (not --debug): a silently-dropped resource limit is operator-
        // visible degradation, like the privileged-DinD warning below.
        jackin_diagnostics::emit_compact_line("warning", warning);
    }
    // WP4 Part B: rootless DinD requires cgroup v2 — fail closed on v1 rather
    // than silently falling back to a privileged sidecar.
    crate::runtime::docker_profile::validate_dind_grant_for_cgroup(grants.dind, cgroup_version)
        .map_err(|msg| anyhow::anyhow!(msg))?;
    // WP4 / Decision 12: privileged DinD under hardened/locked defeats the
    // capability + network boundary the profile promises. It is allowed only by
    // explicit grant, but the operator must be told the enforcement is partial.
    if crate::runtime::docker_profile::dind_privileged(grants)
        && crate::runtime::docker_profile::drops_all_caps(*profile)
    {
        jackin_diagnostics::emit_compact_line(
            "warning",
            &format!(
                "privileged DinD under `{profile}` profile defeats capability and network isolation (partial enforcement); prefer `dind = \"rootless\"`"
            ),
        );
    }
    // AppArmor only feeds the `--debug` telemetry + session contract, so skip the
    // `docker info` round-trip on the common non-debug launch. On a probe error,
    // report layer `unknown` rather than letting a failed round-trip masquerade
    // as a genuine `available=no` in the audit surface.
    let (apparmor_available, apparmor_layer) = if *debug {
        match runner
            .capture(
                "docker",
                &["info", "--format", "{{.SecurityOptions}}"],
                None,
            )
            .await
        {
            Ok(info) => crate::runtime::docker_profile::parse_apparmor_from_docker_info(&info),
            Err(err) => {
                jackin_diagnostics::telemetry_debug!("launch", "apparmor probe failed: {err:#}");
                (false, "unknown")
            }
        }
    } else {
        (false, "host")
    };

    let docker_run_opts = RunOptions {
        quiet: !debug,
        ..RunOptions::default()
    };

    // Step 4: Mount volumes and launch
    steps.next("Launching role").await?;
    steps.done();

    if steps.progress.is_none() {
        launch_output().print_deploying(agent_display_name).await;
    }

    let class_label = format!("jackin.class={}", selector.key());
    let display_label = format!("jackin.display.name={agent_display_name}");
    let docker_host = format!("DOCKER_HOST=tcp://{dind}:2376");
    let docker_cert_path = "DOCKER_CERT_PATH=/jackin/run/dind-certs/client";
    let dind_hostname = format!("{}={dind}", jackin_core::JACKIN_DIND_HOSTNAME_ENV_NAME);
    let role_container_name_env = format!(
        "{}={container_name}",
        jackin_core::JACKIN_CONTAINER_NAME_ENV_NAME
    );
    let instance_id = if let Some(id) =
        crate::instance::naming::instance_id_from_container_base(container_name)
    {
        id
    } else {
        jackin_diagnostics::emit_compact_line(
            "warning",
            &format!(
                "warning: instance_id_from_container_base could not parse {container_name:?}; falling back to full container name as JACKIN_INSTANCE_ID — chrome chip will render the full name"
            ),
        );
        container_name
    };
    let instance_id_env = format!("{}={instance_id}", jackin_core::JACKIN_INSTANCE_ID_ENV_NAME);
    let testcontainers_host_override = format!(
        "{}={dind}",
        jackin_core::TESTCONTAINERS_HOST_OVERRIDE_ENV_NAME
    );
    let git_author_name = format!("GIT_AUTHOR_NAME={}", git.user_name);
    let git_author_email = format!("GIT_AUTHOR_EMAIL={}", git.user_email);
    let agent_specific_mounts = super::agent_mounts(state);
    let gh_config_mount = super::github_config_mount(state);
    let certs_agent_mount = format!("{certs_volume}:/jackin/run/dind-certs/client:ro");

    // Start detached with a persistent TTY, then attach separately.  This
    // decouples the container's lifetime from the foreground attach, so
    // closing the terminal tab only drops the attach — the container keeps
    // running and `jackin hardline` can reconnect to the same live session.
    //
    // No `--rm` here, intentionally.  Omitting `--rm` lets the container
    // persist after exit so that:
    //   - A crashed container's logs and filesystem remain available for
    //     diagnosis, and `jackin hardline` can restart the exact same
    //     container in place without rebuilding the network stack.
    //   - Clean-exit containers (exit 0, not OOM-killed) are removed by
    //     `cleanup` at the end of a normal session, and the
    //     `claim_container_name` loop reclaims those slots on the next load
    //     by inspecting per-candidate state rather than relying on Docker
    //     auto-removal.
    //
    // Using the naming loop rather than `--rm` as the removal mechanism is
    // the right boundary: the loop can inspect the exit code and OOM flag
    // and make a correct per-container decision (reclaim vs. preserve for
    // hardline), whereas `--rm` would indiscriminately destroy every exited
    // container, making crash recovery impossible.
    let mut run_args: Vec<&str> = vec![
        "run",
        "-d",
        "--name",
        container_name,
        "--hostname",
        container_name,
        "--label",
        crate::runtime::naming::LABEL_MANAGED,
        "--label",
        crate::runtime::naming::LABEL_KIND_ROLE,
        "--label",
        &class_label,
        "--label",
        &display_label,
        "--workdir",
        &workspace.workdir,
    ];

    let network = if network_disabled { "none" } else { network };
    run_args.extend_from_slice(&["--network", network]);

    if workspace.keep_awake_enabled {
        run_args.extend_from_slice(&["--label", crate::runtime::naming::LABEL_KEEP_AWAKE]);
    }

    let capability_flags =
        crate::runtime::docker_profile::capability_flags(*profile, &grants.capabilities_add);
    run_args.extend(capability_flags.iter().map(String::as_str));
    let readonly_flags = crate::runtime::docker_profile::readonly_root_flags(*profile, grants);
    run_args.extend(readonly_flags.iter().map(String::as_str));
    if grants.no_new_privileges {
        run_args.extend_from_slice(&["--security-opt", "no-new-privileges"]);
    }
    let resource_flags = crate::runtime::docker_profile::resource_flags(grants);
    run_args.extend(resource_flags.iter().map(String::as_str));
    // WP3: per-decision launch telemetry. One line per applied control so a
    // `--debug` run shows exactly what was enforced. The session contract
    // (emitted below, once credential state is known) is the human-readable
    // summary of the same data.
    let yes_no = |enabled: bool| if enabled { "yes" } else { "no" };
    jackin_diagnostics::telemetry_debug!(
        "launch",
        "profile_selected profile={profile} source={profile_source}",
    );
    jackin_diagnostics::telemetry_debug!(
        "launch",
        "cap_drop_all={} cap_add={}",
        yes_no(crate::runtime::docker_profile::drops_all_caps(*profile)),
        if grants.capabilities_add.is_empty() {
            "-".to_owned()
        } else {
            grants.capabilities_add.join(",")
        },
    );
    jackin_diagnostics::telemetry_debug!(
        "launch",
        "no_new_privileges enforced={}",
        yes_no(grants.no_new_privileges),
    );
    jackin_diagnostics::telemetry_debug!("launch", "seccomp profile=docker-default");
    jackin_diagnostics::telemetry_debug!(
        "launch",
        "apparmor available={} profile=docker-default layer={apparmor_layer}",
        yes_no(apparmor_available),
    );
    jackin_diagnostics::telemetry_debug!(
        "launch",
        "read_only_root enforced={} tmpfs={}",
        yes_no(!grants.system_writes),
        if grants.system_writes {
            "-".to_owned()
        } else {
            crate::runtime::docker_profile::tmpfs_paths(*profile).join(",")
        },
    );
    jackin_diagnostics::telemetry_debug!("launch", "cgroup_version v={cgroup_version}");
    for (kind, value) in [
        ("memory", grants.memory_bytes.map(|b| b.to_string())),
        ("cpus", grants.cpus.map(|c| c.to_string())),
        ("pids", grants.pids.map(|p| p.to_string())),
    ] {
        if let Some(value) = value {
            jackin_diagnostics::telemetry_debug!(
                "launch",
                "resource_limit kind={kind} value={value}"
            );
        }
    }
    jackin_diagnostics::telemetry_debug!(
        "launch",
        "dind enabled={dind_enabled} mode={}",
        grants.dind
    );
    // Host Docker socket is never mounted into a role container (hard rule);
    // guarded by `role_container_never_mounts_host_docker_socket` in tests.
    jackin_diagnostics::telemetry_debug!("launch", "host_socket_check passed=yes");
    jackin_diagnostics::telemetry_debug!(
        "launch",
        "network mode={} enforcement={}",
        crate::runtime::docker_profile::network_grant_label(grants.network),
        crate::runtime::docker_profile::network_enforcement_label(grants),
    );

    // Run the container as the host operator's UID (group 0). Matching the host
    // UID makes host-owned bind mounts transparently read/write, and the
    // derived image bakes that same UID into image-owned `/home/agent` paths —
    // see `identity::host_run_as_user`. `HOME` is set explicitly so shells and
    // the agent CLIs resolve the bind-mounted home even before any passwd lookup.
    let run_as_user = crate::runtime::identity::host_run_as_user();
    if let Some(ref user) = run_as_user {
        run_args.extend_from_slice(&[
            "--user",
            user.as_str(),
            "--group-add",
            "0",
            "-e",
            "HOME=/home/agent",
        ]);
    }

    run_args.extend_from_slice(&[
        // JACKIN_* runtime metadata is injected by jackin, not declared in role manifests.
        "-e",
        &role_container_name_env,
        "-e",
        &instance_id_env,
        "-e",
        &testcontainers_host_override,
        "-e",
        &git_author_name,
        "-e",
        &git_author_email,
    ]);
    if dind_enabled {
        run_args.extend_from_slice(&[
            "-e",
            &docker_host,
            "-e",
            "DOCKER_TLS_VERIFY=1",
            "-e",
            docker_cert_path,
            "-e",
            &dind_hostname,
        ]);
    }
    let run_envs = run_runtime_envs();
    for env in &run_envs {
        run_args.extend_from_slice(&["-e", env.as_str()]);
    }
    let debug_envs = debug_runtime_envs(*debug);
    for env in &debug_envs {
        run_args.extend_from_slice(&["-e", env.as_str()]);
    }
    let telemetry_envs = telemetry_runtime_envs(*debug);
    for env in &telemetry_envs {
        run_args.extend_from_slice(&["-e", env.as_str()]);
    }
    // Always pass the host jackin version so the capsule ContainerInfo dialog
    // can surface which host binary launched the container.
    let host_version_env = format!("JACKIN_HOST_VERSION={}", env!("JACKIN_VERSION"));
    run_args.extend_from_slice(&["-e", host_version_env.as_str()]);

    let git_coauthor_trailer_env = git_coauthor_trailer
        .then(|| format!("{}=1", jackin_core::JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME));
    if let Some(ref env) = git_coauthor_trailer_env {
        run_args.extend_from_slice(&["-e", env.as_str()]);
    }
    let git_dco_env = git_dco.then(|| format!("{}=1", jackin_core::JACKIN_GIT_DCO_ENV_NAME));
    if let Some(ref env) = git_dco_env {
        run_args.extend_from_slice(&["-e", env.as_str()]);
    }

    let passthrough_strings = host_runtime_passthrough_env(std::env::vars());
    for env_str in &passthrough_strings {
        run_args.push("-e");
        run_args.push(env_str);
    }
    let mut env_strings: Vec<String> = Vec::new();
    env_strings.push(format!(
        "{}={}",
        jackin_core::JACKIN_ENV_NAME,
        jackin_core::JACKIN_ENV_VALUE
    ));
    // DinD reachable only via Docker network; route past HTTP_PROXY by adding
    // hostname to NO_PROXY in both casings — Go reads upper, curl/Python
    // requests/wget read lower. Mirror the merged value across both casings
    // so an operator who declared only one variant still gets full bypass
    // coverage for tools that read the other.
    let proxy_seen = resolved_env.vars.iter().any(|(k, _)| is_proxy_env_name(k));
    let upper_existing = resolved_env
        .vars
        .iter()
        .find_map(|(k, v)| (k == NO_PROXY_UPPER).then_some(v.as_str()));
    let lower_existing = resolved_env
        .vars
        .iter()
        .find_map(|(k, v)| (k == NO_PROXY_LOWER).then_some(v.as_str()));
    for (key, value) in &resolved_env.vars {
        if jackin_core::is_reserved(key) {
            continue;
        }
        if key == NO_PROXY_UPPER || key == NO_PROXY_LOWER {
            // Synthesized below from merged casing — skip the inline emit.
            continue;
        }
        env_strings.push(format!("{key}={value}"));
    }

    // Grok's CLI accepts either XAI_API_KEY (personal) or GROK_DEPLOYMENT_KEY
    // (enterprise deployment key) for api-key auth. When the operator
    // configures a credential for Grok (via the api_key slot, which stores
    // under XAI_API_KEY, or has XAI_API_KEY in any env layer), also expose
    // it under GROK_DEPLOYMENT_KEY so the in-container `grok` sees the
    // credential under the name it prefers. Explicit GROK_DEPLOYMENT_KEY
    // in the layers takes precedence.
    if *agent == jackin_core::Agent::Grok
        && let Some((_, value)) = resolved_env.vars.iter().find(|(k, _)| k == "XAI_API_KEY")
        && !resolved_env
            .vars
            .iter()
            .any(|(k, _)| k == "GROK_DEPLOYMENT_KEY")
    {
        env_strings.push(format!("GROK_DEPLOYMENT_KEY={value}"));
    }
    // Trigger synth when any proxy class OR any NO_PROXY casing is declared.
    // The latter covers operators who set NO_PROXY without an HTTP_PROXY
    // (transparent proxy, /etc/environment, container-injected proxy vars).
    if dind_enabled && (proxy_seen || upper_existing.is_some() || lower_existing.is_some()) {
        let upper_value = upper_existing
            .or(lower_existing)
            .map_or_else(|| dind.to_string(), |v| append_no_proxy_host(v, dind));
        let lower_value = lower_existing
            .or(upper_existing)
            .map_or_else(|| dind.to_string(), |v| append_no_proxy_host(v, dind));
        env_strings.push(format!("{NO_PROXY_UPPER}={upper_value}"));
        env_strings.push(format!("{NO_PROXY_LOWER}={lower_value}"));
    }

    // GitHub auth env wiring. Token mode and Sync-with-host-token both
    // export GH_TOKEN AND GITHUB_TOKEN from the same source — `gh`
    // prefers GH_TOKEN, but the official github-mcp-server and most
    // GitHub-Actions-style scripts read GITHUB_TOKEN. Exporting both
    // closes Known Gap 3 in the roadmap doc. GH_HOST and
    // GH_ENTERPRISE_TOKEN are passed through as-is when declared by
    // the operator so GHE workspaces work end to end.
    let gh_token = state.gh_provision_outcome.token();
    push_env_if_present(&mut env_strings, jackin_core::GH_TOKEN_ENV_NAME, gh_token);

    env_strings.push(format!(
        "{}={}",
        jackin_core::JACKIN_NETWORK_MODE_ENV_NAME,
        crate::runtime::docker_profile::network_grant_label(grants.network)
    ));
    // WP-SUDO: the container provisions sudo at runtime from this signal (the
    // base image bakes no sudoers). Reserved so role manifests can't set it.
    if grants.sudo {
        env_strings.push(format!("{}=1", jackin_core::JACKIN_SUDO_ENV_NAME));
    }
    // Read-only-root profiles need writable-home env redirects (e.g. git's
    // global config) — see `readonly_home_env`, the companion to `tmpfs_paths`.
    env_strings.extend(crate::runtime::docker_profile::readonly_home_env(grants));
    // Computed once here so the WP1 allowlist (below) can include the OTLP
    // endpoint host; reused for OTLP propagation after env_strings is flushed.
    let container_otlp = jackin_diagnostics::container_otlp();
    // WP1: egress allowlist enforcement. Inject the assembled allowlist and the
    // truthful enforcement label so the `firewall-apply` exec (after the
    // container starts) installs an iptables OUTPUT allowlist. The OTLP host is
    // always included (Decision 9) so telemetry keeps flowing. Only the
    // allowlist tier installs a firewall; open/none get none.
    if grants.network == crate::runtime::docker_profile::NetworkGrant::Allowlist {
        let github_hosts = if gh_token.is_some() {
            crate::runtime::docker_profile::github_allowlist_hosts(
                github_env
                    .get(jackin_core::GH_HOST_ENV_NAME)
                    .map(String::as_str),
            )
        } else {
            Vec::new()
        };
        let otlp_host = container_otlp.as_ref().map(|_| "host.docker.internal");
        let allowlist = crate::runtime::docker_profile::allowlist_hosts(
            agent.slug(),
            grants,
            &github_hosts,
            otlp_host,
        );
        env_strings.push(format!(
            "{}={}",
            jackin_core::JACKIN_ALLOWED_HOSTS_ENV_NAME,
            allowlist.join(",")
        ));
        env_strings.push(format!(
            "{}={}",
            jackin_core::JACKIN_NETWORK_ENFORCEMENT_ENV_NAME,
            crate::runtime::docker_profile::network_enforcement_label(grants)
        ));
    }
    // WP3: render the session contract under `--debug` only (its sole consumer
    // is the debug_log below). Coarse `agent_auth_mode` reflects whether the
    // selected agent's auth was provisioned; richer posture is owned by WP7.
    if *debug {
        let agent_auth_mode = match agent.slug() {
            "claude" => state.auth.claude.is_some(),
            "codex" => state.auth.codex.is_some(),
            "amp" => state.auth.amp.is_some(),
            "kimi" => state.auth.kimi.is_some(),
            "opencode" => state.auth.opencode.is_some(),
            "grok" => state.auth.grok.is_some(),
            _ => false,
        };
        let session_contract = crate::runtime::docker_profile::format_session_contract(
            *profile,
            &profile_source.to_string(),
            grants,
            apparmor_available,
            apparmor_layer,
            cgroup_version,
            if agent_auth_mode {
                "provisioned"
            } else {
                "none"
            },
            gh_token.is_some(),
        );
        jackin_diagnostics::telemetry_debug!("launch", "session_contract\n{session_contract}");
    }
    push_env_if_present(
        &mut env_strings,
        jackin_core::GITHUB_TOKEN_ENV_NAME,
        gh_token,
    );
    push_env_if_present(
        &mut env_strings,
        jackin_core::GH_HOST_ENV_NAME,
        github_env
            .get(jackin_core::GH_HOST_ENV_NAME)
            .map(String::as_str),
    );
    push_env_if_present(
        &mut env_strings,
        jackin_core::GH_ENTERPRISE_TOKEN_ENV_NAME,
        github_env
            .get(jackin_core::GH_ENTERPRISE_TOKEN_ENV_NAME)
            .map(String::as_str),
    );

    for env_str in &env_strings {
        run_args.push("-e");
        run_args.push(env_str);
    }

    // OTLP cross-process propagation: hand the container the launch trace
    // context (W3C traceparent) and a container-reachable endpoint, so the
    // capsule's telemetry links back to this launch trace and shares the run.
    // host.docker.internal must be wired to the host gateway for the rewritten
    // loopback endpoint to resolve on Linux engines. `container_otlp` is
    // computed once above (for the WP1 allowlist) and reused here.
    let mut otlp_propagation: Vec<String> = Vec::new();
    if let Some(otlp) = &container_otlp {
        otlp_propagation.push(format!("OTEL_EXPORTER_OTLP_ENDPOINT={}", otlp.endpoint));
        if let Some(traceparent) = jackin_telemetry::propagation::current_traceparent() {
            otlp_propagation.push(format!("TRACEPARENT={traceparent}"));
        }
    }
    for env_str in &otlp_propagation {
        run_args.push("-e");
        run_args.push(env_str);
    }
    if container_otlp
        .as_ref()
        .is_some_and(|otlp| otlp.needs_host_gateway)
    {
        run_args.extend_from_slice(&["--add-host", "host.docker.internal:host-gateway"]);
    }

    if dind_enabled {
        run_args.extend_from_slice(&["-v", &certs_agent_mount]);
    }
    if let Some(gh_config_mount) = gh_config_mount.as_deref() {
        run_args.extend_from_slice(&["-v", gh_config_mount]);
    }
    for mount in &agent_specific_mounts {
        run_args.push("-v");
        run_args.push(mount);
    }

    let mount_strings = build_workspace_mount_strings(workspace);
    for ms in &mount_strings {
        run_args.push("-v");
        run_args.push(ms);
    }
    let image_label = format!("jackin.image={image}");
    run_args.extend_from_slice(&["--label", &image_label]);
    // Host-side bind-mount of the daemon's socket directory. Pre-create
    // host-side so Docker does not materialise the target itself as
    // root:root 0755. The dir is owned by the host operator (this process)
    // and the container runs as that same UID/GID (`--user`), so the `agent`
    // user creates jackin.sock with no special directory mode. The socket
    // file itself gets 0o600 from inside the capsule. The same directory
    // carries Capsule's normalized launch config.
    let socket_dir = paths.jackin_home.join("sockets").join(*container_name);
    let capsule_config_contents = toml::to_string(capsule_config)
        .context("serializing Capsule launch config for /jackin/run/agent.toml")?;
    // Runtime passwd/group entries for the host UID/GID so `getpwuid`/`$HOME`
    // resolve to the `agent` user inside the container even though the image
    // only bakes UID 1000. Consumed via `libnss-extrausers` (see
    // docker/construct). Shared files depend only on the host UID/GID; written
    // atomically (per-container temp + rename) so a concurrent launch can't
    // read torn files at mount time, and only when the bytes actually change
    // so the rename can't swap the inode out from under a live `:ro` bind
    // mount in an already-running container.
    let extrausers_passwd = paths.jackin_home.join("extrausers").join("passwd");
    let extrausers_group = paths.jackin_home.join("extrausers").join("group");
    let extrausers_entries = match (
        crate::runtime::identity::host_uid(),
        crate::runtime::identity::host_gid(),
    ) {
        (Some(uid), Some(gid)) => Some((
            format!("agent:x:{uid}:{gid}:agent:/home/agent:/bin/zsh\n"),
            format!("agent-host:x:{gid}:agent\n"),
        )),
        _ => None,
    };
    let extrausers_tmp = extrausers_passwd.with_file_name(format!("passwd.{container_name}.tmp"));
    let extrausers_group_tmp =
        extrausers_group.with_file_name(format!("group.{container_name}.tmp"));
    // Run the filesystem syscalls on the blocking pool — the tokio
    // runtime is built without the `fs` feature here, and blocking on
    // a slow / NFS host parks the worker driving the docker-run RPC
    // for every other future scheduled on it.
    // Host-shared usage cache dir, mounted into every container so the
    // account-keyed snapshot/cooldown (and refresh lock) coordinate across
    // instances — a new instance reads the prior state and only one instance
    // refreshes a given account (Class III). Shared (no container_name), owned by
    // the host UID like the socket dir so the container can write it.
    let usage_shared_dir = paths.jackin_home.join("data").join("usage-shared");
    let usage_shared_dir_for_mkdir = usage_shared_dir.clone();
    let socket_dir_for_mkdir = socket_dir.clone();
    let capsule_config_contents_for_write = capsule_config_contents.clone();
    let extrausers_passwd_for_write = extrausers_passwd.clone();
    let extrausers_group_for_write = extrausers_group.clone();
    let extrausers_entries_for_write = extrausers_entries.clone();
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "prepare_socket_dir",
        Some(container_name),
    );
    let prepare_socket_dir_result =
        jackin_telemetry::spawn::joined_blocking(move || -> std::io::Result<()> {
            std::fs::create_dir_all(&socket_dir_for_mkdir)?;
            std::fs::create_dir_all(&usage_shared_dir_for_mkdir)?;
            std::fs::write(
                socket_dir_for_mkdir.join(jackin_protocol::CAPSULE_CONFIG_FILENAME),
                capsule_config_contents_for_write,
            )?;
            if let Some((passwd_line, group_line)) = extrausers_entries_for_write {
                if let Some(parent) = extrausers_passwd_for_write.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                super::write_if_changed_atomic(
                    &extrausers_passwd_for_write,
                    &extrausers_tmp,
                    passwd_line.as_bytes(),
                )?;
                super::write_if_changed_atomic(
                    &extrausers_group_for_write,
                    &extrausers_group_tmp,
                    group_line.as_bytes(),
                )?;
            }
            Ok(())
        })
        .await
        .context("socket dir mkdir worker join")
        .and_then(|result| {
            result.with_context(|| {
                format!(
                    "creating host-side socket dir {} for container {container_name}",
                    socket_dir.display(),
                )
            })
        });
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "prepare_socket_dir",
        if prepare_socket_dir_result.is_ok() {
            Some("prepared")
        } else {
            Some("error")
        },
    );
    prepare_socket_dir_result?;
    // Start the jackin-exec host credential resolver for this container's
    // on-demand bindings. Its socket lands in the dir just prepared (bind-
    // mounted to /jackin/run), so the in-container capsule reaches it at
    // /jackin/run/host.sock. Spawned detached: the task runs independently of
    // this handle, for the host process's lifetime alongside the interactive
    // attach. No-op when the workspace declares no on-demand credentials.
    if !ctx.capsule_config.exec_bindings.is_empty() {
        drop(crate::exec_host::start_for_container(
            &ctx.paths.jackin_home,
            ctx.container_name,
            &ctx.capsule_config.exec_bindings,
        ));
    }
    // `Display` is lossy on non-UTF-8 paths — docker would silently mount a
    // different host dir than the one we just created. Bail rather than
    // smuggle U+FFFD into a `-v` argument.
    let socket_dir_str = socket_dir.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "socket dir {} contains non-UTF-8 bytes; cannot pass to docker -v",
            socket_dir.display(),
        )
    })?;
    let socket_mount = format!("{socket_dir_str}:/jackin/run");
    run_args.extend_from_slice(&["-v", &socket_mount]);
    // Bind the host-shared usage cache RW and point the capsule's shared-dir env
    // at subdirectories under it, so the account-keyed snapshot/cooldown/lock
    // files live on one host-shared volume across all containers (Class III). The capsule
    // `create_dir_all`s the subdirectories on first write.
    let usage_shared_str = usage_shared_dir.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "usage-shared dir {} contains non-UTF-8 bytes; cannot pass to docker -v",
            usage_shared_dir.display(),
        )
    })?;
    let usage_shared_mount = format!("{usage_shared_str}:/jackin/usage-shared");
    run_args.extend_from_slice(&[
        "-v",
        &usage_shared_mount,
        "-e",
        "JACKIN_USAGE_SNAPSHOTS_DIR=/jackin/usage-shared/snapshots",
        "-e",
        "JACKIN_USAGE_COOLDOWN_DIR=/jackin/usage-shared/cooldowns",
        "-e",
        "JACKIN_USAGE_LOCK_DIR=/jackin/usage-shared/locks",
    ]);
    // Mount the host UID/GID entries where libnss-extrausers reads them.
    let extrausers_mounts = if extrausers_entries.is_some() {
        let passwd_mount = extrausers_passwd
            .to_str()
            .map(|p| format!("{p}:/var/lib/extrausers/passwd:ro"));
        let group_mount = extrausers_group
            .to_str()
            .map(|p| format!("{p}:/var/lib/extrausers/group:ro"));
        passwd_mount.into_iter().chain(group_mount).collect()
    } else {
        Vec::new()
    };
    for mount in &extrausers_mounts {
        run_args.extend_from_slice(&["-v", mount.as_str()]);
    }
    jackin_diagnostics::telemetry_debug!(
        "launch",
        "prepared host socket dir {socket_dir_str} (owned by host UID, default umask) and Capsule config for bind-mount at /jackin/run",
    );
    run_args.push(image);
    // Pass the initial agent as the container command argument. The
    // daemon uses it only to choose the first tab; per-session
    // `JACKIN_AGENT` is set later when spawning an actual agent PTY.
    run_args.push(agent.slug());
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "docker_run_role",
        Some(container_name),
    );
    let run_role = runner.run("docker", &run_args, None, &docker_run_opts);
    let run_role_result = if let Some(progress) = steps.progress_mut() {
        progress.while_waiting(run_role).await
    } else {
        run_role.await
    };
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "docker_run_role",
        if run_role_result.is_ok() {
            Some("started")
        } else {
            Some("error")
        },
    );
    if run_role_result.is_err() {
        let span = jackin_diagnostics::operation_span("launch.prepare", &[]);
        span.in_scope(|| {
            jackin_diagnostics::operation_error(
                "launch.prepare",
                "docker_run_failed",
                "role container start failed",
                &[],
            );
        });
    }
    run_role_result?;

    // Privileged post-run capsule steps, each run as root via `docker exec`
    // (needs no setuid, so composes with no-new-privileges) and each fail-closed:
    // a non-zero exit tears the container down rather than start the agent with a
    // control the profile only partially applied.
    //   - WP1 firewall-apply (allowlist tier only): installs the egress allowlist
    //     from JACKIN_ALLOWED_HOSTS; an empty list is itself fail-closed.
    //   - WP-SUDO sudo-provision (sudo-granted profiles only — compat / explicit
    //     `sudo = true`): writes /etc/sudoers.d/agent. The base image bakes no
    //     sudoers, so non-sudo profiles have nothing to provision and skip it.
    let mut post_run_steps: Vec<(String, [&str; 6], String)> = Vec::new();
    if let Some(argv) =
        crate::runtime::docker_profile::firewall_post_run_argv(grants, container_name)
    {
        post_run_steps.push((
            format!("firewall_apply profile={profile}"),
            argv,
            format!("egress allowlist install failed for `{profile}` profile; container torn down (fail-closed). The agent was not started without the firewall the profile promises."),
        ));
    }
    if grants.sudo {
        post_run_steps.push((
            format!("sudo_provision profile={profile}"),
            crate::runtime::docker_profile::sudo_provision_post_run_argv(container_name),
            format!("sudo provisioning failed for `{profile}` profile; container torn down (fail-closed)."),
        ));
    }
    for (label, argv, failure_context) in post_run_steps {
        let result = runner.run("docker", &argv, None, &docker_run_opts).await;
        jackin_diagnostics::telemetry_debug!(
            "launch",
            "{label} exit={}",
            if result.is_ok() { "0" } else { "nonzero" },
        );
        if let Err(err) = result {
            if let Err(remove_err) = docker.remove_container(container_name).await {
                jackin_diagnostics::emit_compact_line(
                    "warning",
                    &format!(
                        "fail-closed teardown could not remove {container_name}: {remove_err}"
                    ),
                );
            }
            return Err(err.context(failure_context));
        }
    }

    // Reconcile keep_awake AFTER the role container is running but
    // BEFORE the foreground session blocks. This is the only window in
    // which an interactive `jackin load` can spawn caffeinate.
    jackin_host::caffeinate::reconcile_when_configured(
        paths,
        docker,
        runner,
        workspace.keep_awake_enabled,
    )
    .await;

    // Pre-session safety check: if jackin-capsule exited immediately
    // (missing binary, bad image), surface the container logs rather than
    // failing with a cryptic docker exec error.
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "pre_attach_exit_check",
        Some(container_name),
    );
    if let Some(err) =
        diagnose_premature_exit(docker, runner, container_name, ExitPhase::PreAttach).await
    {
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Capsule,
            "pre_attach_exit_check",
            Some("exited"),
        );
        return Err(err);
    }
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "pre_attach_exit_check",
        Some("running"),
    );

    // Connect the operator's terminal to the running jackin-capsule multiplexer.
    // The shared reconnect helper first waits for `/jackin/run/jackin.sock`
    // to answer `status`; jackin-capsule detects PID != 1 and then runs in
    // client mode, connecting to that daemon socket inside the container.
    if let Some(progress) = steps.progress_mut() {
        progress.stage_done(crate::runtime::progress::LaunchStage::Capsule, "ready");
        progress.opening_hardline();
        progress.settle_stage_visual().await;
    }
    // Tear down the loading cockpit before the interactive attach: the
    // capsule's `docker exec -it` must own a clean terminal, and leaving the
    // rich surface active would force-capture its PTY and hang the handoff.
    steps.finish_progress();
    if let Some(refresh) = selected_image_refresh {
        crate::runtime::image::spawn_selected_image_refresh(
            paths,
            selector,
            refresh.role_git,
            refresh.branch_override,
            *agent,
            refresh.reason,
            *debug,
        );
    }
    if let Some(sentinel) = reuse_staleness_sentinel {
        crate::runtime::image::spawn_reuse_staleness_sentinel(
            paths,
            selector,
            sentinel.role_git,
            sentinel.branch_override,
            *agent,
            sentinel.image,
            *debug,
        );
    }
    let _sibling_runtime_prewarm = crate::runtime::image::spawn_sibling_runtime_prewarm(
        paths,
        sibling_prewarm.validated_repo,
        *agent,
        sibling_prewarm.selected_image_reused,
    );
    crate::runtime::image::spawn_sibling_image_prewarm(
        paths,
        selector,
        sibling_prewarm.role_git,
        sibling_prewarm.branch_override,
        sibling_prewarm.validated_repo,
        *agent,
        sibling_prewarm.selected_image_reused,
    );
    let _sibling_auth_prewarm =
        spawn_sibling_auth_prewarm(paths, container_name, sibling_auth_prewarm, *agent);
    let session_result = crate::runtime::attach::reconnect_or_create_session_with_focus(
        paths,
        container_name,
        None,
        docker,
        runner,
    )
    .await;
    // Ensure cleanup debug logs start on a fresh line after the interactive session
    eprintln!();
    if let Err(err) = session_result {
        // Single inspect — the previous two-call shape opened a TOCTOU
        // window where the container could transition Running→Stopped(0)
        // between the diagnose and swallow checks. If the attach command
        // itself returned Err, propagate it even when PID 1 exited cleanly:
        // the capsule attach protocol uses that path to report failed final
        // sessions while the daemon still shuts down as init with exit 0.
        let inspect = docker.inspect_container_state(container_name).await;
        if let Some(diag) =
            diagnose_with_state(runner, container_name, &inspect, ExitPhase::PostAttach).await
        {
            return Err(diag);
        }
        // `diagnose_with_state` returned `None`, so PID 1 exited cleanly: the
        // in-capsule dirty-exit modal already made any keep/discard decision and
        // recorded it in exit-action.json. The non-zero `docker exec` result is
        // the attach socket-close race at clean shutdown, not a failed session,
        // so fall through to a successful return — the pipeline then runs
        // finalize, which reads exit-action.json and executes the choice. The
        // attach detail is kept only as a diagnostic breadcrumb.
        let attach_detail = attach_failure_error(container_name, &err);
        jackin_diagnostics::telemetry_debug!(
            "session",
            "clean container exit for {container_name}; proceeding to finalize \
             (attach shutdown detail: {attach_detail})"
        );
        if let Some(run) = jackin_diagnostics::active_run() {
            run.compact(
                jackin_telemetry::schema::events::CAPSULE_SESSION_CLEAN_SHUTDOWN,
                &format!("container {container_name} exited cleanly after session"),
            );
        }
    }
    if let Some(progress) = steps.progress_mut() {
        progress.stage_done(crate::runtime::progress::LaunchStage::Hardline, "open");
    }
    if matches!(
        sidecar_prewarm_replenish,
        SidecarPrewarmReplenish::AfterAttach
    ) {
        crate::runtime::prewarm_trigger::spawn_background_sidecar_prewarm(paths, *debug);
    }

    Ok(())
}

pub(crate) fn host_runtime_passthrough_env(
    vars: impl IntoIterator<Item = (String, String)>,
) -> Vec<String> {
    vars.into_iter()
        .filter_map(|(key, value)| {
            if key.starts_with("JACKIN_DISABLE_")
                || matches!(
                    key.as_str(),
                    "JACKIN_DHAT_ALLOC_LOG" | "JACKIN_CAPSULE_FORCE_PANIC" | "TZ"
                )
            {
                Some(format!("{key}={value}"))
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn debug_runtime_envs(_debug: bool) -> Vec<String> {
    Vec::new()
}

pub(crate) fn telemetry_runtime_envs_for(level: jackin_diagnostics::TelemetryLevel) -> Vec<String> {
    let level = match level {
        jackin_diagnostics::TelemetryLevel::Info => "info",
        jackin_diagnostics::TelemetryLevel::Debug => "debug",
        jackin_diagnostics::TelemetryLevel::Trace => "trace",
    };
    vec![format!("JACKIN_TELEMETRY_LEVEL={level}")]
}

pub(crate) fn telemetry_runtime_envs(debug: bool) -> Vec<String> {
    telemetry_runtime_envs_for(jackin_diagnostics::telemetry_level(debug))
}

pub(crate) fn run_runtime_envs() -> Vec<String> {
    jackin_telemetry::identity::current_invocation().map_or_else(Vec::new, |invocation| {
        vec![format!("JACKIN_INVOCATION_ID={invocation}")]
    })
}
