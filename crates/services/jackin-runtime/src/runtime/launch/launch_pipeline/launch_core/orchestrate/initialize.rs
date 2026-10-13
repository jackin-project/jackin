// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Launch initialization inputs and outputs.

use crate::runtime::launch::launch_pipeline::launch_phases::{
    GrantsValidated, ImagePhaseClassified,
};

use anyhow::Context;
use jackin_core::ContainerId;
use jackin_docker::docker_client::DockerApi;

use crate::instance::DockerResources;

use crate::runtime::docker_profile::{DockerSecurityProfile, EffectiveGrants, ProfileSource};

pub(crate) struct InitializeLaunch<'a, D> {
    pub(crate) paths: &'a jackin_core::JackinPaths,
    pub(crate) config: &'a jackin_config::AppConfig,
    pub(crate) selector: &'a jackin_core::RoleSelector,
    pub(crate) workspace: &'a jackin_config::ResolvedWorkspace,
    pub(crate) docker: &'a D,
    pub(crate) opts: &'a crate::runtime::launch::LoadOptions,
    pub(crate) validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    pub(crate) image_decision: &'a crate::runtime::image::ImageDecision,
    pub(crate) container_name: &'a str,
}

pub(crate) struct LaunchInitialized {
    pub(crate) adopted_sidecar_was_used: bool,
    pub(crate) network: String,
    pub(crate) dind: String,
    pub(crate) certs_volume: String,
    pub(crate) cleanup: crate::runtime::launch::LoadCleanup,
    pub(crate) effective_grants: EffectiveGrants,
    pub(crate) resolved_profile: (DockerSecurityProfile, ProfileSource),
    pub(crate) dind_started: bool,
    pub(crate) image_phase: ImagePhaseClassified,
}

pub(crate) async fn initialize_launch<D: DockerApi>(
    input: InitializeLaunch<'_, D>,
) -> anyhow::Result<LaunchInitialized> {
    let InitializeLaunch {
        paths,
        config,
        selector,
        workspace,
        docker,
        opts,
        validated_repo,
        image_decision,
        container_name,
    } = input;
    let container_id = ContainerId::parse(container_name).context("validating container name")?;
    let GrantsValidated {
        effective_grants,
        resolved_profile,
        profile_source,
        dind_started,
    } = crate::runtime::launch::launch_pipeline::launch_phases::validate_launch_grants(
        crate::runtime::launch::launch_pipeline::launch_phases::GrantPhaseInput {
            config,
            workspace_label: workspace.label.as_str(),
            workspace_docker: None,
            opts_docker_profile: opts.docker_profile,
            selector,
            role_manifest: &validated_repo.manifest,
        },
    )?;
    let adopted = if dind_started {
        crate::runtime::launch::adopt_prewarmed_dind_sidecar(paths, docker).await
    } else {
        None
    };
    let adopted_sidecar_was_used = adopted.is_some();
    let resources = adopted.as_ref().map_or_else(
        || DockerResources::from_container_id(&container_id),
        |sidecar| DockerResources {
            role_container: container_name.to_owned(),
            dind_container: Some(sidecar.sidecar.dind.clone()),
            network: sidecar.sidecar.network.clone(),
            certs_volume: Some(sidecar.sidecar.certs_volume.clone()),
        },
    );
    let network = resources.network;
    let dind = resources
        .dind_container
        .unwrap_or_else(|| crate::instance::naming::dind_container_name(container_name));
    let certs_volume = resources
        .certs_volume
        .unwrap_or_else(|| crate::instance::naming::dind_certs_volume(container_name));
    let cleanup = crate::runtime::launch::LoadCleanup::new(
        container_name.to_owned(),
        dind.clone(),
        certs_volume.clone(),
        network.clone(),
        paths.jackin_home.join("sockets").join(container_name),
    );
    if let Some(sidecar) = adopted.as_ref() {
        cleanup.set_dind_handle(sidecar.dind_handle.clone());
    }
    cleanup.set_dind_required(adopted_sidecar_was_used || dind_started);
    Ok(LaunchInitialized {
        adopted_sidecar_was_used,
        network,
        dind,
        certs_volume,
        cleanup,
        effective_grants,
        resolved_profile: (resolved_profile, profile_source),
        dind_started,
        image_phase: crate::runtime::launch::launch_pipeline::launch_phases::classify_image_phase(
            image_decision,
        ),
    })
}
