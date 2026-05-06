use crate::derived_image::create_derived_build_context;
use crate::docker::{CommandRunner, RunOptions};
use crate::paths::JackinPaths;
use crate::repo::CachedRepo;
use crate::selector::RoleSelector;
use crate::version_check;
use owo_colors::OwoColorize;

use super::identity::HostIdentity;
use super::naming::image_name;

/// Build the Docker image for the role. Returns the image name.
#[allow(clippy::similar_names, clippy::too_many_arguments)]
pub(super) fn build_agent_image(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &crate::repo::ValidatedRoleRepo,
    host: &HostIdentity,
    agent: crate::agent::Agent,
    rebuild: bool,
    agent_update: bool,
    debug: bool,
    branch_override: Option<&str>,
    runner: &mut impl CommandRunner,
    repo_lock: std::fs::File,
) -> anyhow::Result<String> {
    // Decide the build mode up front.
    //
    // Pre-built mode: the manifest declares a `published_image` and the
    // caller has not passed `--rebuild`. The heavy workspace layers (apt
    // installs, Rust toolchain, etc.) are already baked into that image; we
    // only need to layer the agent install on top.
    //
    // Workspace mode: either `--rebuild` was requested or no `published_image`
    // is declared. We build from the workspace Dockerfile from scratch.
    let published_image = validated_repo.manifest.published_image.as_deref();
    // Branch builds always use the workspace Dockerfile regardless of
    // `published_image` — the operator is testing uncommitted code that has
    // not been pushed to the registry.
    let use_prebuilt = published_image.is_some() && !rebuild && branch_override.is_none();
    let base_image_override = use_prebuilt.then(|| published_image.unwrap());

    // create_derived_build_context copies the repo into a temp directory,
    // creating an immutable snapshot.  After this point the shared cached
    // repo can be safely modified by a parallel load.
    let build =
        create_derived_build_context(&cached_repo.repo_dir, validated_repo, base_image_override)?;
    drop(repo_lock);

    if debug {
        let dockerfile_body = std::fs::read_to_string(&build.dockerfile_path)
            .unwrap_or_else(|e| format!("<read failed: {e}>"));
        eprintln!(
            "{}",
            format!(
                r"[debug] DerivedDockerfile ({}):
{dockerfile_body}",
                build.dockerfile_path.display(),
            )
            .dimmed()
        );
    }
    let image = branch_override.map_or_else(
        || image_name(selector),
        |b| super::naming::image_name_for_branch(selector, b),
    );

    let build_arg_uid = format!("JACKIN_HOST_UID={}", host.uid);
    let build_arg_gid = format!("JACKIN_HOST_GID={}", host.gid);
    // Always pass the cache-bust arg so Docker matches the correct layer.
    //
    // When rebuilding (update available / --rebuild), generate a fresh
    // timestamp to invalidate the cached agent install layer, and persist it
    // so subsequent non-rebuild builds reuse the same layer.
    //
    // When NOT rebuilding, replay the stored bust value.  Without this,
    // Docker resolves the Dockerfile default `JACKIN_CACHE_BUST=0` and hits
    // the original pre-bust layer, causing the installed agent version to
    // ping-pong between old and new on alternate launches.
    let cache_bust_value = if rebuild || agent_update {
        // System clock before UNIX_EPOCH is essentially impossible, but if it
        // happens we must not silently fall back to 0 — that collapses to the
        // Dockerfile's `JACKIN_CACHE_BUST=0` default and defeats the operator's
        // explicit `--rebuild` request.
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("system clock is before UNIX epoch: {e}"))?
            .as_secs()
            .to_string();
        version_check::store_cache_bust(paths, &image, &ts);
        ts
    } else {
        version_check::stored_cache_bust(paths, &image).unwrap_or_else(|| "0".to_string())
    };
    let cache_bust = format!("JACKIN_CACHE_BUST={cache_bust_value}");
    let dockerfile_path = build.dockerfile_path.display().to_string();
    let context_dir = build.context_dir.display().to_string();

    let mut build_args: Vec<&str> = vec!["build"];

    // --pull semantics:
    //
    // Pre-built mode: pass --pull so Docker always checks the registry for an
    // updated published image. A pull with an unchanged digest is a fast
    // no-op, so this adds negligible overhead while ensuring the local daemon
    // picks up any newly pushed workspace image.
    //
    // Workspace mode with --rebuild: pass --pull to refresh the upstream
    // construct base before rebuilding from the workspace Dockerfile.
    //
    // Workspace mode without --rebuild (no published_image): omit --pull so
    // Docker's layer cache is respected across invocations. The base image is
    // not re-evaluated and heavy apt / toolchain layers stay cached.
    if use_prebuilt || rebuild {
        build_args.push("--pull");
    }

    build_args.extend(["--build-arg", &build_arg_uid]);
    build_args.extend(["--build-arg", &build_arg_gid]);
    build_args.extend(["--build-arg", &cache_bust]);
    build_args.extend(["-t", &image, "-f", &dockerfile_path, &context_dir]);

    runner.run(
        "docker",
        &build_args,
        None,
        &RunOptions {
            capture_stderr: true,
            ..RunOptions::default()
        },
    )?;

    // Extract and store the Claude version from the built image when launching
    // Claude. Codex's V1 update path is explicit `--rebuild`.
    if agent == crate::agent::Agent::Claude
        && let Ok(version) = runner.capture(
            "docker",
            &["run", "--rm", "--entrypoint", "claude", &image, "--version"],
            None,
        )
    {
        let version = version.trim();
        if !version.is_empty() {
            if debug {
                eprintln!("        Claude {version}");
            }
            if let Some(semver) = version_check::parse_claude_version(version) {
                version_check::store_image_version(paths, &image, semver);
            } else if debug {
                eprintln!("warning: unexpected claude --version output: {version:?}");
            }
        }
    }

    Ok(image)
}
