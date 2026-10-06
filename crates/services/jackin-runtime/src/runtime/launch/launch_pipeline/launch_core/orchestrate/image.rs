// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Image build and materialization phases.

use crate::runtime::launch::launch_pipeline::launch_phases::{
    ImageMaterialized, ImagePhaseClassified,
};

use jackin_core::CommandRunner;
use jackin_docker::docker_client::DockerApi;

use std::future::Future;
use std::pin::Pin;

use super::poll_sidecar_while;

pub(crate) struct MaterializeImage<'a, D, R> {
    pub(crate) paths: &'a jackin_core::JackinPaths,
    pub(crate) selector: &'a jackin_core::RoleSelector,
    pub(crate) cached_repo: &'a jackin_manifest::repo::CachedRepo,
    pub(crate) validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    pub(crate) agent: jackin_core::Agent,
    pub(crate) supported_agents: &'a [jackin_core::Agent],
    pub(crate) rebuild: bool,
    pub(crate) opts: &'a crate::runtime::launch::LoadOptions,
    pub(crate) steps: &'a mut crate::runtime::launch::StepCounter,
    pub(crate) docker: &'a D,
    pub(crate) runner: &'a mut R,
    pub(crate) restoring: bool,
    pub(crate) container_name: &'a str,
    pub(crate) repo_lock: &'a mut Option<crate::runtime::repo_cache::RepoLock>,
    pub(crate) cleanup: &'a crate::runtime::launch::LoadCleanup,
    pub(crate) classified: ImagePhaseClassified,
    pub(crate) decision: Option<crate::runtime::image::ImageDecision>,
}

pub(crate) struct BuildImage<'a, D, R> {
    common: MaterializeImage<'a, D, R>,
    reason: crate::runtime::image::ImageInvalidationReason,
    role_git_sha: Option<String>,
    base_image_override: Option<String>,
}

pub(crate) async fn build_image<D, R, S>(
    input: BuildImage<'_, D, R>,
    mut sidecar: Pin<&mut S>,
    early_sidecar_result: &mut Option<anyhow::Result<()>>,
) -> anyhow::Result<ImageMaterialized>
where
    D: DockerApi,
    R: CommandRunner,
    S: Future<Output = anyhow::Result<()>>,
{
    let BuildImage {
        common,
        reason,
        role_git_sha,
        base_image_override,
    } = input;
    crate::runtime::launch::emit_image_materialization_plan(
        false,
        reason.as_str(),
        common.restoring,
        common.container_name,
    );
    common.steps.next("Preparing runtime binaries").await?;
    let image_agents = common.supported_agents.to_vec();
    let binaries = poll_sidecar_while(
        async {
            crate::runtime::image::prepare_runtime_binaries_for_agents(
                common.paths,
                common.validated_repo,
                &image_agents,
                common.steps.progress_mut(),
            )
            .await
        },
        sidecar.as_mut(),
        early_sidecar_result,
    )
    .await;
    let binaries = match binaries {
        Ok(binaries) => binaries,
        Err(error) => {
            common
                .steps
                .stage_error(crate::runtime::progress::LaunchStage::AgentBinaries);
            common.cleanup.run(common.docker).await;
            return Err(error);
        }
    };
    common.steps.next("Preparing derived image").await?;
    let Some(repo_lock) = common.repo_lock.take() else {
        common.cleanup.run(common.docker).await;
        return Err(anyhow::anyhow!("repo lock already consumed"));
    };
    let image = poll_sidecar_while(
        async {
            crate::runtime::image::build_agent_image(
                common.paths,
                common.selector,
                common.cached_repo,
                common.validated_repo,
                common.agent,
                binaries,
                common.rebuild,
                reason,
                base_image_override.as_deref(),
                common.opts.debug,
                common.opts.role_branch.as_deref(),
                common.docker,
                common.runner,
                repo_lock,
                role_git_sha.as_deref(),
                common.steps.progress_mut(),
            )
            .await
        },
        sidecar,
        early_sidecar_result,
    )
    .await;
    match image {
        Ok(image) => {
            common
                .steps
                .stage_done(crate::runtime::progress::LaunchStage::DerivedImage, "built");
            Ok(ImageMaterialized {
                image,
                selected_image_reused: false,
            })
        }
        Err(error) => {
            common
                .steps
                .stage_error(crate::runtime::progress::LaunchStage::DerivedImage);
            common.cleanup.run(common.docker).await;
            Err(error)
        }
    }
}

pub(crate) async fn materialize_image_phase<D, R, S>(
    mut input: MaterializeImage<'_, D, R>,
    sidecar: Pin<&mut S>,
    early_sidecar_result: &mut Option<anyhow::Result<()>>,
) -> anyhow::Result<ImageMaterialized>
where
    D: DockerApi,
    R: CommandRunner,
    S: Future<Output = anyhow::Result<()>>,
{
    let Some(decision) = input.decision.take() else {
        input.cleanup.run(input.docker).await;
        return Err(anyhow::anyhow!("image decision already consumed"));
    };
    match (input.classified.class, decision) {
        (
            crate::runtime::launch::launch_pipeline::launch_phases::ImagePhaseClass::ReuseOrBackgroundRefresh,
            decision @ (crate::runtime::image::ImageDecision::Reuse { .. }
            | crate::runtime::image::ImageDecision::RefreshInBackground { .. }),
        ) => {
            let (image, reason) = match decision {
                crate::runtime::image::ImageDecision::Reuse { image } => {
                    (image, "recipe_hash_match")
                }
                crate::runtime::image::ImageDecision::RefreshInBackground { image, reason } => {
                    (image, reason.as_str())
                }
                _ => unreachable!(),
            };
            crate::runtime::launch::emit_image_materialization_plan(
                true,
                reason,
                input.restoring,
                input.container_name,
            );
            drop(input.repo_lock.take());
            input.steps.stage_skipped(
                crate::runtime::progress::LaunchStage::AgentBinaries,
                "image reused",
            );
            input.steps.stage_done(
                crate::runtime::progress::LaunchStage::DerivedImage,
                "reused local image",
            );
            Ok(ImageMaterialized {
                image,
                selected_image_reused: true,
            })
        }
        (
            crate::runtime::launch::launch_pipeline::launch_phases::ImagePhaseClass::BuildRequired,
            crate::runtime::image::ImageDecision::BuildFromPublished {
                reason,
                role_git_sha,
                base_image,
            },
        ) => {
            build_image(
                BuildImage {
                    common: input,
                    reason,
                    role_git_sha,
                    base_image_override: Some(base_image),
                },
                sidecar,
                early_sidecar_result,
            )
            .await
        }
        (
            crate::runtime::launch::launch_pipeline::launch_phases::ImagePhaseClass::BuildRequired,
            crate::runtime::image::ImageDecision::BuildFromWorkspace {
                reason,
                role_git_sha,
            },
        ) => {
            build_image(
                BuildImage {
                    common: input,
                    reason,
                    role_git_sha,
                    base_image_override: None,
                },
                sidecar,
                early_sidecar_result,
            )
            .await
        }
        _ => {
            input.cleanup.run(input.docker).await;
            Err(anyhow::anyhow!(
                "internal: image phase class does not match ImageDecision variant"
            ))
        }
    }
}
