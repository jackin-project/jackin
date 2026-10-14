// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::Agent;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_core::{CommandRunner, RunOptions};
use jackin_docker::docker_client::DockerApi;
use jackin_image::derived_image::{
    create_derived_build_context_for_agents, create_role_base_build_context,
};
use jackin_image::image_decision::ImageInvalidationReason;
use jackin_image::image_recipe::{recipe_labels, supported_set_uses_cache_bust};
use jackin_image::version_check;
use jackin_launch::build_log::DiagnosticsBuildLogSink;
use jackin_manifest::repo::CachedRepo;

use jackin_runtime_naming::naming::{
    LABEL_IMAGE_AGENT_VERSION_PREFIX, LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_ROLE_GIT_SHA, image_name,
    image_name_for_branch, role_base_image_name, short_git_sha,
};
use jackin_runtime_progress::progress::{LaunchProgress, LaunchStage};

use super::{
    PreparedRuntimeBinaries, docker_build_env, dockerfile_body_requests_role_git_sha_arg,
    dockerfile_requests_role_git_sha_arg, emit_build_context_snapshot, emit_image_build_source,
    emit_non_containerd_image_store_note, local_image_build_args, local_image_output_arg,
    local_role_base_labels_match, record_built_agent_version, role_git_sha_for_recipe,
    should_stream_build_output,
};

pub fn should_mint_fresh_cache_bust(rebuild: bool, build_reason: ImageInvalidationReason) -> bool {
    rebuild || build_reason == ImageInvalidationReason::AgentVersionChanged
}

pub fn cache_bust_value_for_build(
    paths: &JackinPaths,
    image: &str,
    manifest: &jackin_core::RoleManifest,
    mint_fresh_cache_bust: bool,
) -> anyhow::Result<String> {
    if !supported_set_uses_cache_bust(manifest) {
        return Ok("unused".to_owned());
    }

    if mint_fresh_cache_bust {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("system clock is before UNIX epoch: {e}"))?
            .as_secs()
            .to_string();
        version_check::store_cache_bust(paths, image, &ts);
        return Ok(ts);
    }

    Ok(version_check::stored_cache_bust(paths, image).unwrap_or_else(|| "0".to_owned()))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Resolving the local role base needs every caller-supplied input \
              (selector, branch + head sha, cached + validated repos, published \
              base, rebuild, debug, docker, runner) to flow through to the pull- \
              or-build branch. Named-arg reads match the per-input propagation \
              idiom the role-base resolver walks."
)]
pub async fn ensure_local_role_base(
    selector: &RoleSelector,
    branch_override: Option<&str>,
    head_sha: Option<&str>,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    published_base: Option<&str>,
    rebuild: bool,
    debug: bool,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    mut progress: Option<&mut LaunchProgress>,
) -> anyhow::Result<String> {
    let construct = jackin_manifest::repo_contract::construct_image();
    let base_name = role_base_image_name(selector, branch_override, head_sha);

    if !rebuild
        && docker
            .list_image_tags(&base_name)
            .await
            .is_ok_and(|tags| !tags.is_empty())
        && docker
            .inspect_image_labels(&base_name)
            .await
            .is_ok_and(|labels| {
                local_role_base_labels_match(
                    &labels,
                    &construct,
                    &validated_repo.dockerfile.construct_version,
                    head_sha,
                )
            })
    {
        return Ok(base_name);
    }

    if let Some(published) = published_base {
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::DerivedImage,
            "tag_role_base",
            Some(&base_name),
        );
        if let Some(p) = progress.as_deref_mut() {
            p.stage_progress(
                LaunchStage::DerivedImage,
                "Tagging published role base image",
            );
        }
        let args = ["tag", published, base_name.as_str()];
        let options = RunOptions {
            capture_stderr: true,
            capture_stdout: true,
            null_stdin: true,
            ..RunOptions::default()
        };
        let result = runner.run("docker", &args, None, &options).await;
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::DerivedImage,
            "tag_role_base",
            if result.is_ok() {
                Some("tagged")
            } else {
                Some("error")
            },
        );
        result?;
        return Ok(base_name);
    }

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "build_role_base",
        Some(&base_name),
    );
    let build = create_role_base_build_context(&cached_repo.repo_dir, validated_repo, None)?;
    let dockerfile_path = build.dockerfile_path.display().to_string();
    let context_dir = build.context_dir.display().to_string();
    let role_sha_label = format!(
        "{LABEL_IMAGE_ROLE_GIT_SHA}={}",
        head_sha.map_or("unknown", short_git_sha)
    );
    let construct_label = format!("{LABEL_IMAGE_CONSTRUCT}={construct}");
    let build_arg_role_git_sha = format!("ROLE_GIT_SHA={}", head_sha.unwrap_or("unknown"));

    let mut args = local_image_build_args();
    let construct_is_locally_built = construct != jackin_manifest::repo_contract::CONSTRUCT_IMAGE;
    if rebuild && !construct_is_locally_built {
        args.push("--pull");
    }
    args.extend(["--label", &role_sha_label, "--label", &construct_label]);
    if dockerfile_requests_role_git_sha_arg(&build.dockerfile_path) {
        args.extend(["--build-arg", &build_arg_role_git_sha]);
    }

    let output_arg = local_image_output_arg(&base_name);
    args.extend([
        "--output",
        &output_arg,
        "-f",
        &dockerfile_path,
        &context_dir,
    ]);

    if let Some(p) = progress.as_deref_mut() {
        p.stage_progress(LaunchStage::DerivedImage, "Building role base image");
    }
    emit_non_containerd_image_store_note(runner).await;
    jackin_diagnostics::build_log::begin();
    let build_options = RunOptions {
        capture_stderr: true,
        capture_stdout: true,
        null_stdin: true,
        stream_captured_output: should_stream_build_output(debug),
        tee_to_build_log: true,
        build_log_sink: Some(Arc::new(DiagnosticsBuildLogSink)),
        extra_env: docker_build_env(),
        ..RunOptions::default()
    };
    let build_future = runner.run("docker", &args, None, &build_options);
    let build_result = match progress.as_deref() {
        Some(p) => p.while_waiting(build_future).await,
        None => build_future.await,
    };
    jackin_diagnostics::build_log::end();
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "build_role_base",
        if build_result.is_ok() {
            Some("built")
        } else {
            Some("error")
        },
    );
    build_result?;
    Ok(base_name)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Building the agent image needs every caller-supplied input \
              (paths, selector, cached + validated repos, agent, runtime \
              binaries, rebuild + reason + base override, debug + branch override, \
              docker, runner, repo_lock, known head sha, progress) to flow into \
              the build pipeline. Named-arg reads match the per-input propagation \
              idiom the image builder walks."
)]
#[expect(
    clippy::too_many_lines,
    reason = "Same justification as the too_many_arguments allow: build-agent- \
              image carries every caller-supplied input through the build \
              pipeline. Inline shape preserves captured-locals across phases."
)]
pub async fn build_agent_image(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agent: Agent,
    runtime_binaries: PreparedRuntimeBinaries,
    rebuild: bool,
    build_reason: ImageInvalidationReason,
    build_base_image_override: Option<&str>,
    debug: bool,
    branch_override: Option<&str>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    repo_lock: jackin_runtime_repo_cache::repo_cache::RepoLock,
    known_head_sha: Option<&str>,
    mut progress: Option<&mut LaunchProgress>,
) -> anyhow::Result<String> {
    let use_prebuilt = build_base_image_override.is_some();
    let build_source_reason = if use_prebuilt {
        "published_image_fresh"
    } else if branch_override.is_some() {
        "branch_override"
    } else if rebuild {
        "rebuild_requested"
    } else {
        build_reason.as_str()
    };

    let head_sha = role_git_sha_for_recipe(cached_repo, known_head_sha, runner).await;

    let local_image_name = branch_override.map_or_else(
        || image_name(selector, head_sha.as_deref()),
        |b| image_name_for_branch(selector, b, head_sha.as_deref()),
    );

    let force_base_rebuild = rebuild;
    let mint_fresh_cache_bust = should_mint_fresh_cache_bust(rebuild, build_reason);

    let local_base = ensure_local_role_base(
        selector,
        branch_override,
        head_sha.as_deref(),
        cached_repo,
        validated_repo,
        build_base_image_override,
        force_base_rebuild,
        debug,
        docker,
        runner,
        progress.as_deref_mut(),
    )
    .await?;
    let base_image_override = Some(local_base.as_str());

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "create_build_context",
        None,
    );
    let agents_to_install = validated_repo.manifest.supported_agents();
    let build_result = create_derived_build_context_for_agents(
        &cached_repo.repo_dir,
        validated_repo,
        base_image_override,
        Some(&runtime_binaries.jackin_capsule_src),
        &agents_to_install,
        &runtime_binaries.agent_installs,
    );
    let build = match build_result {
        Ok(build) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::DerivedImage,
                "create_build_context",
                Some("created"),
            );
            let source = if base_image_override.is_some() {
                "published"
            } else {
                "workspace"
            };
            emit_build_context_snapshot(&build.context_dir, source);
            build
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::DerivedImage,
                "create_build_context",
                Some("error"),
            );
            return Err(error);
        }
    };
    drop(repo_lock);

    let dockerfile_body = std::fs::read_to_string(&build.dockerfile_path).ok();
    if debug {
        let rendered = dockerfile_body.as_deref().unwrap_or("<read failed>");
        jackin_diagnostics::emit_debug_line(
            "image",
            &format!(
                "DerivedDockerfile ({}):\n{rendered}",
                build.dockerfile_path.display(),
            ),
        );
    }
    let requests_role_git_sha = dockerfile_body
        .as_deref()
        .is_some_and(dockerfile_body_requests_role_git_sha_arg);
    let image = local_image_name.clone();

    let build_arg_role_git_sha =
        format!("ROLE_GIT_SHA={}", head_sha.as_deref().unwrap_or("unknown"));
    let build_arg_run_uid = format!(
        "JACKIN_RUN_UID={}",
        jackin_runtime_identity::identity::host_uid().unwrap_or(1000)
    );
    let current_construct = jackin_manifest::repo_contract::construct_image();
    let construct_mismatch = if force_base_rebuild {
        false
    } else {
        docker
            .inspect_image_label(&image, LABEL_IMAGE_CONSTRUCT)
            .await
            .unwrap_or(None)
            .is_some_and(|cached| cached != current_construct)
    };
    let mint_fresh_cache_bust = mint_fresh_cache_bust || construct_mismatch;

    let cache_bust_value = cache_bust_value_for_build(
        paths,
        &image,
        &validated_repo.manifest,
        mint_fresh_cache_bust,
    )?;
    let dockerfile_path = build.dockerfile_path.display().to_string();
    let context_dir = build.context_dir.display().to_string();
    let recipe = jackin_image::image_recipe::build_image_recipe(
        cached_repo,
        validated_repo,
        head_sha.as_deref(),
        branch_override,
        base_image_override,
        &cache_bust_value,
    )?;
    let recipe_hash = recipe.hash()?;
    let recipe_labels = recipe_labels(&recipe, &recipe_hash);

    let mut build_args = local_image_build_args();

    emit_image_build_source(base_image_override, build_source_reason, false);

    let cache_bust = format!("JACKIN_CACHE_BUST={cache_bust_value}");
    build_args.extend(["--build-arg", &build_arg_run_uid]);
    if supported_set_uses_cache_bust(&validated_repo.manifest) {
        build_args.extend(["--build-arg", &cache_bust]);
    }
    if requests_role_git_sha {
        build_args.extend(["--build-arg", &build_arg_role_git_sha]);
    }
    for label in &recipe_labels {
        build_args.extend(["--label", label]);
    }
    let agent_version_labels: Vec<String> = runtime_binaries
        .prefetched_agent_versions
        .iter()
        .map(|(agent, version)| {
            format!(
                "{LABEL_IMAGE_AGENT_VERSION_PREFIX}.{}.version={version}",
                agent.slug()
            )
        })
        .collect();
    for label in &agent_version_labels {
        build_args.extend(["--label", label]);
    }
    let output_arg = local_image_output_arg(&image);
    build_args.extend([
        "--output",
        &output_arg,
        "-f",
        &dockerfile_path,
        &context_dir,
    ]);

    if let Some(ref mut p) = progress {
        p.stage_progress(LaunchStage::DerivedImage, "Building Docker image");
    }
    emit_non_containerd_image_store_note(runner).await;

    jackin_diagnostics::build_log::begin();
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "docker_build",
        None,
    );
    let build_options = RunOptions {
        capture_stderr: true,
        capture_stdout: true,
        null_stdin: true,
        stream_captured_output: should_stream_build_output(debug),
        tee_to_build_log: true,
        build_log_sink: Some(Arc::new(DiagnosticsBuildLogSink)),
        extra_env: docker_build_env(),
        ..RunOptions::default()
    };
    let build_future = runner.run("docker", &build_args, None, &build_options);
    let build_result = match progress.as_deref() {
        Some(p) => p.while_waiting(build_future).await,
        None => build_future.await,
    };
    jackin_diagnostics::build_log::end();
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::DerivedImage,
        "docker_build",
        if build_result.is_ok() {
            Some("built")
        } else {
            Some("error")
        },
    );
    build_result?;

    record_built_agent_version(paths, &image, agent, &runtime_binaries, debug, runner).await;

    Ok(image)
}

pub async fn git_head_sha(
    dir: &std::path::Path,
    runner: &mut impl CommandRunner,
) -> Option<String> {
    let dir_str = dir.display().to_string();
    runner
        .capture("git", &["-C", &dir_str, "rev-parse", "HEAD"], None)
        .await
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}
