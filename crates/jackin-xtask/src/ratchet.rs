//! Unified shrink-only ratchet engine (codebase-health-enforcement Phase 7).
//!
//! One declarative `ratchet.toml` plus one semantics implementation for every
//! budget family. Numeric families use high-water-mark shrink-only checks;
//! presence families use stale/new allowlist checks. CLI: `cargo xtask lint ratchet`.
//!
//! Legacy gates (`lint files` / `lint tests` / `lint suppressions`) are thin
//! shims that call [`check_families_at_root`] / [`print_families`] for their
//! family IDs — no independent budget TOML readers on the production enforce path.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;

const CONFIG_PATH: &str = "ratchet.toml";
const RERUN: &str = "cargo xtask lint ratchet";

/// Family IDs for the file-size production + test caps.
pub(crate) const FILE_SIZE_FAMILIES: &[&str] = &["file-size-production", "file-size-test"];
/// Family ID for the test-layout presence allowlist.
pub(crate) const TEST_LAYOUT_FAMILIES: &[&str] = &["test-layout"];
/// Family IDs for bare-allow + per-lint expect suppression budgets.
pub(crate) const SUPPRESSION_FAMILIES: &[&str] = &["bare-allow-per-crate", "expect-per-lint-crate"];

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

/// One problem row from a family check (structured for shims / JSON report).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FamilyProblem {
    pub family: String,
    pub key: String,
    pub message: String,
}

/// Result of checking one or more families.
#[derive(Debug, Clone)]
pub(crate) struct FamilyCheckOutcome {
    pub problems: Vec<FamilyProblem>,
    pub report_lines: Vec<String>,
}

pub(crate) fn enforce() -> Result<()> {
    run(LintRatchetArgs { print: None })
}

/// Print regenerated entries for each named family (legacy `--print-*` shims).
pub(crate) fn print_families(ids: &[&str]) -> Result<()> {
    let root = crate::docs::repo_root()?;
    let config = read_config(&root.join(CONFIG_PATH))?;
    for id in ids {
        print_family(&root, &config, id)?;
    }
    Ok(())
}

/// Cap for a numeric family (e.g. file-size production/test).
pub(crate) fn family_cap(id: &str) -> Result<usize> {
    let root = crate::docs::repo_root()?;
    let config = read_config(&root.join(CONFIG_PATH))?;
    let family = config
        .family
        .iter()
        .find(|f| f.id == id)
        .with_context(|| format!("unknown family {id:?} in {CONFIG_PATH}"))?;
    Ok(family.cap.unwrap_or(0))
}

/// Run family checks and return structured problems (for shims that need Report).
pub(crate) fn check_families_at_root(ids: &[&str]) -> Result<FamilyCheckOutcome> {
    let root = crate::docs::repo_root()?;
    let config = read_config(&root.join(CONFIG_PATH))?;
    check_families(&root, &config, Some(ids))
}

pub(crate) fn run(args: LintRatchetArgs) -> Result<()> {
    let root = crate::docs::repo_root()?;
    let config = read_config(&root.join(CONFIG_PATH))?;
    if let Some(family_id) = args.print.as_deref() {
        return print_family(&root, &config, family_id);
    }

    let outcome = check_families(&root, &config, None)?;
    emit_outcome(&outcome, config.family.len())
}

fn emit_outcome(outcome: &FamilyCheckOutcome, family_count: usize) -> Result<()> {
    if !outcome.report_lines.is_empty() {
        for line in &outcome.report_lines {
            emit(line);
        }
    }
    if outcome.problems.is_empty() {
        emit(&format!(
            "ratchet OK — {family_count} families (config {CONFIG_PATH})"
        ));
        return Ok(());
    }
    let mut problems: Vec<&str> = outcome
        .problems
        .iter()
        .map(|p| p.message.as_str())
        .collect();
    problems.sort_unstable();
    bail!(
        "{} ratchet violation(s):\n  {}\n\nFix the listed rows, then re-run `{RERUN}`.",
        problems.len(),
        problems.join("\n  ")
    )
}

fn check_families(
    root: &Path,
    config: &Config,
    only: Option<&[&str]>,
) -> Result<FamilyCheckOutcome> {
    let mut problems: Vec<FamilyProblem> = Vec::new();
    let mut report_lines: Vec<String> = Vec::new();

    for family in &config.family {
        if let Some(ids) = only
            && !ids.contains(&family.id.as_str())
        {
            continue;
        }
        match family.kind.as_str() {
            "numeric" => {
                let measured = invoke_provider(root, &family.provider)?;
                let cap = family.cap.unwrap_or(0);
                let budgeted: BTreeMap<&str, usize> = family
                    .entry
                    .iter()
                    .filter_map(|e| e.bound.map(|b| (e.key.as_str(), b)))
                    .collect();
                for (key, bound) in &budgeted {
                    // Cap-0 families (suppressions) treat a missing key as 0 so
                    // "now zero → delete the row" hits StaleUnderCap rather than
                    // the file-missing path meant for path keys.
                    let measured_opt = match measured.get(*key).copied() {
                        some @ Some(_) => some,
                        None if cap == 0 => Some(0),
                        None => None,
                    };
                    let v = check_numeric_entry(measured_opt, *bound, cap);
                    let msg = match v {
                        NumericVerdict::Ok => None,
                        NumericVerdict::StaleMissing => Some(format!(
                            "{id}/{key}: budgeted but file missing — delete the stale budget row; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::StaleUnderCap { measured } => Some(format!(
                            "{id}/{key}: measured {measured} ≤ cap {cap} — no longer needs grandfathering; delete the budget row; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::Shrink { measured, budgeted } => Some(format!(
                            "{id}/{key}: measured {measured} < budgeted {budgeted} — shrink the budget row to {measured}; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::Growth { measured, budgeted } => Some(format!(
                            "{id}/{key}: grew from {budgeted} to {measured} — refactor or intentionally raise only via budget update; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::UnlistedOverCap { .. } => None,
                    };
                    if let Some(message) = msg {
                        problems.push(FamilyProblem {
                            family: family.id.clone(),
                            key: (*key).to_owned(),
                            message,
                        });
                    }
                }
                for (key, m) in &measured {
                    if budgeted.contains_key(key.as_str()) {
                        continue;
                    }
                    if let NumericVerdict::UnlistedOverCap { measured, cap } =
                        check_numeric_unlisted(*m, cap)
                    {
                        problems.push(FamilyProblem {
                            family: family.id.clone(),
                            key: key.clone(),
                            message: format!(
                                "{id}/{key}: {measured} exceeds cap {cap} (unlisted) — refactor or add a budget row; regenerate: {RERUN} --print {id}",
                                id = family.id
                            ),
                        });
                    }
                }
                if family.mode == "report" {
                    report_lines.push(format!(
                        "{} (report-only) — {} key(s) measured",
                        family.id,
                        measured.len()
                    ));
                    problems.retain(|p| p.family != family.id);
                }
            }
            "presence" => {
                let allowed: BTreeSet<String> =
                    family.entry.iter().map(|e| e.key.clone()).collect();
                let measured = invoke_provider_presence(root, &family.provider)?;
                for (key, verdict) in check_presence(&measured, &allowed) {
                    let message = match verdict {
                        PresenceVerdict::Stale => format!(
                            "{id}/{key}: listed but no longer violates — remove the stale allowlist entry; regenerate: {RERUN} --print {id}",
                            id = family.id
                        ),
                        PresenceVerdict::New { reason } => format!(
                            "{id}/{key}: {reason} — fix or allowlist via `{RERUN} --print {id}`",
                            id = family.id
                        ),
                    };
                    problems.push(FamilyProblem {
                        family: family.id.clone(),
                        key,
                        message,
                    });
                }
            }
            other => bail!("unknown family kind {other:?} in {CONFIG_PATH}"),
        }
    }

    if let Some(ids) = only {
        let known: BTreeSet<&str> = config.family.iter().map(|f| f.id.as_str()).collect();
        for id in ids {
            if !known.contains(id) {
                bail!("unknown family {id:?} in {CONFIG_PATH}");
            }
        }
    }

    Ok(FamilyCheckOutcome {
        problems,
        report_lines,
    })
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
        "expect_per_lint_crate" => {
            let m = crate::suppressions::measure(root)?;
            Ok(m.expect_by_lint_crate
                .into_iter()
                .map(|((lint, crate_name), n)| (expect_key(&lint, &crate_name), n))
                .collect())
        }
        "agent_doc_bytes" => measure_agent_doc_bytes(root),
        "export_volume_constants" => measure_export_volume_constants(root),
        "export_volume_measured" => measure_export_volume_measured(root),
        "perf_dhat_budgets" => measure_perf_dhat_budgets(root),
        "test_layout_violations" => {
            // numeric view not used for presence; return empty
            Ok(BTreeMap::new())
        }
        "public_surface_pub_mods" => measure_public_surface_pub_mods(root),
        other => bail!("unknown ratchet provider {other:?}"),
    }
}

/// Per-crate count of `pub mod` lines (proxy for foundational surface growth).
/// Shrink-only ratchet — plan 019 growth report alternative to API snapshots.
fn measure_public_surface_pub_mods(root: &Path) -> Result<BTreeMap<String, usize>> {
    let crates_dir = root.join("crates");
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    if !crates_dir.is_dir() {
        return Ok(out);
    }
    for entry in fs::read_dir(&crates_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let lib = entry.path().join("src/lib.rs");
        if !lib.is_file() {
            continue;
        }
        let text =
            fs::read_to_string(&lib).with_context(|| format!("reading {}", lib.display()))?;
        let mut count = 0usize;
        for line in text.lines() {
            let t = line.trim_start();
            if t.starts_with("pub mod ") || t.starts_with("pub(crate) mod ") {
                count += 1;
            }
        }
        out.insert(name, count);
    }
    Ok(out)
}

/// Composite key for expect family rows: `{lint}@{crate}`.
pub(crate) fn expect_key(lint: &str, crate_name: &str) -> String {
    format!("{lint}@{crate_name}")
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

/// Read dhat allocation ceilings from `jackin-capsule` `perf_budgets.rs`.
fn measure_perf_dhat_budgets(root: &Path) -> Result<BTreeMap<String, usize>> {
    let path = root.join("crates/jackin-capsule/src/perf_budgets.rs");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading perf budgets at {}", path.display()))?;
    let mut out = BTreeMap::new();
    for (key, name) in [
        (
            "focused_full_snapshot_max_blocks",
            "FOCUSED_FULL_SNAPSHOT_MAX_BLOCKS",
        ),
        (
            "focused_full_snapshot_max_bytes",
            "FOCUSED_FULL_SNAPSHOT_MAX_BYTES",
        ),
        (
            "focused_borrowed_view_max_blocks",
            "FOCUSED_BORROWED_VIEW_MAX_BLOCKS",
        ),
        (
            "focused_borrowed_view_max_bytes",
            "FOCUSED_BORROWED_VIEW_MAX_BYTES",
        ),
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

/// Read the telemetry-conformance `MAX_DEBUG_LOGS` / `MAX_SPANS` constants.
fn measure_export_volume_constants(root: &Path) -> Result<BTreeMap<String, usize>> {
    let path = root.join("crates/jackin-diagnostics/src/tests.rs");
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

/// Prefer measured counts from `target/telemetry-volume.json` (written by the
/// conformance suite); fall back to source constants when the artifact is absent
/// (e.g. ratchet clean without a prior nextest run).
fn measure_export_volume_measured(root: &Path) -> Result<BTreeMap<String, usize>> {
    let artifact = root.join("target/telemetry-volume.json");
    if artifact.is_file() {
        let text = fs::read_to_string(&artifact)
            .with_context(|| format!("reading measured volume at {}", artifact.display()))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("parsing measured volume at {}", artifact.display()))?;
        let mut out = BTreeMap::new();
        for key in [
            "default_mode_logs",
            "default_mode_spans",
            "default_mode_metrics",
            "max_debug_logs",
            "max_spans",
        ] {
            if let Some(n) = value.get(key).and_then(serde_json::Value::as_u64) {
                out.insert(key.to_owned(), n as usize);
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    measure_export_volume_constants(root)
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
    // crate README files
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
                // report-only: all keys; cap=0 (suppression-style): every nonzero;
                // positive cap: only over-cap grandfather candidates.
                let print = match (family.mode.as_str(), family.cap) {
                    ("report", _) => true,
                    (_, Some(0)) => *bound > 0,
                    (_, Some(cap)) => *bound > cap,
                    (_, None) => *bound > 0,
                };
                if print {
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
mod tests;
