//! Launch-core phase contracts and injectable failure-path helpers
//! (R-launch-typestate / R-033-suite-a).
//!
//! Pure grant resolution is separated from I/O so suite A can prove
//! grant-failure ordering and `FailedSetup` teardown without constructing a
//! full ~20-crate `LaunchCore` graph.
//!
//! The ten pipeline phases (roadmap Ownership item 1) are typed `#[must_use]`
//! outputs consumed by value by the next phase. Helper-only substitutes are
//! not acceptance — see `run_launch_core` + the `launch_pipeline` harness.

use crate::instance::{InstanceManifest, InstanceStatus};
use crate::runtime::docker_profile::{
    DockerGrants, DockerSecurityProfile, EffectiveGrants, ProfileSource, dind_enabled,
    fold_role_grants, profile_meets_floor, resolve_effective_grants, resolve_profile,
    validate_effective_grants,
};
use jackin_config::{AppConfig, WorkspaceDockerConfig};
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::docker_client::DockerApi;
use jackin_manifest::RoleManifest;

use super::super::LoadCleanup;
use super::{bail_on_grant_errors, tag_errors, tagged_grant_errors};

// ── Phase 1: validation ────────────────────────────────────────────────────

/// Pure classification of an image decision for the launch typestate chain
/// (`GrantsValidated` → [`ImagePhaseClassified`] → materialize → run).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePhaseClass {
    /// Local image is reusable (or background refresh only).
    ReuseOrBackgroundRefresh,
    /// Derived image must be built before container create.
    BuildRequired,
}

/// Classified image phase after grants (injectable; no Docker I/O).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImagePhaseClassified {
    /// Whether the pipeline builds or reuses.
    pub class: ImagePhaseClass,
    /// True when the selected agent image is reused without a foreground build.
    pub selected_image_reused: bool,
}

/// Classify [`jackin_image::image_decision::ImageDecision`] without I/O.
#[must_use]
pub fn classify_image_phase(
    decision: &crate::runtime::image::ImageDecision,
) -> ImagePhaseClassified {
    match decision {
        crate::runtime::image::ImageDecision::Reuse { .. }
        | crate::runtime::image::ImageDecision::RefreshInBackground { .. } => {
            ImagePhaseClassified {
                class: ImagePhaseClass::ReuseOrBackgroundRefresh,
                selected_image_reused: true,
            }
        }
        crate::runtime::image::ImageDecision::BuildFromPublished { .. }
        | crate::runtime::image::ImageDecision::BuildFromWorkspace { .. } => ImagePhaseClassified {
            class: ImagePhaseClass::BuildRequired,
            selected_image_reused: false,
        },
    }
}

/// Grant + profile resolution succeeded (phase 1 → 2).
#[derive(Debug, Clone)]
pub struct GrantsValidated {
    /// Folded effective grants after config/workspace/role layers.
    pub effective_grants: EffectiveGrants,
    /// Resolved Docker security profile.
    pub resolved_profile: DockerSecurityProfile,
    /// Where the profile came from (CLI / workspace / config / default).
    pub profile_source: ProfileSource,
    /// Whether `DinD` is enabled under the effective grants.
    pub dind_started: bool,
}

/// Inputs for the pure grant-validation phase (no Docker I/O).
#[derive(Debug)]
pub struct GrantPhaseInput<'a> {
    /// Host app config (global docker grants + profile).
    pub config: &'a AppConfig,
    /// Workspace label used for workspace-layer docker grants lookup.
    pub workspace_label: &'a str,
    /// Optional pre-resolved workspace docker table (avoids re-lookup).
    pub workspace_docker: Option<&'a WorkspaceDockerConfig>,
    /// CLI `--docker-profile` override.
    pub opts_docker_profile: Option<DockerSecurityProfile>,
    /// Role selector (error messages).
    pub selector: &'a RoleSelector,
    /// Role manifest (min profile + role grants).
    pub role_manifest: &'a RoleManifest,
}

/// Validate config/workspace/role docker grants and fold effective grants.
///
/// Returns [`GrantsValidated`] on success. On failure the caller must run
/// [`LoadCleanup::run`] / [`cleanup_after_grant_failure`] before returning
/// the error (suite A ordering).
///
/// # Errors
/// Returns when any layer fails grants validation or profile floor check.
pub fn validate_launch_grants(input: GrantPhaseInput<'_>) -> anyhow::Result<GrantsValidated> {
    let workspace_docker_for_grants = input.workspace_docker.or_else(|| {
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

// ── Phase outputs (by-value handoff tokens) ────────────────────────────────

/// Image materialization finished (reuse or build).
#[derive(Debug, Clone)]
#[must_use]
pub(crate) struct ImageMaterialized {
    /// Local image tag to run.
    pub image: String,
    /// True when the selected agent image was reused without a foreground build.
    pub selected_image_reused: bool,
}

/// Instance manifest written with `Active` status (post-materialize identity).
#[derive(Debug)]
#[must_use]
pub(crate) struct InstancePrepared {
    /// On-disk instance identity for the launch.
    pub instance_manifest: InstanceManifest,
    /// Per-container state directory under `paths.data_dir`.
    pub container_state: std::path::PathBuf,
    /// Host workdir fingerprint captured into the manifest.
    pub host_workdir_fingerprint: String,
}

/// Credentials, GitHub env, and `RoleState` prepared for the container.
#[derive(Debug)]
#[must_use]
pub(crate) struct EnvironmentResolved {
    /// Prepared per-agent home/auth state.
    pub state: crate::instance::RoleState,
    /// Resolved GitHub env map (may be empty under Ignore).
    pub github_resolved_env: std::collections::BTreeMap<String, String>,
    /// Saved-workspace name when present (empty string for ad-hoc).
    pub workspace_name_str: String,
    /// Optional saved workspace name for config lookups.
    pub workspace_opt: Option<jackin_core::WorkspaceName>,
    /// GitHub auth mode in effect.
    pub github_mode: jackin_config::GithubAuthMode,
    /// Declared `[github.env]` layers (for operator breadcrumb).
    pub github_env_decls: std::collections::BTreeMap<String, jackin_config::EnvValue>,
}

/// Trust seeding and operator-facing auth breadcrumbs completed.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub(crate) struct TrustSeeded;

/// Workspace mounts materialized and network/DinD sidecar ready.
#[derive(Debug)]
#[must_use]
pub(crate) struct WorkspaceMaterialized {
    /// Materialized mount table for docker `-v` flags.
    pub materialized: crate::isolation::materialize::MaterializedWorkspace,
    /// Capsule launch config (bindings filled by the caller).
    pub launch_config: jackin_protocol::CapsuleConfig,
}

/// Docker (or apple-container) runtime launch completed successfully.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub(crate) struct RuntimeLaunched;

/// Foreground attach finalization decision.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub(crate) struct SessionFinalized {
    /// Isolation finalizer decision (clean / preserved / return-to-agent).
    pub decision: crate::isolation::finalize::FinalizeDecision,
}

/// Teardown classification finished; container name is the pipeline result.
#[derive(Debug, Clone)]
#[must_use]
pub(crate) struct CleanupClassified {
    /// Final container name returned to the caller.
    pub container_name: String,
}

// ── Failure helpers ────────────────────────────────────────────────────────

/// Mid-pipeline failure: mark `FailedSetup` (best-effort) then run cleanup.
///
/// Order is intentional — suite A asserts cleanup Docker ops still run even
/// when the status write fails or is skipped.
pub(crate) async fn mark_failed_setup_then_cleanup(
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

/// Grant-failure path: cleanup only (no `FailedSetup` — instance may not exist yet).
pub async fn cleanup_after_grant_failure(cleanup: &LoadCleanup, docker: &impl DockerApi) {
    cleanup.run(docker).await;
}

#[cfg(test)]
mod tests;
