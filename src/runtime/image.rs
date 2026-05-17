use crate::derived_image::create_derived_build_context;
use crate::docker::{CommandRunner, RunOptions};
use crate::paths::JackinPaths;
use crate::repo::CachedRepo;
use crate::selector::RoleSelector;
use crate::version_check;
use owo_colors::OwoColorize;

use super::identity::HostIdentity;
use super::naming::{LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION, image_name};

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
    // Skip the pre-built image when JACKIN_CONSTRUCT_IMAGE points at a local
    // build: the published image was built against the canonical construct, so
    // using it as base would silently ignore the local construct override.
    let custom_construct =
        crate::repo_contract::construct_image() != crate::repo_contract::CONSTRUCT_IMAGE;
    let mut use_prebuilt =
        published_image.is_some() && !rebuild && branch_override.is_none() && !custom_construct;
    let mut base_image_override = use_prebuilt.then(|| published_image.unwrap());

    // When using the pre-built published image, verify it was built from the
    // same construct version the Dockerfile now pins. A mismatch means the
    // published image pre-dates a Renovate update (the role Dockerfile was
    // bumped to a newer construct version but CI has not yet rebuilt the
    // published image). Fall back to workspace mode so the role's workspace
    // Dockerfile — which carries the new pinned version — is used directly.
    let rebuild = if use_prebuilt
        && construct_version_is_stale(
            published_image.unwrap(),
            &validated_repo.dockerfile.construct_version,
            debug,
            runner,
        ) {
        use_prebuilt = false;
        base_image_override = None;
        true
    } else {
        rebuild
    };

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
    // If a derived image already exists locally, check whether it was built
    // against the same construct image as the current invocation. A mismatch
    // means the cached image is tainted — e.g. built with a local construct
    // override while this invocation uses the canonical one, or vice versa —
    // and must be rebuilt from scratch rather than reused.
    let current_construct = crate::repo_contract::construct_image();
    let cached_construct_label = runner
        .capture(
            "docker",
            &[
                "inspect",
                "--format",
                &format!("{{{{index .Config.Labels \"{LABEL_IMAGE_CONSTRUCT}\"}}}}"),
                &image,
            ],
            None,
        )
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let construct_mismatch = cached_construct_label
        .as_deref()
        .is_some_and(|cached| cached != current_construct);
    let rebuild = rebuild || construct_mismatch;

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

    let construct_label = format!("{LABEL_IMAGE_CONSTRUCT}={current_construct}");
    build_args.extend(["--build-arg", &build_arg_uid]);
    build_args.extend(["--build-arg", &build_arg_gid]);
    build_args.extend(["--build-arg", &cache_bust]);
    build_args.extend(["--label", &construct_label]);
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

    extract_agent_version(paths, &image, agent, debug, runner);

    Ok(image)
}

/// Returns true when the published image's construct version label differs from
/// the Dockerfile's pinned version, meaning the published image pre-dates a
/// Renovate bump. If the label is absent the image predates this tracking
/// feature — treat as fresh so existing published images keep working.
fn construct_version_is_stale(
    published: &str,
    dockerfile_version: &str,
    debug: bool,
    runner: &mut impl CommandRunner,
) -> bool {
    if let Err(e) = runner.run(
        "docker",
        &["pull", "--quiet", published],
        None,
        &RunOptions::default(),
    ) && debug
    {
        eprintln!(
            "{}",
            format!("[debug] docker pull {published} failed ({e}); staleness check will use cached digest")
                .dimmed()
        );
    }
    let label_stored = runner
        .capture(
            "docker",
            &[
                "inspect",
                "--format",
                &format!("{{{{index .Config.Labels \"{LABEL_IMAGE_CONSTRUCT_VERSION}\"}}}}"),
                published,
            ],
            None,
        )
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    label_stored.is_some_and(|stored| stored != dockerfile_version)
}

fn extract_agent_version(
    paths: &JackinPaths,
    image: &str,
    agent: crate::agent::Agent,
    debug: bool,
    runner: &mut impl CommandRunner,
) {
    match agent {
        crate::agent::Agent::Claude => {
            let Ok(version) = runner.capture(
                "docker",
                &["run", "--rm", "--entrypoint", "claude", image, "--version"],
                None,
            ) else {
                return;
            };
            let version = version.trim();
            if !version.is_empty() {
                if debug {
                    eprintln!("        Claude {version}");
                }
                if let Some(semver) = version_check::parse_claude_version(version) {
                    version_check::store_image_version(paths, image, semver);
                } else if debug {
                    eprintln!("warning: unexpected claude --version output: {version:?}");
                }
            }
        }
        crate::agent::Agent::Opencode => {
            let Ok(version) = runner.capture(
                "docker",
                &[
                    "run",
                    "--rm",
                    "--entrypoint",
                    "opencode",
                    image,
                    "--version",
                ],
                None,
            ) else {
                return;
            };
            let version = version.trim();
            if !version.is_empty() {
                if debug {
                    eprintln!("        OpenCode {version}");
                }
                if let Some(semver) = version_check::parse_opencode_version(version) {
                    version_check::store_opencode_version(paths, image, semver);
                } else if debug {
                    eprintln!("warning: unexpected opencode --version output: {version:?}");
                }
            }
        }
        crate::agent::Agent::Kimi => {
            let Ok(version) = runner.capture(
                "docker",
                &["run", "--rm", "--entrypoint", "kimi", image, "--version"],
                None,
            ) else {
                return;
            };
            let version = version.trim();
            if !version.is_empty() {
                if debug {
                    eprintln!("        Kimi {version}");
                }
                if let Some(semver) = version_check::parse_kimi_version(version) {
                    version_check::store_kimi_version(paths, image, semver);
                } else if debug {
                    eprintln!("warning: unexpected kimi --version output: {version:?}");
                }
            }
        }
        _ => {}
    }
}
