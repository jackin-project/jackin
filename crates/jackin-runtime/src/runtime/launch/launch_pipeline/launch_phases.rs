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
mod suite_a_tests {
    use super::*;
    use crate::instance::{
        DockerResources, InstanceManifest, NewInstanceManifest,
    };
    use jackin_config::AppConfig;
    use jackin_core::agent::Agent;
    use jackin_core::paths::JackinPaths;
    use jackin_core::selector::RoleSelector;
    use jackin_test_support::FakeDockerClient;
    use std::collections::VecDeque;
    use tempfile::tempdir;

    fn test_manifest(container: &str) -> InstanceManifest {
        let role_source_git = "https://example.invalid/agent-smith.git";
        InstanceManifest::new(NewInstanceManifest {
            container_base: container,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: Agent::Claude,
            role_source_git,
            role_source_ref: None,
            image_tag: "projectjackin/agent-smith:test",
            docker: DockerResources::from_container_name(container),
            role_git_sha: None,
            base_image_ref: None,
            base_image_digest: None,
            supported_agents: vec![],
        })
    }

    #[test]
    fn grant_phase_rejects_root_sudo_without_docker_io() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        crate::runtime::test_support::install_all_test_stubs(&paths);
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        config.docker.grants = Some(DockerGrants {
            user: Some("root".to_owned()),
            sudo: Some(true),
            ..Default::default()
        });
        let selector = RoleSelector::new(None, "agent-smith");
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            "version = \"v1alpha4\"\ndockerfile = \"Dockerfile\"\nagents = [\"claude\"]\n\n[claude]\n",
        )
        .unwrap();
        std::fs::write(
            manifest_temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:0.1-trixie\n",
        )
        .unwrap();
        let role_manifest =
            jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

        let err = validate_launch_grants(GrantPhaseInput {
            config: &config,
            workspace_label: "workspace",
            workspace_docker: None,
            opts_docker_profile: None,
            selector: &selector,
            role_manifest: &role_manifest,
        })
        .unwrap_err();
        assert!(
            err.to_string().contains("docker grants validation failed"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn grant_failure_cleanup_removes_adopted_sidecar_resources() {
        let docker = FakeDockerClient::default();
        let cleanup = LoadCleanup::new(
            "jk-role".into(),
            "jk-role-dind".into(),
            "jk-role-certs".into(),
            "jk-role-net".into(),
            std::env::temp_dir().join("jackin-suite-a-sock"),
        );
        cleanup_after_grant_failure(&cleanup, &docker).await;
        let recorded = docker.recorded.borrow();
        assert!(
            recorded.iter().any(|c| c == "docker rm -f jk-role-dind"),
            "grant-failure cleanup must remove DinD; recorded: {recorded:?}"
        );
        assert!(
            recorded.iter().any(|c| c == "docker network rm jk-role-net"),
            "grant-failure cleanup must remove network; recorded: {recorded:?}"
        );
        assert!(
            recorded
                .iter()
                .any(|c| c == "docker volume rm jk-role-certs"),
            "grant-failure cleanup must remove certs volume; recorded: {recorded:?}"
        );
    }

    #[tokio::test]
    async fn mid_pipeline_failed_setup_still_runs_cleanup() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        crate::runtime::test_support::install_all_test_stubs(&paths);
        let container = "jk-failed-setup-suite-a";
        let container_state = paths.data_dir.join(container);
        std::fs::create_dir_all(&container_state).unwrap();
        let mut manifest = test_manifest(container);
        manifest
            .write(&container_state)
            .unwrap();

        let docker = FakeDockerClient {
            // cleanup will inspect/rm regardless
            inspect_queue: std::cell::RefCell::new(VecDeque::new()),
            ..Default::default()
        };
        let cleanup = LoadCleanup::new(
            container.into(),
            format!("{container}-dind"),
            format!("{container}-certs"),
            format!("{container}-net"),
            paths.jackin_home.join("sockets").join(container),
        );

        mark_failed_setup_then_cleanup(
            &paths,
            &container_state,
            container,
            &mut manifest,
            &cleanup,
            &docker,
            "workspace materialization",
        )
        .await;

        let reloaded = InstanceManifest::read(&container_state).unwrap();
        assert_eq!(
            reloaded.status,
            InstanceStatus::FailedSetup,
            "mid-pipeline failure must stamp FailedSetup"
        );
        let recorded = docker.recorded.borrow();
        assert!(
            recorded
                .iter()
                .any(|c| c == &format!("docker rm -f {container}-dind")),
            "FailedSetup path must still tear down DinD; recorded: {recorded:?}"
        );
    }

    #[test]
    fn typestate_grants_validated_carries_dind_flag() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        crate::runtime::test_support::install_all_test_stubs(&paths);
        let config = AppConfig::load_or_init(&paths).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let manifest_temp = tempdir().unwrap();
        std::fs::write(
            manifest_temp.path().join("jackin.role.toml"),
            "version = \"v1alpha4\"\ndockerfile = \"Dockerfile\"\nagents = [\"claude\"]\n\n[claude]\n",
        )
        .unwrap();
        std::fs::write(
            manifest_temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:0.1-trixie\n",
        )
        .unwrap();
        let role_manifest =
            jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

        let validated = validate_launch_grants(GrantPhaseInput {
            config: &config,
            workspace_label: "workspace",
            workspace_docker: None,
            opts_docker_profile: None,
            selector: &selector,
            role_manifest: &role_manifest,
        })
        .expect("default grants valid");
        // Typestate surface: fields exist for later phases to consume.
        let _profile = format!("{:?}", validated.resolved_profile);
        let _source = format!("{}", validated.profile_source);
        let _dind = validated.dind_started;
        drop(validated.effective_grants);
    }
}
