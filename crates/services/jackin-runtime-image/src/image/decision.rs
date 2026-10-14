// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::published::{
    PublishedImageFreshness, published_image_freshness, published_image_is_stale,
};

use jackin_core::CommandRunner;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::docker_client::DockerApi;

use jackin_image::image_recipe::expected_image_recipes;

use jackin_manifest::repo::CachedRepo;

use jackin_runtime_naming::naming::{image_name, image_name_for_branch, role_base_image_name};

use super::{
    ImageDecision, ImageInvalidationReason, build_decision, classify_image_labels,
    decision_base_image_override, emit_image_decision, emit_image_reuse, role_git_sha_for_recipe,
};

pub fn local_image_build_args() -> Vec<&'static str> {
    vec!["build", "--provenance", "false", "--sbom", "false"]
}

#[expect(
    clippy::too_many_arguments,
    reason = "Decide-role-image call site propagates paths, selector, cached + \
              validated repos, rebuild + branch override + pinned sha, docker, \
              runner. Named-arg reads match the per-input propagation idiom; \
              bundling into a config struct is the deferred-parallel-pass."
)]
pub async fn decide_role_image(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    rebuild: bool,
    branch_override: Option<&str>,
    pinned_sha: Option<&str>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<ImageDecision> {
    let head_sha = role_git_sha_for_recipe(cached_repo, pinned_sha, runner).await;
    let image = branch_override.map_or_else(
        || image_name(selector, head_sha.as_deref()),
        |branch| image_name_for_branch(selector, branch, head_sha.as_deref()),
    );
    let mut base_image_override = decision_base_image_override(validated_repo, branch_override);
    if rebuild {
        emit_image_decision(&image, ImageInvalidationReason::ExplicitRebuild);
        return Ok(ImageDecision::BuildFromWorkspace {
            reason: ImageInvalidationReason::ExplicitRebuild,
            role_git_sha: head_sha,
        });
    }

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "image_tag_lookup",
        Some(image.as_str()),
    );
    let tag_result = docker.list_image_tags(&image).await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "image_tag_lookup",
        if tag_result.is_ok() {
            Some(image.as_str())
        } else {
            Some("error")
        },
    );
    let Ok(tags) = tag_result else {
        let _warning = jackin_telemetry::record_recovered_degradation();
        emit_image_decision(&image, ImageInvalidationReason::ImageListFailed);
        return Ok(build_decision(
            ImageInvalidationReason::ImageListFailed,
            None,
            base_image_override,
        ));
    };
    if tags.is_empty() {
        let mut reason = ImageInvalidationReason::LocalImageMissing;
        if let Some(published) = base_image_override {
            let freshness = published_image_freshness(
                published,
                &validated_repo.dockerfile.construct_version,
                head_sha.as_deref(),
                docker,
            )
            .await;
            let stale = match freshness {
                PublishedImageFreshness::Fresh => false,
                PublishedImageFreshness::Stale => true,
                PublishedImageFreshness::NeedsRoleSha(stored_sha) => {
                    head_sha.as_deref() != Some(stored_sha.as_str())
                }
            };
            if stale {
                base_image_override = None;
                reason = ImageInvalidationReason::PublishedImageStale;
            }
        }
        emit_image_decision(&image, reason);
        return Ok(build_decision(reason, head_sha, base_image_override));
    }

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "image_recipe",
        None,
    );
    let local_base_image = role_base_image_name(selector, branch_override, head_sha.as_deref());
    let expected_recipes = expected_image_recipes(
        cached_repo,
        validated_repo,
        head_sha.as_deref(),
        branch_override,
        Some(local_base_image.as_str()),
        paths,
        &image,
    )?;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "image_recipe",
        Some(&format!("{} expected recipes", expected_recipes.len())),
    );
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "image_label_inspect",
        Some(image.as_str()),
    );
    let label_result = docker.inspect_image_labels(&image).await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "image_label_inspect",
        if label_result.is_ok() {
            Some(image.as_str())
        } else {
            Some("error")
        },
    );
    let Ok(labels) = label_result else {
        let _warning = jackin_telemetry::record_recovered_degradation();
        emit_image_decision(&image, ImageInvalidationReason::InspectFailed);
        return Ok(build_decision(
            ImageInvalidationReason::InspectFailed,
            head_sha,
            base_image_override,
        ));
    };

    match classify_image_labels(&labels, &expected_recipes) {
        None => {
            emit_image_reuse(&image);
            Ok(ImageDecision::Reuse { image })
        }
        Some(reason) => {
            if let Some(published) = base_image_override
                && published_image_is_stale(
                    published,
                    &validated_repo.dockerfile.construct_version,
                    head_sha.as_deref(),
                    docker,
                )
                .await
            {
                base_image_override = None;
            }
            emit_image_decision(&image, reason);
            Ok(build_decision(reason, head_sha, base_image_override))
        }
    }
}
