//! Report-only code-health dashboard (codebase-health-enforcement Phase 0).
//!
//! ```sh
//! cargo xtask health                  # human report
//! cargo xtask health --format json    # machine-readable
//! cargo xtask health --write-baseline # refresh code-health-baseline.toml
//! ```
//!
//! Report-only by design — not wired into `cargo xtask lint` or CI as a
//! failing gate. Phase 7 decides which metrics become budgets.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use serde::Serialize;

use crate::docs::repo_root;

const BASELINE_PATH: &str = "code-health-baseline.toml";
const CRATES_GLOB: &str = "crates";
const LARGE_MODULE_LINES: usize = 300;
const TOP_PRODUCTION: usize = 15;
const TOP_TESTS: usize = 10;

#[derive(Args, Debug)]
pub(crate) struct HealthArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
    /// Write aggregate floors to `code-health-baseline.toml` at the repo root.
    #[arg(long)]
    write_baseline: bool,
    /// Minimum crate count for a helper name to count as a duplicate family.
    #[arg(long, default_value_t = 3)]
    min_crates: usize,
}

#[derive(Clone, Copy, Debug, ValueEnum, Default)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

let mut __cmd = Command::new("cargo")
        .args(["metadata", "--format-version=1", "--no-deps"])
        .current_dir(root)
        ;
    let output = crate::cmd::output_raw(&mut __cmd)
        .context("running cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let meta: Metadata =
        serde_json::from_slice(&output.stdout).context("parsing cargo metadata")?;
    let member_ids: BTreeSet<&str> = meta.workspace_members.iter().map(String::as_str).collect();
    let mut map = BTreeMap::new();
    for pkg in meta.packages {
        if !member_ids.contains(pkg.id.as_str()) {
            continue;
        }
        let cmd = if pkg.name == "jackin" {
            String::from("cargo nextest run -p jackin (E2E: --features e2e --profile docker-e2e)")
        } else {
            format!("cargo nextest run -p {}", pkg.name)
        };
        map.insert(pkg.name, cmd);
    }
    Ok(map)
}

fn print_human(report: &Report) {
    emit("# Code-health dashboard (Phase 0)");
    emit("");
    emit("## Largest production files (Phase 2/4 sizing)");
    for f in &report.largest_production_files {
        emit(&format!("  {:>5}  {}", f.lines, f.path));
    }
    emit("");
    emit("## Largest tests.rs files (Phase 3 sizing)");
    for f in &report.largest_test_files {
        emit(&format!("  {:>5}  {}", f.lines, f.path));
    }
    emit("");
    emit(&format!(
        "## Untested large modules >{LARGE_MODULE_LINES} lines (Phase 3 coverage-map report)"
    ));
    emit(&format!(
        "  count: {}",
        report.untested_large_modules.len()
    ));
    for f in report.untested_large_modules.iter().take(25) {
        emit(&format!("  {:>5}  {}", f.lines, f.path));
    }
    if report.untested_large_modules.len() > 25 {
        emit(&format!(
            "  … {} more",
            report.untested_large_modules.len() - 25
        ));
    }
    emit("");
    emit("## Suppressions (Phase 1 ratchet input)");
    let s = &report.suppressions;
    emit(&format!(
        "  allow_attrs={} expect_attrs={} bare_allow={} bare_expect={}",
        s.allow_attrs, s.expect_attrs, s.bare_allow_attrs, s.bare_expect_attrs
    ));
    emit("  top bare-allow crates:");
    let mut bare: Vec<_> = s.bare_by_crate.iter().collect();
    bare.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    for (crate_name, count) in bare.into_iter().take(10) {
        emit(&format!("    {crate_name}: {count}"));
    }
    emit("");
    emit("## Public surface proxy (Phase 2 pub-surface report)");
    let mut pubs: Vec<_> = report.pub_surface.iter().collect();
    pubs.sort_by(|a, b| b.1.pub_items.cmp(&a.1.pub_items).then_with(|| a.0.cmp(b.0)));
    for (name, surface) in pubs.into_iter().take(12) {
        emit(&format!(
            "  {name}: pub_items={} pub_mods={}",
            surface.pub_items, surface.pub_mods
        ));
    }
    emit("");
    emit("## Agent-doc bytes (Phase 6/7 context-economy budgets)");
    let mut total = 0usize;
    for d in &report.agent_docs {
        total += d.bytes;
        emit(&format!(
            "  {:>7} B (~{} tok)  {}",
            d.bytes, d.token_approx, d.path
        ));
    }
    emit(&format!("  total_bytes={total}"));
    emit("");
    emit("## Duplicate helper families (Phase 0 dashboard)");
    emit(&format!(
        "  families_reported={}",
        report.duplicate_helpers.len()
    ));
    for h in report.duplicate_helpers.iter().take(15) {
        emit(&format!(
            "  {} ({} crates): {}",
            h.name,
            h.crates.len(),
            h.crates.join(", ")
        ));
    }
    emit("");
    emit("## Advisory (Phase 1 scheduled lanes feed)");
    emit(&format!(
        "  bare_allow_ratio={:.3} ({}/{})",
        report.advisory.bare_allow_ratio,
        report.advisory.bare_allow_attrs,
        report.advisory.allow_attrs
    ));
    emit(&format!("  note: {}", report.advisory.note));
    emit("");
    emit("## Verification map (Phase 6 narrowest-command)");
    emit(&format!(
        "  workspace_members={}",
        report.verification_map.len()
    ));
}

fn toml_key(path: &str) -> String {
    path.replace(['/', '.'], "_")
}

fn write_baseline(root: &Path, report: &Report) -> Result<()> {
    let mut out = String::new();
    out.push_str(
        "# Generated by cargo xtask health --write-baseline. Phase 0 baseline; Phase 7's ratchet engine consumes these floors.\n",
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    out.push_str(&format!("# Generated at unix_time={now}\n"));
    out.push_str(
        "# suite-wall-time: seeded by plan 013 from CI junit artifacts, not locally computable\n\n",
    );

    out.push_str("[suppressions]\n");
    out.push_str(&format!("allow_attrs = {}\n", report.suppressions.allow_attrs));
    out.push_str(&format!(
        "expect_attrs = {}\n",
        report.suppressions.expect_attrs
    ));
    out.push_str(&format!(
        "bare_allow_attrs = {}\n",
        report.suppressions.bare_allow_attrs
    ));
    out.push_str(&format!(
        "bare_expect_attrs = {}\n\n",
        report.suppressions.bare_expect_attrs
    ));

    out.push_str("[suppressions.bare_by_crate]\n");
    for (k, v) in &report.suppressions.bare_by_crate {
        out.push_str(&format!("{k} = {v}\n"));
    }
    out.push('\n');

    out.push_str("[suppressions.by_lint]\n");
    for (k, v) in &report.suppressions.by_lint {
        let key = k.replace(':', "_");
        out.push_str(&format!("\"{key}\" = {v}\n"));
    }
    out.push('\n');

    out.push_str("[pub_surface]\n");
    for (k, v) in &report.pub_surface {
        out.push_str(&format!("{k} = {}\n", v.pub_items));
    }
    out.push('\n');

    out.push_str("[agent_docs]\n");
    for d in &report.agent_docs {
        let key = toml_key(&d.path);
        out.push_str(&format!("\"{key}\" = {}\n", d.bytes));
    }
    out.push('\n');

    out.push_str("[largest_production]\n");
    for f in &report.largest_production_files {
        let key = toml_key(&f.path);
        out.push_str(&format!("\"{key}\" = {}\n", f.lines));
    }

    fs::write(root.join(BASELINE_PATH), out)
        .with_context(|| format!("writing {BASELINE_PATH}"))?;
    Ok(())
}

#[cfg(test)]
mod tests;
