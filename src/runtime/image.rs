use crate::capsule_binary;
use crate::derived_image::create_derived_build_context;
use crate::docker::{CommandRunner, RunOptions};
use crate::docker_client::DockerApi;
use crate::paths::JackinPaths;
use crate::repo::CachedRepo;
use crate::selector::RoleSelector;
use crate::version_check;
use anyhow::Context as _;
use futures_util::future::try_join_all;
use std::path::PathBuf;

use super::identity::HostIdentity;
use super::naming::{
    LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_ROLE_GIT_SHA, image_name,
};
use super::progress::{LaunchProgress, LaunchStage};

pub(super) struct PreparedRuntimeBinaries {
    agent_binaries: Vec<(crate::agent::Agent, PathBuf)>,
    jackin_capsule_src: String,
}

pub(super) async fn prepare_runtime_binaries(
    paths: &JackinPaths,
    validated_repo: &crate::repo::ValidatedRoleRepo,
    mut progress: Option<&mut LaunchProgress>,
) -> anyhow::Result<PreparedRuntimeBinaries> {
    if let Some(progress) = &mut progress {
        progress.stage_progress(LaunchStage::AgentBinaries, "preparing agent binaries");
    }

    let agents = validated_repo.manifest.supported_agents();

    // Resolve + download all agent binaries and jackin-capsule concurrently.
    // Each ensure_available call is network-bound (HTTP resolve + optional download),
    // so running them in parallel cuts wall-clock time to the slowest single binary
    // rather than the sum of all.
    //
    // Derived image ENTRYPOINT is `/jackin/runtime/jackin-capsule`, so a missing
    // capsule binary would produce an opaque "exec: file not found" at `docker run`.
    // Failing fast here gives an actionable error message.
    let agent_futures = agents.into_iter().map(|agent| async move {
        let binary = crate::agent_binary::ensure_available(paths, agent)
            .await
            .with_context(|| format!("preparing {} binary", agent.slug()))?;
        Ok::<_, anyhow::Error>((binary.agent, binary.path))
    });
    let capsule_future = async {
        capsule_binary::ensure_available(paths)
            .await
            .context("preparing jackin-capsule binary")
    };

    let (agent_binaries, jackin_capsule_binary) =
        tokio::try_join!(try_join_all(agent_futures), capsule_future)?;

    let jackin_capsule_src = jackin_capsule_binary.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "cached jackin-capsule path {} contains non-UTF-8 bytes; cannot reference it from Dockerfile",
            jackin_capsule_binary.display()
        )
    })?;

    Ok(PreparedRuntimeBinaries {
        agent_binaries,
        jackin_capsule_src: jackin_capsule_src.to_string(),
    })
}

/// Build the Docker image for the role. Returns the image name.
#[expect(
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
pub(super) async fn build_agent_image(
    paths: &JackinPaths,
    selector: &RoleSelector,
    cached_repo: &CachedRepo,
    validated_repo: &crate::repo::ValidatedRoleRepo,
    host: &HostIdentity,
    agent: crate::agent::Agent,
    runtime_binaries: PreparedRuntimeBinaries,
    rebuild: bool,
    agent_update: bool,
    debug: bool,
    branch_override: Option<&str>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    repo_lock: std::fs::File,
    progress: Option<&mut LaunchProgress>,
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
    let mut base_image_override = if use_prebuilt { published_image } else { None };

    // Resolve the role repo HEAD SHA once — used for the published-image
    // staleness check, the local-image freshness check, and as a build-arg
    // so local builds carry the same label.
    let head_sha = git_head_sha(&cached_repo.repo_dir, runner).await;

    // Compute the local workspace tag early so the local-freshness check
    // below can read its labels before we commit to a rebuild.
    let local_image_name = branch_override.map_or_else(
        || image_name(selector),
        |b| super::naming::image_name_for_branch(selector, b),
    );

    // When using the pre-built published image, check whether it is current:
    // - Primary check: `jackin.role_git_sha` label matches the HEAD of the
    //   cached role repo → image was built from the exact same commit, fresh.
    // - Fallback (images predating this feature): `jackin.construct_version`
    //   label matches the Dockerfile's pinned version → still usable.
    //
    // When the published image is stale, do NOT rebuild blindly — the local
    // workspace image from a previous `docker build` may already carry the
    // correct `jackin.role_git_sha` label. Without this short-circuit, every
    // launch declares "published image is out of date" and busts the Claude
    // install layer via a fresh `JACKIN_CACHE_BUST` timestamp, even when
    // nothing in the role repo or agent version has actually changed.
    let rebuild = if let Some(published) = published_image.filter(|_| use_prebuilt) {
        if published_image_is_stale(
            published,
            &validated_repo.dockerfile.construct_version,
            head_sha.as_deref(),
            docker,
        )
        .await
        {
            let local_is_fresh = match head_sha.as_deref() {
                Some(sha) => docker
                    .inspect_image_label(&local_image_name, LABEL_IMAGE_ROLE_GIT_SHA)
                    .await
                    .unwrap_or(None)
                    .is_some_and(|cached| cached == sha),
                None => false,
            };
            if local_is_fresh {
                crate::debug_log!(
                    "image",
                    "published image {published} is out of date; reusing local workspace image {local_image_name} (role SHA matches)"
                );
                use_prebuilt = false;
                base_image_override = None;
                rebuild
            } else {
                crate::debug_log!(
                    "image",
                    "published image {published} is out of date; rebuilding from workspace Dockerfile"
                );
                use_prebuilt = false;
                base_image_override = None;
                true
            }
        } else {
            rebuild
        }
    } else {
        rebuild
    };

    // create_derived_build_context copies the repo into a temp directory,
    // creating an immutable snapshot.  After this point the shared cached
    // repo can be safely modified by a parallel load.
    let build = create_derived_build_context(
        &cached_repo.repo_dir,
        validated_repo,
        base_image_override,
        Some(&runtime_binaries.jackin_capsule_src),
        &runtime_binaries.agent_binaries,
    )?;
    drop(repo_lock);

    if debug {
        let dockerfile_body = std::fs::read_to_string(&build.dockerfile_path)
            .unwrap_or_else(|e| format!("<read failed: {e}>"));
        crate::tui::emit_debug_line(
            "image",
            &format!(
                "DerivedDockerfile ({}):\n{dockerfile_body}",
                build.dockerfile_path.display(),
            ),
        );
    }
    let image = local_image_name.clone();

    let build_arg_uid = format!("JACKIN_HOST_UID={}", host.uid);
    let build_arg_gid = format!("JACKIN_HOST_GID={}", host.gid);
    let build_arg_role_git_sha =
        format!("ROLE_GIT_SHA={}", head_sha.as_deref().unwrap_or("unknown"));
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
    // When rebuild is already forced, the mismatch check result cannot change the
    // outcome — skip the round-trip. Treat inspect errors as label-absent (no
    // mismatch) so transient daemon errors never abort an otherwise-proceeding build.
    let construct_mismatch = if rebuild {
        false
    } else {
        docker
            .inspect_image_label(&image, LABEL_IMAGE_CONSTRUCT)
            .await
            .unwrap_or(None)
            .is_some_and(|cached| cached != current_construct)
    };
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
    // Workspace mode with rebuild=true (explicit --rebuild or staleness-driven
    // fallback): pass --pull to refresh the upstream construct base before
    // rebuilding from the workspace Dockerfile.
    //
    // Workspace mode without rebuild (no published_image): omit --pull so
    // Docker's layer cache is respected across invocations. The base image is
    // not re-evaluated and heavy apt / toolchain layers stay cached.
    if use_prebuilt || rebuild {
        build_args.push("--pull");
    }

    let construct_label = format!("{LABEL_IMAGE_CONSTRUCT}={current_construct}");
    build_args.extend(["--build-arg", &build_arg_uid]);
    build_args.extend(["--build-arg", &build_arg_gid]);
    build_args.extend(["--build-arg", &cache_bust]);
    build_args.extend(["--build-arg", &build_arg_role_git_sha]);
    build_args.extend(["--label", &construct_label]);
    build_args.extend(["-t", &image, "-f", &dockerfile_path, &context_dir]);

    let github_token = resolve_github_token(runner).await;
    let secret_file: Option<tempfile::NamedTempFile> =
        github_token
            .as_ref()
            .and_then(|token| match tempfile::NamedTempFile::new() {
                Err(e) => {
                    emit_compact_image_warning(&format!(
                        "failed to create tempfile for GitHub token: {e}; build will use unauthenticated GitHub API"
                    ),
                    );
                    None
                }
                Ok(mut f) => match std::io::Write::write_all(&mut f, token.as_bytes()) {
                    Err(e) => {
                        emit_compact_image_warning(&format!(
                            "failed to write GitHub token to tempfile: {e}; build will use unauthenticated GitHub API"
                        ),
                        );
                        None
                    }
                    Ok(()) => Some(f),
                },
            });
    let secret_arg = secret_file
        .as_ref()
        .map(|f| format!("id=github_token,src={}", f.path().display()));
    if let Some(ref s) = secret_arg {
        build_args.extend(["--secret", s.as_str()]);
    }

    if let Some(progress) = progress {
        progress.stage_progress(LaunchStage::DerivedImage, "Building Docker image");
    }

    // Tee the build's captured output into the live build-log sink so the
    // loading cockpit can show it on demand (the build is the slowest step).
    // `end` stops teeing but keeps the captured lines for the dialog.
    crate::runtime::build_log::begin();
    let build_result = runner
        .run(
            "docker",
            &build_args,
            None,
            &RunOptions {
                capture_stderr: true,
                capture_stdout: true,
                null_stdin: true,
                stream_captured_output: should_stream_build_output(debug),
                tee_to_build_log: true,
                extra_env: docker_build_env(github_token.is_some()),
                ..RunOptions::default()
            },
        )
        .await;
    crate::runtime::build_log::end();
    build_result?;

    extract_agent_version(paths, &image, agent, debug, runner).await;

    Ok(image)
}

/// Returns the HEAD commit SHA of the git repo at `dir`, or `None` if the
/// directory is not a git repo or the command fails.
async fn git_head_sha(dir: &std::path::Path, runner: &mut impl CommandRunner) -> Option<String> {
    let dir_str = dir.display().to_string();
    runner
        .capture("git", &["-C", &dir_str, "rev-parse", "HEAD"], None)
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn should_stream_build_output(debug: bool) -> bool {
    !debug && !crate::tui::rich_terminal_owned()
}

fn docker_build_env(has_github_token: bool) -> Vec<(String, String)> {
    let mut env = vec![("BUILDKIT_PROGRESS".to_string(), "plain".to_string())];
    if has_github_token {
        env.push(("DOCKER_BUILDKIT".to_string(), "1".to_string()));
    }
    env
}

fn emit_compact_image_warning(message: &str) {
    crate::tui::emit_compact_line("warning", &compact_image_warning_line(message));
}

fn compact_image_warning_line(message: &str) -> String {
    format!("jackin: warning: {message}")
}

/// Returns `true` when the published image is out of date relative to the
/// current role repo state.
///
/// Checks in order:
/// 1. `jackin.role_git_sha` label: if present and matches `head_sha`, the
///    image was built from the exact same commit — fresh, no rebuild needed.
///    If present and different, the image is stale.
/// 2. Fallback for images predating role-git-sha tracking:
///    `jackin.construct_version` label must match `dockerfile_version`.
///    Absent label is treated as fresh (backward compatibility).
///
/// If `docker pull` fails the image may not exist locally at all. Treating a
/// missing image as "not stale" would let the prebuilt path proceed and produce
/// a confusing late failure inside `docker build`. Return `true` (stale) so
/// jackin falls back to workspace mode, which gives the operator a clearer
/// error if the construct base is also unreachable.
async fn published_image_is_stale(
    published: &str,
    dockerfile_version: &str,
    head_sha: Option<&str>,
    docker: &impl DockerApi,
) -> bool {
    if let Err(e) = docker.pull_image(published).await {
        emit_compact_image_warning(&format!(
            "docker pull {published} failed ({e}); treating published image as stale and rebuilding from workspace Dockerfile"
        ));
        return true;
    }

    let labels = match docker.inspect_image_labels(published).await {
        Err(e) => {
            emit_compact_image_warning(&format!(
                "could not read labels from {published} ({e}); treating published image as stale"
            ));
            return true;
        }
        Ok(map) => map,
    };

    if let Some(sha) = head_sha {
        match labels.get(LABEL_IMAGE_ROLE_GIT_SHA).map(String::as_str) {
            Some(label_sha) if label_sha == sha => return false,
            Some(_) => return true,
            None => {}
        }
    }

    // Fallback: construct-version check for pre-role-git-sha images.
    labels
        .get(LABEL_IMAGE_CONSTRUCT_VERSION)
        .is_some_and(|stored| stored != dockerfile_version)
}

#[expect(clippy::type_complexity)]
async fn extract_agent_version(
    paths: &JackinPaths,
    image: &str,
    agent: crate::agent::Agent,
    debug: bool,
    runner: &mut impl CommandRunner,
) {
    let (display, parse, store): (
        &str,
        for<'a> fn(&'a str) -> Option<&'a str>,
        fn(&JackinPaths, &str, &str),
    ) = match agent {
        crate::agent::Agent::Claude => (
            "Claude",
            version_check::parse_claude_version,
            version_check::store_image_version,
        ),
        crate::agent::Agent::Opencode => (
            "OpenCode",
            version_check::parse_opencode_version,
            version_check::store_opencode_version,
        ),
        crate::agent::Agent::Codex => (
            "Codex",
            version_check::parse_codex_version,
            version_check::store_codex_version,
        ),
        crate::agent::Agent::Amp => (
            "Amp",
            version_check::parse_amp_version,
            version_check::store_amp_version,
        ),
        crate::agent::Agent::Kimi => (
            "Kimi",
            version_check::parse_kimi_version,
            version_check::store_kimi_version,
        ),
    };
    let slug = agent.slug();
    let Ok(raw) = runner
        .capture(
            "docker",
            &["run", "--rm", "--entrypoint", slug, image, "--version"],
            None,
        )
        .await
    else {
        if debug {
            crate::tui::emit_debug_line(
                "image",
                &format!("could not probe {display} version from {image}; version check skipped"),
            );
        }
        return;
    };
    let version = raw.trim();
    if version.is_empty() {
        return;
    }
    if debug {
        crate::tui::emit_debug_line("image", &format!("{display} {version}"));
    }
    if let Some(semver) = parse(version) {
        store(paths, image, semver);
    } else if debug {
        crate::tui::emit_debug_line(
            "image",
            &format!("unexpected {slug} --version output: {version:?}"),
        );
    }
}

/// Resolves a GitHub token for authenticating mise's GitHub API calls during
/// Docker image builds. Checks `GITHUB_TOKEN` and `GH_TOKEN` env vars first
/// (set in CI and by operators), then falls back to `gh auth token` for local
/// development where the user is already logged in via the gh CLI.
///
/// Returns `None` when no token is available; callers must degrade gracefully
/// (build still works, mise falls back to unauthenticated GitHub API access).
async fn resolve_github_token(runner: &mut impl CommandRunner) -> Option<String> {
    for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Some(t) = std::env::var(var).ok().filter(|t| !t.trim().is_empty()) {
            return Some(t.trim().to_string());
        }
    }
    match runner.capture_secret("gh", &["auth", "token"], None).await {
        Ok(s) => {
            let s = s.trim().to_string();
            (!s.is_empty()).then_some(s)
        }
        Err(e) => {
            crate::debug_log!("github_token", "gh auth token failed (no token): {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker_client::FakeDockerClient;
    use std::collections::HashMap;
    use std::sync::{Mutex, MutexGuard};

    static RICH_SURFACE_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct RichSurfaceTestGuard {
        _guard: MutexGuard<'static, ()>,
    }

    impl Drop for RichSurfaceTestGuard {
        fn drop(&mut self) {
            crate::tui::set_rich_surface_active(false);
            crate::tui::set_host_screen_owned(false);
        }
    }

    fn rich_surface_test_guard() -> RichSurfaceTestGuard {
        let guard = RICH_SURFACE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        crate::tui::set_rich_surface_active(false);
        crate::tui::set_host_screen_owned(false);
        RichSurfaceTestGuard { _guard: guard }
    }

    fn make_docker(labels: HashMap<String, String>) -> FakeDockerClient {
        let docker = FakeDockerClient::default();
        docker
            .inspect_image_labels_queue
            .borrow_mut()
            .push_back(labels);
        docker
    }

    #[test]
    fn build_output_streams_for_compact_non_debug_runs() {
        let _guard = rich_surface_test_guard();
        assert!(should_stream_build_output(false));
    }

    #[test]
    fn build_output_is_suppressed_for_debug_or_rich_surface() {
        let _guard = rich_surface_test_guard();
        assert!(!should_stream_build_output(true));

        crate::tui::set_rich_surface_active(true);
        assert!(!should_stream_build_output(false));
        crate::tui::set_rich_surface_active(false);

        crate::tui::set_host_screen_owned(true);
        assert!(!should_stream_build_output(false));
    }

    #[test]
    fn docker_build_env_forces_plain_buildkit_progress() {
        assert_eq!(
            docker_build_env(false),
            vec![("BUILDKIT_PROGRESS".to_string(), "plain".to_string())]
        );
        assert_eq!(
            docker_build_env(true),
            vec![
                ("BUILDKIT_PROGRESS".to_string(), "plain".to_string()),
                ("DOCKER_BUILDKIT".to_string(), "1".to_string()),
            ]
        );
    }

    #[test]
    fn compact_image_warning_line_is_not_debug_prefixed() {
        let line = compact_image_warning_line("docker pull image failed");
        assert_eq!(line, "jackin: warning: docker pull image failed");
        assert!(!line.contains("[jackin debug"));
    }

    #[tokio::test]
    async fn published_image_fresh_when_sha_matches() {
        let docker =
            make_docker([(LABEL_IMAGE_ROLE_GIT_SHA.to_string(), "abc123".to_string())].into());
        let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
        assert!(!stale, "matching SHA should report image as fresh");
    }

    #[tokio::test]
    async fn published_image_stale_when_sha_differs() {
        let docker =
            make_docker([(LABEL_IMAGE_ROLE_GIT_SHA.to_string(), "oldsha".to_string())].into());
        let stale = published_image_is_stale("img:latest", "0.1", Some("newsha"), &docker).await;
        assert!(stale, "mismatched SHA should report image as stale");
    }

    #[tokio::test]
    async fn published_image_falls_back_to_construct_version_when_no_sha_label() {
        // No SHA label; construct_version matches → fresh.
        let docker =
            make_docker([(LABEL_IMAGE_CONSTRUCT_VERSION.to_string(), "0.1".to_string())].into());
        let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
        assert!(
            !stale,
            "matching construct version should be fresh when no SHA label"
        );
    }

    #[tokio::test]
    async fn published_image_stale_when_construct_version_differs() {
        let docker =
            make_docker([(LABEL_IMAGE_CONSTRUCT_VERSION.to_string(), "0.0".to_string())].into());
        let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
        assert!(
            stale,
            "outdated construct version should report image as stale"
        );
    }

    #[tokio::test]
    async fn published_image_fresh_when_no_labels_at_all() {
        // No labels at all → backward-compat: fresh.
        let docker = make_docker(HashMap::new());
        let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
        assert!(
            !stale,
            "absent construct_version label should be treated as fresh (compat)"
        );
    }

    #[tokio::test]
    async fn published_image_stale_when_pull_fails() {
        let docker = FakeDockerClient {
            fail_with: vec![("docker pull".to_string(), "network error".to_string())],
            ..FakeDockerClient::default()
        };
        let stale = published_image_is_stale("img:latest", "0.1", Some("abc123"), &docker).await;
        assert!(stale, "pull failure should report image as stale");
    }

    #[tokio::test]
    async fn published_image_stale_when_inspect_image_labels_fails() {
        let docker = FakeDockerClient {
            fail_with: vec![(
                "docker inspect image:".to_string(),
                "daemon error".to_string(),
            )],
            ..FakeDockerClient::default()
        };
        let stale = published_image_is_stale("img:latest", "0.1", Some("abc"), &docker).await;
        assert!(
            stale,
            "inspect_image_labels failure should treat image as stale"
        );
    }
}
