// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `PrepareInstance` input and instance preparation.

use crate::runtime::launch::launch_pipeline::launch_phases::{ImageMaterialized, InstancePrepared};

use anyhow::Context;
use jackin_docker::docker_client::DockerApi;

use crate::instance::{DockerResources, InstanceManifest, InstanceStatus, NewInstanceManifest};

pub(crate) struct PrepareInstance<'a, D> {
    pub(crate) paths: &'a jackin_core::JackinPaths,
    pub(crate) workspace: &'a jackin_config::ResolvedWorkspace,
    pub(crate) workspace_name: &'a Option<String>,
    pub(crate) container_name: &'a str,
    pub(crate) role_key: &'a str,
    pub(crate) agent_display_name: &'a str,
    pub(crate) agent: jackin_core::Agent,
    pub(crate) source: &'a jackin_config::RoleSource,
    pub(crate) opts: &'a crate::runtime::launch::LoadOptions,
    pub(crate) dind_started: bool,
    pub(crate) dind: &'a str,
    pub(crate) network: &'a str,
    pub(crate) certs_volume: &'a str,
    pub(crate) recipe_role_git_sha: Option<String>,
    pub(crate) recipe_base_image_ref: Option<String>,
    pub(crate) supported_agents: &'a [jackin_core::Agent],
    pub(crate) restoring: bool,
    pub(crate) docker: &'a D,
    pub(crate) cleanup: &'a crate::runtime::launch::LoadCleanup,
    pub(crate) image: ImageMaterialized,
}

pub(crate) async fn prepare_instance<D>(
    input: PrepareInstance<'_, D>,
) -> anyhow::Result<InstancePrepared>
where
    D: DockerApi,
{
    let PrepareInstance {
        paths,
        workspace,
        workspace_name,
        container_name,
        role_key,
        agent_display_name,
        agent,
        source,
        opts,
        dind_started,
        dind,
        network,
        certs_volume,
        recipe_role_git_sha,
        recipe_base_image_ref,
        supported_agents,
        restoring,
        docker,
        cleanup,
        image: ImageMaterialized {
            image,
            selected_image_reused,
        },
    } = input;
    let host_workdir_fingerprint =
        crate::runtime::launch::manifest_host_workdir_fingerprint(workspace);
    let new_manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: container_name,
        workspace_name: workspace_name.as_deref(),
        workspace_label: workspace.label.as_str(),
        workdir: &workspace.workdir,
        host_workdir_fingerprint: &host_workdir_fingerprint,
        role_key,
        role_display_name: agent_display_name,
        agent_runtime: agent,
        role_source_git: &source.git,
        role_source_ref: opts.role_branch.as_deref(),
        image_tag: &image,
        docker: DockerResources {
            role_container: container_name.to_owned(),
            dind_container: dind_started.then(|| dind.to_owned()),
            network: network.to_owned(),
            certs_volume: dind_started.then(|| certs_volume.to_owned()),
        },
        role_git_sha: recipe_role_git_sha,
        base_image_ref: recipe_base_image_ref,
        base_image_digest: None,
        supported_agents: supported_agents.to_vec(),
    });
    let container_state = paths.data_dir.join(container_name);
    // The launch was admitted before `prepare_instance`; migrate any old
    // isolation envelope while its independent v3 manifest witness is still
    // on disk. Inspection and restore-candidate reads stay non-mutating.
    if let Err(error) = crate::isolation::state::migrate_records(&container_state)
        .context("cannot migrate admitted isolation state before manifest replacement")
    {
        cleanup.run(docker).await;
        return Err(error);
    }
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
    if let Err(error) = crate::runtime::launch::write_instance_status(
        paths,
        &container_state,
        &mut instance_manifest,
        InstanceStatus::Active,
    ) {
        cleanup.run(docker).await;
        return Err(error);
    }
    Ok(InstancePrepared {
        image,
        selected_image_reused,
        instance_manifest,
        container_state,
        host_workdir_fingerprint,
    })
}
