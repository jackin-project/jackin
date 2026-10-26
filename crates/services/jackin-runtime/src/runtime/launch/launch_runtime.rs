// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::CommandRunner;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::docker_client::DockerApi;

use crate::instance::RoleState;
use crate::runtime::identity::GitIdentity;

use super::progress_helpers::StepCounter;

mod docker;
mod environment;
mod profile;
mod session;
mod setup;

use crate::runtime::progress::launch_output;

#[expect(
    clippy::struct_excessive_bools,
    reason = "LaunchContext carries independent launch switches (debug, git_coauthor_trailer, git_dco, non_interactive) resolved upstream, not a state machine."
)]
pub(crate) struct LaunchContext<'a, 'manifest> {
    pub(crate) container_name: &'a str,
    pub(crate) ownership: &'a DockerLaunchOwnership<'manifest>,
    pub(crate) role_handle_slot:
        &'a std::sync::Arc<std::sync::Mutex<Option<jackin_core::ContainerHandle>>>,
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
    pub(crate) credential_scope: &'a jackin_protocol::usage_broker::UsageCredentialScope,
    pub(crate) profile: crate::runtime::docker_profile::DockerSecurityProfile,
    pub(crate) profile_source: crate::runtime::docker_profile::ProfileSource,
    pub(crate) grants: &'a crate::runtime::docker_profile::EffectiveGrants,
    pub(crate) github_env: &'a std::collections::BTreeMap<String, String>,
    pub(crate) paths: &'a JackinPaths,
    pub(crate) selected_image_refresh: Option<SelectedImageRefresh<'a>>,
    pub(crate) reuse_staleness_sentinel: Option<ReuseStalenessSentinel<'a>>,
    pub(crate) sidecar_prewarm_replenish: SidecarPrewarmReplenish,
    pub(crate) sibling_prewarm: SiblingPrewarm<'a>,
    pub(crate) sibling_auth_prewarm: SiblingAuthPrewarm<'a>,
    pub(crate) non_interactive: bool,
    pub(crate) account_revision: &'a super::account_identity::AccountConfigRevision,
    pub(crate) entry_claim: Option<&'a crate::runtime::universe::EntryClaim>,
}

pub(crate) struct DockerLaunchOwnership<'a> {
    pub(crate) manifest: std::sync::Mutex<&'a mut crate::instance::InstanceManifest>,
    pub(crate) resources: crate::instance::DockerResources,
    pub(crate) dind_handle_slot:
        std::sync::Arc<std::sync::Mutex<Option<jackin_core::ContainerHandle>>>,
    pub(crate) paths: &'a JackinPaths,
    pub(crate) state_dir: &'a std::path::Path,
}

impl DockerLaunchOwnership<'_> {
    pub(crate) fn persist(&self, role: &jackin_core::ContainerHandle) -> anyhow::Result<()> {
        let dind = self
            .dind_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        anyhow::ensure!(
            role.name() == self.resources.role_container,
            "created role identity does not match launch resources"
        );
        match (&self.resources.dind_container, &dind) {
            (Some(name), Some(handle)) if name == handle.name() => {}
            (None, None) => {}
            _ => anyhow::bail!("launch sidecar ownership was not captured"),
        }
        let mut manifest = self
            .manifest
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        manifest.docker = self.resources.clone();
        manifest.docker_identity = Some(crate::instance::DockerIdentity {
            role_container_id: role.id().to_owned(),
            dind_container_id: dind.map(|handle| handle.id().to_owned()),
        });
        manifest.backend = None;
        let status = manifest.status;
        super::write_instance_status(self.paths, self.state_dir, &mut manifest, status)
    }
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

pub(crate) use jackin_runtime_launch_sibling_auth_prewarm::sibling_auth_prewarm::{
    SiblingAuthPrewarm, await_sibling_auth_prewarm, spawn_sibling_auth_prewarm,
    spawn_sibling_auth_prewarm_with_admitted_instances,
};

fn initial_daemon_argv(
    agent: jackin_core::Agent,
    capsule_config: &jackin_protocol::CapsuleConfig,
) -> &str {
    let slug = agent.slug();
    capsule_config
        .instances
        .iter()
        .find(|id| {
            capsule_config
                .agents
                .get(id.as_str())
                .is_some_and(|candidate| candidate == slug)
        })
        .or_else(|| capsule_config.instances.first())
        .map_or(slug, String::as_str)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LaunchOutcome {
    Detached(jackin_core::ContainerHandle),
    ForegroundSessionEnded(jackin_core::ContainerHandle),
}

pub(crate) async fn launch_role_runtime(
    ctx: &LaunchContext<'_, '_>,
    steps: &mut StepCounter,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<LaunchOutcome> {
    let facts = profile::prepare(ctx, runner).await?;
    steps.next("Launching role").await?;
    steps.done();
    if steps.progress.is_none() {
        launch_output()
            .print_deploying(ctx.agent_display_name)
            .await;
    }
    let prepared = docker::prepare(ctx, &facts).await?;
    let (role_container, prepared_usage_relay) =
        docker::start(ctx, docker, runner, prepared).await?;
    session::apply_post_run_policy(ctx, &role_container, runner, docker).await?;
    let relay = crate::usage_relay::start_docker_tunnel(&role_container, prepared_usage_relay);
    let Some(_usage_relay_guard) = relay.ok() else {
        anyhow::bail!("starting scoped usage stdio tunnel failed");
    };
    session::ensure_ready(ctx, &role_container, runner, docker).await?;
    session::finish(ctx, steps, role_container, docker, runner).await
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

pub(crate) use jackin_runtime_launch_debug_envs::debug_envs::debug_runtime_envs;

pub(crate) fn telemetry_runtime_envs_for(level: jackin_diagnostics::TelemetryLevel) -> Vec<String> {
    let level = match level {
        jackin_diagnostics::TelemetryLevel::Info => "info",
        jackin_diagnostics::TelemetryLevel::Debug => "debug",
        jackin_diagnostics::TelemetryLevel::Trace => "trace",
    };
    vec![format!("JACKIN_TELEMETRY_LEVEL={level}")]
}

#[cfg(test)]
mod dhat_allocation_passthrough_tests {
    use super::host_runtime_passthrough_env;

    #[test]
    fn forwards_explicit_dhat_allocation_switch_without_inventing_one() {
        let forwarded =
            host_runtime_passthrough_env([("JACKIN_DHAT_ALLOC_LOG".to_owned(), "on".to_owned())]);
        assert_eq!(forwarded, ["JACKIN_DHAT_ALLOC_LOG=on"]);

        let absent = host_runtime_passthrough_env(std::iter::empty());
        assert!(
            absent
                .iter()
                .all(|entry| !entry.starts_with("JACKIN_DHAT_ALLOC_LOG="))
        );
    }
}

pub(crate) fn telemetry_runtime_envs(debug: bool) -> Vec<String> {
    telemetry_runtime_envs_for(jackin_diagnostics::telemetry_level(debug))
}

pub(crate) use jackin_runtime_launch_run_envs::run_envs::run_runtime_envs;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CapsuleNetwork {
    Disabled,
    Enabled,
}

impl CapsuleNetwork {
    const fn classify(disabled: bool) -> Self {
        if disabled {
            Self::Disabled
        } else {
            Self::Enabled
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CapsuleEndpoint {
    Missing,
    Unclassified,
    Safe,
}

impl CapsuleEndpoint {
    const fn classify(configured: bool, safe: bool) -> Self {
        if !configured {
            Self::Missing
        } else if safe {
            Self::Safe
        } else {
            Self::Unclassified
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CapsuleAuth {
    HostOnly,
    Complete,
}

impl CapsuleAuth {
    const fn classify(host_configured: bool, capsule_configured: bool) -> Self {
        if host_configured && !capsule_configured {
            Self::HostOnly
        } else {
            Self::Complete
        }
    }
}

pub(crate) const fn capsule_export_coverage(
    network: CapsuleNetwork,
    endpoint: CapsuleEndpoint,
    auth: CapsuleAuth,
) -> jackin_diagnostics::CapsuleExportCoverage {
    use jackin_diagnostics::CapsuleExportCoverage;

    if matches!(network, CapsuleNetwork::Disabled) {
        CapsuleExportCoverage::DisabledNetworkNone
    } else if matches!(endpoint, CapsuleEndpoint::Missing) {
        CapsuleExportCoverage::DisabledNoEndpoint
    } else if matches!(endpoint, CapsuleEndpoint::Unclassified) {
        CapsuleExportCoverage::DisabledUnclassifiedEndpoint
    } else if matches!(auth, CapsuleAuth::HostOnly) {
        CapsuleExportCoverage::DisabledUnclassifiedAuth
    } else {
        CapsuleExportCoverage::Enabled
    }
}

pub(crate) fn capsule_otlp_propagation(
    endpoint: Option<&jackin_diagnostics::ContainerOtlp>,
    capsule_safe_headers: Option<&str>,
    traceparent: Option<&str>,
) -> Vec<String> {
    let Some(endpoint) = endpoint else {
        return Vec::new();
    };
    let mut env = vec![format!("OTEL_EXPORTER_OTLP_ENDPOINT={}", endpoint.endpoint)];
    if let Some(headers) = capsule_safe_headers {
        env.push(format!("OTEL_EXPORTER_OTLP_HEADERS={headers}"));
    }
    if let Some(traceparent) = traceparent {
        env.push(format!("TRACEPARENT={traceparent}"));
    }
    env
}

pub(crate) const fn capsule_otlp_allowlist_host(
    endpoint: Option<&jackin_diagnostics::ContainerOtlp>,
) -> Option<&'static str> {
    if endpoint.is_some() {
        Some("host.docker.internal")
    } else {
        None
    }
}

fn with_admitted_final_docker_spec<T>(
    paths: &JackinPaths,
    state: &RoleState,
    spec: jackin_core::ContainerSpec,
    submit: impl FnOnce(jackin_core::ContainerSpec) -> T,
) -> anyhow::Result<T> {
    super::mounts::ensure_provider_authority_not_writable(
        state,
        &spec.binds,
        &[paths.home_dir.join(".jackin-coordination")],
    )?;
    Ok(submit(spec))
}
