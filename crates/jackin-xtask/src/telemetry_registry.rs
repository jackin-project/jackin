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
    validate_with_weaver(&root)
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
