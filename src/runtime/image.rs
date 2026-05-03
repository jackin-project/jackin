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
    harness: crate::harness::Harness,
    rebuild: bool,
    debug: bool,
    runner: &mut impl CommandRunner,
    repo_lock: std::fs::File,
) -> anyhow::Result<String> {
    // create_derived_build_context copies the repo into a temp directory,
    // creating an immutable snapshot.  After this point the shared cached
    // repo can be safely modified by a parallel load.
    let build = create_derived_build_context(&cached_repo.repo_dir, validated_repo)?;
    drop(repo_lock);

    if debug {
        eprintln!(
            "{}",
            format!(
                r"[debug] DerivedDockerfile ({}):
{}",
                build.dockerfile_path.display(),
                std::fs::read_to_string(&build.dockerfile_path).unwrap_or_default()
            )
            .dimmed()
        );
    }
    let image = image_name(selector);

    let build_arg_uid = format!("JACKIN_HOST_UID={}", host.uid);
    let build_arg_gid = format!("JACKIN_HOST_GID={}", host.gid);
    // Always pass the cache-bust arg so Docker matches the correct layer.
    //
    // When rebuilding (update available / --rebuild), generate a fresh
    // timestamp to invalidate the cached Claude Code install layer, and
    // persist it so subsequent non-rebuild builds reuse the same layer.
    //
    // When NOT rebuilding, replay the stored bust value.  Without this,
    // Docker resolves the Dockerfile default `JACKIN_CACHE_BUST=0` and
    // hits the original pre-bust layer, causing the installed Claude
    // version to ping-pong between old and new on alternate launches.
    let cache_bust_value = if rebuild {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
            .to_string();
        version_check::store_cache_bust(paths, &image, &ts);
        ts
    } else {
        version_check::stored_cache_bust(paths, &image).unwrap_or_else(|| "0".to_string())
    };
    let cache_bust = format!("JACKIN_CACHE_BUST={cache_bust_value}");
    let dockerfile_path = build.dockerfile_path.display().to_string();
    let context_dir = build.context_dir.display().to_string();

    let mut build_args: Vec<&str> = vec![
        "build",
        "--pull",
        "--build-arg",
        &build_arg_uid,
        "--build-arg",
        &build_arg_gid,
        "--build-arg",
        &cache_bust,
    ];
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
    if harness == crate::harness::Harness::Claude
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
