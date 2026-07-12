//! Launch-core phase contracts and injectable failure-path helpers
//! (R-launch-typestate / R-033-suite-a).
//!
//! Pure grant resolution is separated from I/O so suite A can prove
//! grant-failure ordering and FailedSetup teardown without constructing a
//! full ~20-crate `LaunchCore` graph.

use crate::instance::{InstanceManifest, InstanceStatus};
use crate::runtime::docker_profile::{
    DockerGrants, DockerSecurityProfile, EffectiveGrants, ProfileSource, dind_enabled,
    fold_role_grants, profile_meets_floor, resolve_effective_grants, resolve_profile,
    validate_effective_grants,
};
use jackin_config::{AppConfig, WorkspaceDockerConfig};
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_docker::docker_client::DockerApi;
use jackin_manifest::RoleManifest;

use super::super::LoadCleanup;
use super::{bail_on_grant_errors, tag_errors, tagged_grant_errors};

/// Grant + profile resolution succeeded (phase 1 → 2).
#[derive(Debug, Clone)]
pub(super) struct GrantsValidated {
    pub effective_grants: EffectiveGrants,
    pub resolved_profile: DockerSecurityProfile,
    pub profile_source: ProfileSource,
    pub dind_started: bool,
}

/// Inputs for the pure grant-validation phase (no Docker I/O).
pub(super) struct GrantPhaseInput<'a> {
    pub config: &'a AppConfig,
    pub workspace_label: &'a str,
    pub workspace_docker: Option<&'a WorkspaceDockerConfig>,
    pub opts_docker_profile: Option<DockerSecurityProfile>,
    pub selector: &'a RoleSelector,
    pub role_manifest: &'a RoleManifest,
}

/// Validate config/workspace/role docker grants and fold effective grants.
///
/// Returns [`GrantsValidated`] on success. On failure the caller must run
/// [`LoadCleanup::run`] before returning the error (suite A ordering).
pub(super) fn validate_launch_grants(
    input: GrantPhaseInput<'_>,
) -> anyhow::Result<GrantsValidated> {
    let workspace_docker_for_grants = input
        .workspace_docker
        .or_else(|| {
            input
                .config
                .workspaces
                .get(input.workspace_label)
                .and_then(|wc| wc.docker.as_ref())
        });
    let resolved_profile = resolve_profile(
        input.opts_docker_profile,
        workspace_docker_for_grants.and_then(|wd| wd.profile),
        input.config.docker.profile,
    );
    let mut grant_errors = Vec::new();
    if let Some(grants) = input.config.docker.grants.as_ref() {
        grant_errors.extend(tagged_grant_errors("config", grants));
    }
    if let Some(grants) = workspace_docker_for_grants.and_then(|wd| wd.grants.as_ref()) {
        grant_errors.extend(tagged_grant_errors("workspace", grants));
    }
    bail_on_grant_errors(grant_errors)?;

    let mut effective_grants = resolve_effective_grants(
        resolved_profile.0,
        input.config.docker.grants.as_ref(),
        workspace_docker_for_grants.and_then(|wd| wd.grants.as_ref()),
    );
    if let Some(min) = input
        .role_manifest
        .docker
        .as_ref()
        .and_then(|d| d.min_profile)
        && !profile_meets_floor(resolved_profile.0, min)
    {
        anyhow::bail!(
            "role `{}` requires Docker profile `{min}` or more capable; resolved `{}` from {}",
            input.selector.key(),
            resolved_profile.0,
            resolved_profile.1,
        );
    }
    if let Some(docker_cfg) = input.role_manifest.docker.as_ref() {
        let role_grants = DockerGrants {
            dind: docker_cfg.dind,
            allowed_hosts: docker_cfg.allowed_hosts.clone(),
            capabilities_add: docker_cfg.capabilities_add.clone(),
            ..Default::default()
        };
        bail_on_grant_errors(tagged_grant_errors("role", &role_grants))?;
        effective_grants = fold_role_grants(effective_grants, &role_grants);
    }
    bail_on_grant_errors(tag_errors(
        "merged",
        validate_effective_grants(&effective_grants),
    ))?;

    let dind_started = dind_enabled(&effective_grants);
    Ok(GrantsValidated {
        effective_grants,
        resolved_profile: resolved_profile.0,
        profile_source: resolved_profile.1,
        dind_started,
    })
}

/// Mid-pipeline failure: mark `FailedSetup` (best-effort) then run cleanup.
///
/// Order is intentional — suite A asserts cleanup Docker ops still run even
/// when the status write fails or is skipped.
pub(super) async fn mark_failed_setup_then_cleanup(
    paths: &JackinPaths,
    container_state: &std::path::Path,
    container_name: &str,
    manifest: &mut InstanceManifest,
    cleanup: &LoadCleanup,
    docker: &impl DockerApi,
    phase: &str,
) {
    if let Err(status_err) = super::super::write_instance_status(
        paths,
        container_state,
        manifest,
        InstanceStatus::FailedSetup,
    ) {
        let message = format!(
            "jackin: warning: failed to mark FailedSetup for {container_name} \
             after {phase}: {status_err:#}; on-disk status may be stale"
        );
        if let Some(run) = jackin_diagnostics::active_run() {
            run.compact("status", &message);
        }
    }
    cleanup.run(docker).await;
}

/// Grant-failure path: cleanup only (no FailedSetup — instance may not exist yet).
pub(super) async fn cleanup_after_grant_failure(
    cleanup: &LoadCleanup,
    docker: &impl DockerApi,
) {
    cleanup.run(docker).await;
}

#[cfg(test)]
#[path = "launch_phases/tests.rs"]
mod suite_a_tests;
