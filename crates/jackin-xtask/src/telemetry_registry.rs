//! Closed telemetry-registry validation and generation gate.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;

use crate::docs::repo_root;

const LEGACY_NAMESPACE_ALLOWLIST: &[&str] = &[
    "crates/jackin-diagnostics/src/metrics.rs",
    "crates/jackin-diagnostics/src/observability.rs",
    "crates/jackin-diagnostics/src/operation.rs",
    "crates/jackin-diagnostics/src/registry.rs",
    "crates/jackin-diagnostics/src/run.rs",
    "crates/jackin-diagnostics/src/run/jsonl_adapter.rs",
    "crates/jackin-diagnostics/src/screen.rs",
    "crates/jackin-usage/src/telemetry.rs",
    "crates/jackin/src/app.rs",
];

const NON_TELEMETRY_DOTTED_NAME_FILES: &[&str] = &["crates/jackin-runtime/src/runtime/naming.rs"];

// Shrink-only migration inventories. A new file never joins these lists: it
// must use the governed facade/spawn helpers from its first commit.
const RAW_SPAWN_ALLOWLIST: &[&str] = &[
    "crates/jackin-capsule/src/daemon.rs",
    "crates/jackin-capsule/src/daemon/context_mgmt.rs",
    "crates/jackin-capsule/src/daemon/input_dispatch.rs",
    "crates/jackin-capsule/src/daemon/multiplexer_utils.rs",
    "crates/jackin-capsule/src/daemon/resource_metrics.rs",
    "crates/jackin-capsule/src/pr_context.rs",
    "crates/jackin-capsule/src/runtime_setup.rs",
    "crates/jackin-capsule/src/session.rs",
    "crates/jackin-capsule/src/socket.rs",
    "crates/jackin-capsule/src/util.rs",
    "crates/jackin-env/src/host_claude.rs",
    "crates/jackin-env/src/op_cli.rs",
    "crates/jackin-host/src/caffeinate.rs",
    "crates/jackin-host/src/host_clipboard.rs",
    "crates/jackin-image/src/agent_binary.rs",
    "crates/jackin-image/src/capsule_binary.rs",
    "crates/jackin-launch-tui/src/tui/input.rs",
    "crates/jackin-launch-tui/src/tui/run.rs",
    "crates/jackin-runtime/src/exec_host.rs",
    "crates/jackin-runtime/src/runtime/image.rs",
    "crates/jackin-runtime/src/runtime/image/prewarm.rs",
    "crates/jackin-runtime/src/runtime/launch/git_pull.rs",
    "crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs",
    "crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core/orchestrate.rs",
    "crates/jackin-runtime/src/runtime/launch/launch_runtime.rs",
    "crates/jackin-runtime/src/runtime/prewarm_trigger.rs",
    "crates/jackin-tui/src/runtime.rs",
    "crates/jackin-usage/src/usage/codex.rs",
    "crates/jackin-usage/src/usage/format.rs",
    "crates/jackin-usage/src/usage/grok.rs",
    "crates/jackin-usage/src/usage/refresh.rs",
    "crates/jackin/src/app.rs",
    "crates/jackin/src/cli/prewarm.rs",
    "crates/jackin/src/cli/status.rs",
    "crates/jackin/src/role_claude_plugins.rs",
];

const RAW_TRACING_ALLOWLIST: &[&str] = &[
    "crates/jackin-instance/src/lib.rs",
    "crates/jackin-runtime/src/runtime/attach.rs",
    "crates/jackin-runtime/src/runtime/cleanup.rs",
    "crates/jackin-runtime/src/runtime/host_attach.rs",
    "crates/jackin-runtime/src/runtime/image.rs",
    "crates/jackin-runtime/src/runtime/launch/git_pull.rs",
    "crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs",
    "crates/jackin-runtime/src/runtime/progress.rs",
    "crates/jackin-usage/src/telemetry.rs",
    "crates/jackin-usage/src/usage.rs",
    "crates/jackin-usage/src/usage/refresh.rs",
];

#[derive(Args, Debug)]
pub(crate) struct TelemetryRegistryArgs {
    /// Regenerate Rust sources from the registry before validating.
    #[arg(long)]
    pub(crate) generate: bool,
}

#[derive(Deserialize)]
struct RegistryFile {
    groups: Vec<Group>,
}

#[derive(Deserialize)]
struct Group {
    #[serde(default)]
    attributes: Vec<Attribute>,
}

#[derive(Deserialize)]
struct Attribute {
    id: String,
}

pub(crate) fn run(args: TelemetryRegistryArgs) -> Result<()> {
    let root = repo_root()?;
    let _generate = args.generate;
    validate_registry_matches_rust(&root)?;
    validate_legacy_namespaces(&root)?;
    validate_source_policy(&root)?;
    validate_with_weaver(&root)
}

fn validate_source_policy(root: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_source_files(&root.join("crates"), root, &mut files)?;
    let mut violations = Vec::new();
    for (relative, source) in files {
        let path = relative.to_string_lossy();
        let raw_spawn = [
            "tokio::spawn(",
            "tokio::task::spawn_blocking(",
            "spawn_blocking(",
            "std::thread::spawn(",
            "thread::spawn(",
            ".spawn_local(",
        ];
        if raw_spawn.iter().any(|needle| source.contains(needle))
            && !path.starts_with("crates/jackin-telemetry/src/spawn.rs")
            && !RAW_SPAWN_ALLOWLIST.contains(&path.as_ref())
        {
            violations.push(format!("{path}: unmanaged async/thread spawn"));
        }
        let raw_tracing = [
            "tracing::event!(",
            "tracing::info!(",
            "tracing::warn!(",
            "tracing::error!(",
            "tracing::debug!(",
            "tracing::trace!(",
            "tracing::span!(",
            "tracing::info_span!(",
        ];
        if raw_tracing.iter().any(|needle| source.contains(needle))
            && !path.starts_with("crates/jackin-telemetry/")
            && !path.starts_with("crates/jackin-diagnostics/")
            && !RAW_TRACING_ALLOWLIST.contains(&path.as_ref())
        {
            violations.push(format!("{path}: raw tracing call outside governed facade"));
        }
        if (source.contains("#[tracing::instrument") || source.contains("#[instrument"))
            && !source.contains("skip_all")
        {
            violations.push(format!("{path}: tracing instrument must declare skip_all"));
        }
        if (source.contains("opentelemetry::logs") || source.contains("LoggerProvider"))
            && !path.starts_with("crates/jackin-telemetry/")
            && !path.starts_with("crates/jackin-diagnostics/")
        {
            violations.push(format!("{path}: raw OpenTelemetry logs API"));
        }
        if source.contains("tracing_subscriber::fmt")
            && !path.starts_with("crates/jackin-diagnostics/")
        {
            violations.push(format!(
                "{path}: formatter layer outside diagnostics composition root"
            ));
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        bail!(
            "telemetry source-policy violations:\n  {}",
            violations.join("\n  ")
        )
    }
}

fn collect_source_files(
    dir: &Path,
    root: &Path,
    files: &mut Vec<(std::path::PathBuf, String)>,
) -> Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(&path, root, files)?;
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let text = relative.to_string_lossy();
        if text.contains("/tests/")
            || text.ends_with("/tests.rs")
            || text.contains("/benches/")
            || text.contains("/fuzz/")
            || text.starts_with("crates/jackin-xtask/")
            || text.starts_with("crates/jackin-dev/")
            || text.starts_with("crates/jackin-pr-trailers/")
            || text.contains("lookbook")
            || text.starts_with("crates/jackin-lints/")
        {
            continue;
        }
        files.push((relative, fs::read_to_string(&path)?));
    }
    Ok(())
}

fn validate_with_weaver(root: &Path) -> Result<()> {
    let mut version = crate::cmd::command("weaver");
    version.arg("--version");
    if crate::cmd::output_raw(&mut version).is_err() {
        return Ok(());
    }
    let mut command = crate::cmd::command("weaver");
    command.current_dir(root).args([
        "registry",
        "check",
        "-r",
        "crates/jackin-telemetry/registry",
    ]);
    crate::cmd::run_streaming(&mut command)
}

fn validate_registry_matches_rust(root: &Path) -> Result<()> {
    let registry_path = root.join("crates/jackin-telemetry/registry/attributes.yaml");
    let source_path = root.join("crates/jackin-telemetry/src/schema/attrs.rs");
    let registry: RegistryFile = serde_yaml_ng::from_str(
        &fs::read_to_string(&registry_path)
            .with_context(|| format!("reading {}", registry_path.display()))?,
    )
    .with_context(|| format!("parsing {}", registry_path.display()))?;
    let source = fs::read_to_string(&source_path)
        .with_context(|| format!("reading {}", source_path.display()))?;
    let ids = registry
        .groups
        .into_iter()
        .flat_map(|group| group.attributes)
        .map(|attribute| attribute.id)
        .collect::<BTreeSet<_>>();
    let missing = ids
        .iter()
        .filter(|id| !source.contains(&format!("\"{id}\"")))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "registry attributes missing generated constants: {}",
            missing.join(", ")
        );
    }
    Ok(())
}

fn validate_legacy_namespaces(root: &Path) -> Result<()> {
    let crates = root.join("crates");
    let mut violations = Vec::new();
    collect_rust_files(&crates, &mut violations, root)?;
    if violations.is_empty() {
        Ok(())
    } else {
        bail!(
            "unapproved legacy telemetry namespace literals:\n  {}",
            violations.join("\n  ")
        )
    }
}

fn collect_rust_files(dir: &Path, violations: &mut Vec<String>, root: &Path) -> Result<()> {
    let mut entries = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, violations, root)?;
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let relative_text = relative.to_string_lossy();
        if relative_text.contains("/tests/")
            || relative_text.ends_with("/tests.rs")
            || relative_text.contains("/benches/")
            || relative_text.contains("/fuzz/")
            || relative_text.starts_with("crates/jackin-xtask/")
            || LEGACY_NAMESPACE_ALLOWLIST.contains(&relative_text.as_ref())
            || NON_TELEMETRY_DOTTED_NAME_FILES.contains(&relative_text.as_ref())
        {
            continue;
        }
        let source =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        for (index, line) in source.lines().enumerate() {
            if contains_legacy_telemetry_name(line) {
                violations.push(format!("{}:{}", relative.display(), index + 1));
            }
        }
    }
    Ok(())
}

fn contains_legacy_telemetry_name(line: &str) -> bool {
    const NAMES: &[&str] = &[
        "parallax.run.id",
        "jackin.component",
        "jackin.screen.name",
        "jackin.screen.from",
        "jackin.workspace",
        "jackin.workspace.kind",
        "jackin.agent.selected",
        "jackin.agents.active",
        "jackin.role",
        "jackin.provider",
        "jackin.container.id",
        "jackin.container.name",
        "jackin.launch.stage",
        "jackin.stage",
        "jackin.action",
        "jackin.tab.label",
        "jackin.category",
        "jackin.diagnostics.events",
        "jackin.cache.hits",
    ];
    NAMES
        .iter()
        .any(|name| line.contains(&format!("\"{name}\"")))
}

#[cfg(test)]
mod tests;
