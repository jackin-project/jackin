//! Unified shrink-only ratchet engine (codebase-health-enforcement Phase 7).
//!
//! One declarative `ratchet.toml` plus one semantics implementation for every
//! budget family. Numeric families use high-water-mark shrink-only checks;
//! presence families use stale/new allowlist checks. CLI: `cargo xtask lint ratchet`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;

const CONFIG_PATH: &str = "ratchet.toml";
const RERUN: &str = "cargo xtask lint ratchet";

#[derive(Debug, Args)]
pub(crate) struct LintRatchetArgs {
    /// Print regenerated entries for one family (`file-size-production`, …).
    #[arg(long)]
    print: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Config {
    family: Vec<Family>,
}

#[derive(Debug, Clone, Deserialize)]
struct Family {
    id: String,
    #[serde(default = "default_kind")]
    kind: String,
    provider: String,
    #[serde(default)]
    cap: Option<usize>,
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default)]
    entry: Vec<Entry>,
}

fn default_kind() -> String {
    "numeric".into()
}
fn default_mode() -> String {
    "enforce".into()
}

#[derive(Debug, Clone, Deserialize)]
struct Entry {
    key: String,
    #[serde(default)]
    bound: Option<usize>,
}

/// One shrink-only problem for a numeric family entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NumericVerdict {
    Ok,
    StaleMissing,
    StaleUnderCap { measured: usize },
    Shrink { measured: usize, budgeted: usize },
    Growth { measured: usize, budgeted: usize },
    UnlistedOverCap { measured: usize, cap: usize },
}

/// Presence-family verdict for one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PresenceVerdict {
    Stale,
    New { reason: String },
}

/// Pure numeric shrink-only check for one budgeted key.
#[must_use]
pub(crate) fn check_numeric_entry(
    measured: Option<usize>,
    budgeted: usize,
    cap: usize,
) -> NumericVerdict {
    match measured {
        None => NumericVerdict::StaleMissing,
        Some(m) if m <= cap => NumericVerdict::StaleUnderCap { measured: m },
        Some(m) if m < budgeted => NumericVerdict::Shrink {
            measured: m,
            budgeted,
        },
        Some(m) if m > budgeted => NumericVerdict::Growth {
            measured: m,
            budgeted,
        },
        Some(_) => NumericVerdict::Ok,
    }
}

/// Unlisted key over the family cap.
#[must_use]
pub(crate) fn check_numeric_unlisted(measured: usize, cap: usize) -> NumericVerdict {
    if measured > cap {
        NumericVerdict::UnlistedOverCap { measured, cap }
    } else {
        NumericVerdict::Ok
    }
}

/// Presence family: allowlist keys must still violate; unlisted violations fail.
#[must_use]
pub(crate) fn check_presence(
    violations: &BTreeMap<String, String>,
    allowed: &BTreeSet<String>,
) -> Vec<(String, PresenceVerdict)> {
    let mut out = Vec::new();
    for key in allowed {
        if !violations.contains_key(key) {
            out.push((key.clone(), PresenceVerdict::Stale));
        }
    }
    for (key, reason) in violations {
        if !allowed.contains(key) {
            out.push((
                key.clone(),
                PresenceVerdict::New {
                    reason: reason.clone(),
                },
            ));
        }
    }
    out
}

pub(crate) fn enforce() -> Result<()> {
    run(LintRatchetArgs { print: None })
}

pub(crate) fn run(args: LintRatchetArgs) -> Result<()> {
    let root = crate::docs::repo_root()?;
    let config = read_config(&root.join(CONFIG_PATH))?;
    if let Some(family_id) = args.print.as_deref() {
        return print_family(&root, &config, family_id);
    }

    let mut problems: Vec<String> = Vec::new();
    let mut report_lines: Vec<String> = Vec::new();
    for family in &config.family {
        match family.kind.as_str() {
            "numeric" => {
                let measured = invoke_provider(&root, &family.provider)?;
                let cap = family.cap.unwrap_or(0);
                let budgeted: BTreeMap<&str, usize> = family
                    .entry
                    .iter()
                    .filter_map(|e| e.bound.map(|b| (e.key.as_str(), b)))
                    .collect();
                for (key, bound) in &budgeted {
                    let v = check_numeric_entry(measured.get(*key).copied(), *bound, cap);
                    match v {
                        NumericVerdict::Ok => {}
                        NumericVerdict::StaleMissing => problems.push(format!(
                            "{id}/{key}: budgeted but file missing — delete the stale budget row; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::StaleUnderCap { measured } => problems.push(format!(
                            "{id}/{key}: measured {measured} ≤ cap {cap} — no longer needs grandfathering; delete the budget row; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::Shrink { measured, budgeted } => problems.push(format!(
                            "{id}/{key}: measured {measured} < budgeted {budgeted} — shrink the budget row to {measured}; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::Growth { measured, budgeted } => problems.push(format!(
                            "{id}/{key}: grew from {budgeted} to {measured} — refactor or intentionally raise only via budget update; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::UnlistedOverCap { .. } => {}
                    }
                }
                for (key, m) in &measured {
                    if budgeted.contains_key(key.as_str()) {
                        continue;
                    }
                    if let NumericVerdict::UnlistedOverCap { measured, cap } =
                        check_numeric_unlisted(*m, cap)
                    {
                        problems.push(format!(
                            "{id}/{key}: {measured} exceeds cap {cap} (unlisted) — refactor or add a budget row; regenerate: {RERUN} --print {id}",
                            id = family.id
                        ));
                    }
                }
                if family.mode == "report" {
                    report_lines.push(format!(
                        "agent-doc-bytes (report-only) — {} key(s) measured",
                        measured.len()
                    ));
                    // strip report-family problems so they never fail
                    problems.retain(|p| !p.starts_with(&format!("{}/", family.id)));
                }
            }
            "presence" => {
                let allowed: BTreeSet<String> =
                    family.entry.iter().map(|e| e.key.clone()).collect();
                let measured = invoke_provider_presence(&root, &family.provider)?;
                for (key, verdict) in check_presence(&measured, &allowed) {
                    match verdict {
                        PresenceVerdict::Stale => problems.push(format!(
                            "{id}/{key}: listed but no longer violates — remove the stale allowlist entry; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        PresenceVerdict::New { reason } => problems.push(format!(
                            "{id}/{key}: {reason} — fix or allowlist via `{RERUN} --print {id}`",
                            id = family.id
                        )),
                    }
                }
            }
            other => bail!("unknown family kind {other:?} in {CONFIG_PATH}"),
        }
    }

    if !report_lines.is_empty() {
        for line in report_lines {
            emit(&line);
        }
    }
    if problems.is_empty() {
        emit(&format!(
            "ratchet OK — {} families (config {CONFIG_PATH})",
            config.family.len()
        ));
        return Ok(());
    }
    problems.sort();
    bail!(
        "{} ratchet violation(s):\n  {}\n\nFix the listed rows, then re-run `{RERUN}`.",
        problems.len(),
        problems.join("\n  ")
    )
}

fn read_config(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading {CONFIG_PATH} at {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {CONFIG_PATH}"))
}

fn invoke_provider(root: &Path, provider: &str) -> Result<BTreeMap<String, usize>> {
    match provider {
        "file_lines_production" => measure_file_lines(root, false),
        "file_lines_test" => measure_file_lines(root, true),
        "bare_allow_per_crate" => {
            let m = crate::suppressions::measure(root)?;
            Ok(m.bare_by_crate)
        }
        "agent_doc_bytes" => measure_agent_doc_bytes(root),
        "export_volume_constants" => measure_export_volume_constants(root),
        "test_layout_violations" => {
            // numeric view not used for presence; return empty
            Ok(BTreeMap::new())
        }
        other => bail!("unknown ratchet provider {other:?}"),
    }
}

fn invoke_provider_presence(root: &Path, provider: &str) -> Result<BTreeMap<String, String>> {
    match provider {
        "test_layout_violations" => crate::test_layout::measure_violations(root),
        other => bail!("unknown presence provider {other:?}"),
    }
}

fn measure_file_lines(root: &Path, tests_only: bool) -> Result<BTreeMap<String, usize>> {
    let counts = crate::lint::measure_lines(root)?;
    let mut out = BTreeMap::new();
    for (path, lines) in counts {
        let is_test = path.file_name().is_some_and(|n| n == "tests.rs");
        if is_test != tests_only {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.insert(rel, lines);
    }
    Ok(out)
}

/// Read plan 044 `MAX_DEBUG_LOGS` / `MAX_SPANS` constants from conformance.rs.
fn measure_export_volume_constants(root: &Path) -> Result<BTreeMap<String, usize>> {
    let path = root.join("crates/jackin-diagnostics/src/conformance.rs");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading export-volume constants at {}", path.display()))?;
    let mut out = BTreeMap::new();
    for (key, name) in [
        ("max_debug_logs", "MAX_DEBUG_LOGS"),
        ("max_spans", "MAX_SPANS"),
    ] {
        let needle = format!("const {name}: usize = ");
        let Some(rest) = text.split(&needle).nth(1) else {
            bail!("missing {name} in {}", path.display());
        };
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let value: usize = digits
            .parse()
            .with_context(|| format!("parsing {name} value from {}", path.display()))?;
        out.insert(key.to_owned(), value);
    }
    Ok(out)
}

fn measure_agent_doc_bytes(root: &Path) -> Result<BTreeMap<String, usize>> {
    let mut out = BTreeMap::new();
    let candidates = ["AGENTS.md", "crates/AGENTS.md", "Claude.md", "CLAUDE.md"];
    for rel in candidates {
        let path = root.join(rel);
        if path.is_file() {
            let n = fs::metadata(&path).map_or(0, |m| m.len() as usize);
            out.insert(rel.to_owned(), n);
        }
    }
    // crate READMEs
    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        for entry in fs::read_dir(&crates_dir)? {
            let entry = entry?;
            let readme = entry.path().join("README.md");
            if readme.is_file() {
                let rel = readme
                    .strip_prefix(root)
                    .unwrap_or(&readme)
                    .to_string_lossy()
                    .replace('\\', "/");
                let n = fs::metadata(&readme).map_or(0, |m| m.len() as usize);
                out.insert(rel, n);
            }
        }
    }
    Ok(out)
}

fn print_family(root: &Path, config: &Config, family_id: &str) -> Result<()> {
    let family = config
        .family
        .iter()
        .find(|f| f.id == family_id)
        .with_context(|| format!("unknown family {family_id:?}"))?;
    match family.kind.as_str() {
        "numeric" => {
            let measured = invoke_provider(root, &family.provider)?;
            emit(&format!("# ratchet family {family_id} regenerated entries"));
            for (key, bound) in &measured {
                if family.cap.is_some_and(|cap| *bound > cap) || family.mode == "report" {
                    emit(&format!("[[family.entry]]\nkey = {key:?}\nbound = {bound}"));
                }
            }
        }
        "presence" => {
            let measured = invoke_provider_presence(root, &family.provider)?;
            emit(&format!("# ratchet family {family_id} regenerated keys"));
            for key in measured.keys() {
                emit(&format!("[[family.entry]]\nkey = {key:?}"));
            }
        }
        other => bail!("unknown kind {other:?}"),
    }
    Ok(())
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; ratchet output is user-facing"
)]
fn emit(line: &str) {
    println!("{line}");
}

#[cfg(test)]
#[path = "ratchet/tests.rs"]
mod tests;
