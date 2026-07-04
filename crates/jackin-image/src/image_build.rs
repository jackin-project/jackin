//! Image-build pipeline helpers — `docker build` env, build-context stats,
//! Dockerfile token/sha detection, buildkit step parsing, compact warning
//! line formatting.
//!
//! Architecture Invariant: L1 application crate module. Depends on `jackin-core`
//! (`CommandRunner`), `jackin-diagnostics`.

use std::collections::HashMap;

use anyhow::Context as _;
use jackin_core::CommandRunner;
use serde::Serialize;

pub fn should_stream_build_output(debug: bool) -> bool {
    !debug && !jackin_diagnostics::rich_terminal_owned()
}

pub fn local_image_output_arg(image: &str) -> String {
    format!("type=docker,name={image},compression=uncompressed")
}

pub async fn emit_non_containerd_image_store_note(runner: &mut impl CommandRunner) {
    let Ok(info) = runner
        .capture(
            "docker",
            &["info", "--format", "{{.Driver}}\n{{json .DriverStatus}}"],
            None,
        )
        .await
    else {
        return;
    };
    if docker_info_uses_containerd_store(&info) != Some(false) {
        return;
    }
    let detail = serde_json::json!({
        "containerd_image_store": false,
        "docker_info": info.trim(),
    })
    .to_string();
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "docker_image_store",
            "derived image",
            "Docker daemon is not using the containerd image store; local image export/unpack may be slower",
            Some(&detail),
        );
    }
}

pub fn docker_info_uses_containerd_store(info: &str) -> Option<bool> {
    let info = info.trim();
    if info.is_empty() {
        return None;
    }
    let lowered = info.to_ascii_lowercase();
    Some(lowered.contains("io.containerd.snapshotter") || lowered.contains("containerd"))
}

pub fn docker_build_env() -> Vec<(String, String)> {
    // BuildKit is required, not optional: the generated Dockerfiles use
    // `COPY --link --chmod=` and `--mount=type=secret`, all BuildKit-only.
    // Gating it on a github token left token-less builds on the legacy
    // builder, which rejects `--chmod` ("the --chmod option requires
    // BuildKit") and fails the derived-image build.
    vec![
        ("DOCKER_BUILDKIT".to_owned(), "1".to_owned()),
        ("BUILDKIT_PROGRESS".to_owned(), "plain".to_owned()),
        ("BUILDX_NO_DEFAULT_ATTESTATIONS".to_owned(), "1".to_owned()),
    ]
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BuildContextStats {
    pub files: u64,
    pub bytes: u64,
}

pub fn emit_build_context_snapshot(context_dir: &std::path::Path, source: &str) {
    match build_context_stats(context_dir) {
        Ok(stats) => {
            if let Some(run) = jackin_diagnostics::active_run() {
                let detail = serde_json::json!({
                    "source": source,
                    "files": stats.files,
                    "bytes": stats.bytes,
                    "context_dir": context_dir.display().to_string(),
                })
                .to_string();
                run.stage(
                    "build_context_snapshot",
                    "derived image",
                    &format!(
                        "derived {source} build context snapshot: {} files, {} bytes",
                        stats.files, stats.bytes
                    ),
                    Some(&detail),
                );
            }
        }
        Err(error) => emit_compact_image_warning(&format!(
            "failed to measure derived build context at {}: {error:#}",
            context_dir.display()
        )),
    }
}

#[derive(Debug, Serialize)]
struct ImageBuildSourceDiagnostic<'a> {
    source: &'a str,
    reason: &'a str,
    base_image: Option<&'a str>,
    pull_base_image: bool,
}

pub fn emit_image_build_source(base_image: Option<&str>, reason: &str, pull_base_image: bool) {
    let source = if base_image.is_some() {
        "published_image"
    } else {
        "workspace_dockerfile"
    };
    let detail = ImageBuildSourceDiagnostic {
        source,
        reason,
        base_image,
        pull_base_image,
    };
    let detail = serde_json::to_string(&detail).unwrap_or_else(|_| "{}".to_owned());
    if let Some(run) = jackin_diagnostics::active_run() {
        run.stage(
            "image_build_source",
            "derived image",
            "derived image build source selected",
            Some(&detail),
        );
    }
}

pub fn build_context_stats(context_dir: &std::path::Path) -> anyhow::Result<BuildContextStats> {
    let mut stats = BuildContextStats::default();
    collect_build_context_stats(context_dir, &mut stats)?;
    Ok(stats)
}

pub fn collect_build_context_stats(
    path: &std::path::Path,
    stats: &mut BuildContextStats,
) -> anyhow::Result<()> {
    let meta = std::fs::symlink_metadata(path)
        .with_context(|| format!("inspecting build-context path {}", path.display()))?;
    if meta.is_dir() {
        for entry in std::fs::read_dir(path)
            .with_context(|| format!("reading build-context directory {}", path.display()))?
        {
            let entry = entry.with_context(|| format!("reading entry under {}", path.display()))?;
            collect_build_context_stats(&entry.path(), stats)?;
        }
    } else {
        stats.files += 1;
        stats.bytes += meta.len();
    }
    Ok(())
}

pub fn dockerfile_requests_github_token_secret(dockerfile_path: &std::path::Path) -> bool {
    match std::fs::read_to_string(dockerfile_path) {
        Ok(body) => dockerfile_body_requests_github_token_secret(&body),
        Err(error) => {
            jackin_diagnostics::debug_log!(
                "image",
                "could not read DerivedDockerfile {} before token lookup ({error}); resolving GitHub token conservatively",
                dockerfile_path.display()
            );
            true
        }
    }
}

pub fn dockerfile_body_requests_github_token_secret(dockerfile_body: &str) -> bool {
    dockerfile_body
        .lines()
        .map(str::trim_start)
        .any(|line| !line.starts_with('#') && line.contains("id=github_token"))
}

pub fn dockerfile_requests_role_git_sha_arg(dockerfile_path: &std::path::Path) -> bool {
    match std::fs::read_to_string(dockerfile_path) {
        Ok(body) => dockerfile_body_requests_role_git_sha_arg(&body),
        Err(error) => {
            jackin_diagnostics::debug_log!(
                "image",
                "could not read DerivedDockerfile {} before ROLE_GIT_SHA arg detection ({error}); omitting unused build arg",
                dockerfile_path.display()
            );
            false
        }
    }
}

pub fn dockerfile_body_requests_role_git_sha_arg(dockerfile_body: &str) -> bool {
    dockerfile_body
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with('#'))
        .any(|line| {
            line.strip_prefix("ARG ")
                .or_else(|| line.strip_prefix("ARG\t"))
                .is_some_and(|rest| {
                    rest.trim_start()
                        .split(['=', ' ', '\t'])
                        .next()
                        .is_some_and(|name| name == "ROLE_GIT_SHA")
                })
        })
}

pub fn emit_docker_build_step_diagnostics() {
    let Some(run) = jackin_diagnostics::active_run() else {
        return;
    };
    let path = run.command_output_path("docker-build");
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return;
    };
    for step in parse_docker_build_steps(&contents) {
        run.docker_build_step(&step.step, &step.label, step.duration_ms, step.cached);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerBuildStep {
    pub step: String,
    pub label: String,
    pub duration_ms: Option<u64>,
    pub cached: bool,
}

pub fn parse_docker_build_steps(contents: &str) -> Vec<DockerBuildStep> {
    let mut labels = HashMap::new();
    let mut steps = Vec::new();
    for line in contents.lines() {
        let Some((step, rest)) = parse_buildkit_line(line) else {
            continue;
        };
        if is_buildkit_step_description(rest, labels.contains_key(&step)) {
            labels.insert(step.clone(), rest.to_owned());
            continue;
        }
        if let Some(completed) = parse_completed_buildkit_step(&step, rest, &labels) {
            steps.push(completed);
        }
    }
    steps
}

pub fn parse_buildkit_line(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let (prefix, rest) = trimmed.split_once(' ')?;
    let step = prefix.strip_prefix('#')?;
    if !step.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((step.to_owned(), rest.trim()))
}

pub fn is_buildkit_step_description(rest: &str, has_label: bool) -> bool {
    if rest.starts_with('[') {
        return true;
    }
    !has_label
        && !matches!(split_buildkit_duration(rest).0, "DONE" | "CACHED")
        && !rest.chars().next().is_some_and(|c| c.is_ascii_digit())
        && !rest.ends_with(" done")
}

pub fn parse_completed_buildkit_step(
    step: &str,
    rest: &str,
    labels: &HashMap<String, String>,
) -> Option<DockerBuildStep> {
    let (label, duration_ms) = split_buildkit_duration(rest);
    let completed = label == "DONE" || label == "CACHED";
    if !completed {
        return None;
    }
    let cached = label == "CACHED";
    let label = labels
        .get(step)
        .map_or_else(|| label.to_owned(), ToOwned::to_owned);
    Some(DockerBuildStep {
        step: step.to_owned(),
        label,
        duration_ms,
        cached,
    })
}

pub fn split_buildkit_duration(rest: &str) -> (&str, Option<u64>) {
    let Some((label, duration)) = rest.rsplit_once(' ') else {
        return (rest, None);
    };
    let Some(duration_ms) = parse_buildkit_duration_ms(duration) else {
        return (rest, None);
    };
    (label.trim_end(), Some(duration_ms))
}

pub fn parse_buildkit_duration_ms(value: &str) -> Option<u64> {
    let seconds = value.strip_suffix('s')?;
    let (whole, fraction) = seconds.split_once('.').map_or((seconds, ""), |parts| parts);
    if whole.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let whole_ms = whole.parse::<u64>().ok()?.checked_mul(1000)?;
    let fraction_ms = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<u64>().ok()?.checked_mul(100)?,
        2 => fraction.parse::<u64>().ok()?.checked_mul(10)?,
        _ => fraction[..3].parse::<u64>().ok()?,
    };
    whole_ms.checked_add(fraction_ms)
}

pub fn emit_compact_image_warning(message: &str) {
    jackin_diagnostics::emit_compact_line("warning", &compact_image_warning_line(message));
}

pub fn compact_image_warning_line(message: &str) -> String {
    format!("jackin: warning: {message}")
}

#[cfg(test)]
mod tests;
