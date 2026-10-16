use crate::instance::{InstanceManifest, InstanceStatus};
use crate::runtime::docker_profile::{
    DockerGrants, DockerSecurityProfile, EffectiveGrants, ProfileSource, WorkflowGrant,
    apply_workflow_grant, dind_enabled, fold_role_grants, profile_meets_floor,
    resolve_effective_grants, resolve_profile, validate_effective_grants,
};
use jackin_config::{AppConfig, WorkspaceDockerConfig};
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::docker_client::DockerApi;
use jackin_manifest::RoleManifest;

use super::super::LoadCleanup;
use super::{bail_on_grant_errors, tag_errors, tagged_grant_errors};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePhaseClass {
    ReuseOrBackgroundRefresh,
    BuildRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImagePhaseClassified {
    pub class: ImagePhaseClass,
    pub selected_image_reused: bool,
}

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

#[derive(Debug, Clone)]
pub struct GrantsValidated {
    pub effective_grants: EffectiveGrants,
    pub resolved_profile: DockerSecurityProfile,
    pub profile_source: ProfileSource,
    pub dind_started: bool,
}

#[derive(Debug)]
pub struct GrantPhaseInput<'a> {
    pub config: &'a AppConfig,
    pub workspace_label: &'a str,
    pub workspace_docker: Option<&'a WorkspaceDockerConfig>,
    pub opts_docker_profile: Option<DockerSecurityProfile>,
    pub selector: &'a RoleSelector,
    pub role_manifest: &'a RoleManifest,
    /// Final cap-only clamp for the headless workflow launch path. `None` on
    /// the manual path, where launch profiles keep their existing meaning.
    pub workflow_grant: Option<&'a WorkflowGrant>,
}

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
    if let Some(grant) = input.workflow_grant {
        effective_grants = apply_workflow_grant(effective_grants, grant)
            .map_err(|err| anyhow::anyhow!("workflow grant rejected launch: {err:#}"))?;
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

#[derive(Debug, Clone)]
#[must_use]
pub(crate) struct ImageMaterialized {
    pub image: String,
    pub selected_image_reused: bool,
}

#[derive(Debug)]
#[must_use]
pub(crate) struct InstancePrepared {
    pub image: String,
    pub selected_image_reused: bool,
    pub instance_manifest: InstanceManifest,
    pub container_state: std::path::PathBuf,
    pub host_workdir_fingerprint: String,
}

#[derive(Debug)]
#[must_use]
pub(crate) struct EnvironmentResolved {
    pub state: crate::instance::RoleState,
    pub github_resolved_env: std::collections::BTreeMap<String, String>,
    pub workspace_name_str: String,
    pub workspace_opt: Option<jackin_core::WorkspaceName>,
    pub github_mode: jackin_config::GithubAuthMode,
    pub github_env_decls: std::collections::BTreeMap<String, jackin_config::EnvValue>,
    pub credential_scope: jackin_protocol::usage_broker::UsageCredentialScope,
}

#[derive(Debug)]
#[must_use]
pub(crate) struct TrustSeeded {
    pub environment: EnvironmentResolved,
    pub instances: Vec<jackin_config::ResolvedInstance>,
}

#[derive(Debug)]
#[must_use]
pub(crate) struct WorkspaceMaterialized {
    pub materialized: crate::isolation::materialize::MaterializedWorkspace,
    pub launch_config: jackin_protocol::CapsuleConfig,
    pub environment: EnvironmentResolved,
}

#[derive(Debug)]
#[must_use]
pub(crate) struct RuntimeLaunched {
    pub instance_manifest: InstanceManifest,
    pub container_state: std::path::PathBuf,
    pub cleanup: LoadCleanup,
    pub account_revision: crate::runtime::launch::account_identity::AccountConfigRevision,
}

#[derive(Debug, Clone, Copy)]
#[must_use]
pub(crate) struct SessionFinalized {
    pub decision: crate::isolation::finalize::FinalizeDecision,
}

#[derive(Debug, Clone)]
#[must_use]
pub(crate) struct CleanupClassified {
    pub container_name: String,
}

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

#[cfg(test)]
mod tests;
