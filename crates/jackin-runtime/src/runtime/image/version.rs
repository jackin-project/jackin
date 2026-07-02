//! Image version helpers extracted from image.rs.
use super::{
    Agent, AgentInstall, CommandRunner, HashMap, JackinPaths, LABEL_IMAGE_CONSTRUCT,
    LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_ROLE_GIT_SHA, PreparedRuntimeBinaries,
    version_check,
};

pub fn local_role_base_labels_match(
    labels: &HashMap<String, String>,
    construct_image: &str,
    dockerfile_construct_version: &str,
    head_sha: Option<&str>,
) -> bool {
    let role_sha_matches = match head_sha {
        Some(expected) => labels
            .get(LABEL_IMAGE_ROLE_GIT_SHA)
            .is_some_and(|stored| stored == expected),
        None => true,
    };
    if !role_sha_matches {
        return false;
    }

    if let Some(stored) = labels.get(LABEL_IMAGE_CONSTRUCT) {
        return stored == construct_image;
    }

    if let Some(stored) = labels.get(LABEL_IMAGE_CONSTRUCT_VERSION) {
        return stored == dockerfile_construct_version;
    }

    head_sha.is_some()
}

// Collapsed from a 5-arm match to AgentRuntime adapter dispatch.
// `runtime().label()` → display label; `runtime().parse_version(raw)` → semver parse;
// `version_check::store_version(paths, agent, image, version)` → unified store.
pub async fn extract_agent_version(
    paths: &JackinPaths,
    image: &str,
    agent: Agent,
    debug: bool,
    runner: &mut impl CommandRunner,
) {
    let runtime = agent.runtime();
    let slug = agent.slug();
    jackin_diagnostics::active_timing_started(
        "derived image",
        "selected_agent_version_probe",
        Some(slug),
    );
    let raw_result = runner
        .capture(
            "docker",
            &["run", "--rm", "--entrypoint", slug, image, "--version"],
            None,
        )
        .await;
    let Ok(raw) = raw_result else {
        jackin_diagnostics::active_timing_done(
            "derived image",
            "selected_agent_version_probe",
            Some("error"),
        );
        if debug {
            jackin_diagnostics::emit_debug_line(
                "image",
                &format!(
                    "could not probe {} version from {image}; version check skipped",
                    runtime.label()
                ),
            );
        }
        return;
    };
    jackin_diagnostics::active_timing_done(
        "derived image",
        "selected_agent_version_probe",
        Some("probed"),
    );
    let version = raw.trim();
    if version.is_empty() {
        return;
    }
    if debug {
        jackin_diagnostics::emit_debug_line("image", &format!("{} {version}", runtime.label()));
    }
    if let Some(semver) = runtime.parse_version(version) {
        version_check::store_version(paths, agent, image, semver);
    } else if debug {
        jackin_diagnostics::emit_debug_line(
            "image",
            &format!("unexpected {slug} --version output: {version:?}"),
        );
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "Image versioning: records docker-image labels + sibling to-disk per \
              agent + per-version branch telemetry. Body extraction follows the \
              deferred-parallel-pass plan; until that slice lands, the inline shape \
              preserves the per-version state machine."
)]
pub(super) async fn record_built_agent_version(
    paths: &JackinPaths,
    image: &str,
    agent: Agent,
    runtime_binaries: &PreparedRuntimeBinaries,
    debug: bool,
    runner: &mut impl CommandRunner,
) {
    if matches!(
        runtime_binaries.agent_installs.get(&agent),
        Some(AgentInstall::Prefetched(_))
    ) && let Some(version) = runtime_binaries.prefetched_agent_versions.get(&agent)
    {
        jackin_diagnostics::active_timing_started(
            "derived image",
            "selected_agent_version_probe",
            Some(agent.slug()),
        );
        jackin_diagnostics::active_timing_done(
            "derived image",
            "selected_agent_version_probe",
            Some("prefetched"),
        );
        version_check::store_version(paths, agent, image, version);
        if debug {
            jackin_diagnostics::emit_debug_line(
                "image",
                &format!(
                    "{} {version} recorded from prefetched binary metadata; Docker probe skipped",
                    agent.runtime().label()
                ),
            );
        }
        return;
    }
    extract_agent_version(paths, image, agent, debug, runner).await;
}

/// Resolves a GitHub token for authenticating mise's GitHub API calls during
/// Docker image builds. Checks `GITHUB_TOKEN` and `GH_TOKEN` env vars first
/// (set in CI and by operators), then falls back to `gh auth token` for local
/// development where the user is already logged in via the gh CLI.
///
/// Returns `None` when no token is available; callers must degrade gracefully
/// (build still works, mise falls back to unauthenticated GitHub API access).
pub async fn resolve_github_token(runner: &mut impl CommandRunner) -> Option<String> {
    for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Some(t) = std::env::var(var).ok().filter(|t| !t.trim().is_empty()) {
            return Some(t.trim().to_owned());
        }
    }
    match runner.capture_secret("gh", &["auth", "token"], None).await {
        Ok(s) => {
            let s = s.trim().to_owned();
            (!s.is_empty()).then_some(s)
        }
        Err(e) => {
            jackin_diagnostics::debug_log!("github_token", "gh auth token failed (no token): {e}");
            None
        }
    }
}
