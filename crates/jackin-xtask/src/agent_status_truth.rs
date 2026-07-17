//! Bind the agent-runtime-status roadmap claim to compiled capsule wiring.

use std::fs;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;

const PAGE: &str = "docs/content/docs/roadmap/(agent-orchestrator-research)/(phase-2-operator-surface)/agent-runtime-status.mdx";
const CAPSULE_LIB: &str = "crates/jackin-capsule/src/lib.rs";

#[derive(Args, Debug)]
pub(crate) struct LintAgentStatusTruthArgs {}

pub(crate) fn run(_args: LintAgentStatusTruthArgs) -> Result<()> {
    let root = repo_root()?;
    let page = fs::read_to_string(root.join(PAGE)).with_context(|| format!("reading {PAGE}"))?;
    let capsule = fs::read_to_string(root.join(CAPSULE_LIB))
        .with_context(|| format!("reading {CAPSULE_LIB}"))?;
    let workspace = fs::read_to_string(root.join("Cargo.toml")).context("reading Cargo.toml")?;
    enforce(&page, &capsule, &workspace)
}

fn enforce(page: &str, capsule: &str, workspace: &str) -> Result<()> {
    let status = page
        .lines()
        .find(|line| line.starts_with("**Status**:"))
        .context("agent runtime status roadmap page has no **Status** line")?;
    let status = status.to_ascii_lowercase();
    let positive = ["implemented", "shipped", "landed", "complete and live"];
    let negative = [
        "not wired",
        "design complete",
        "partially",
        "not implemented",
        "in progress",
    ];
    if !positive.iter().any(|term| status.contains(term))
        || negative.iter().any(|term| status.contains(term))
    {
        return Ok(());
    }
    if !capsule
        .lines()
        .any(|line| line.trim() == "pub mod agent_status;")
    {
        bail!(
            "roadmap claims agent runtime status is implemented, but {CAPSULE_LIB} does not declare `pub mod agent_status;`"
        );
    }
    if workspace.contains("agent_status.rs") {
        bail!(
            "roadmap claims agent runtime status is implemented, but agent_status.rs remains ignored by cargo-shear"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
