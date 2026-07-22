//! Workspace dependency-direction check — tier-graph model.
//!
//! Every workspace member has a declared tier (`TIERS`); production edges
//! must point at a
//! *strictly lower* tier, so a new crate gets a rule automatically by
//! appearing in `TIERS`. Dev-dependencies may point anywhere except into
//! a production+dev cycle (tracked by `DEV_CYCLE_ALLOWLIST` with
//! shrink-only stale-row enforcement). An explicit DFS cycle check over
//! production edges fails first when the graph is not a DAG.
//!
//! Tiers are graph-derived longest-path depths (2026-07-09, refreshed
//! when `jackin-test-support` landed). Re-derive with
//! `cargo xtask lint arch --dump` before renumbering.
//!
//! ```sh
//! cargo xtask lint arch
//! cargo xtask lint arch --strict
//! cargo xtask lint arch --dump
//! ```

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;
use crate::report::{self, FormatArgs};

/// Architecture tiers. Lower = more foundational. A production dependency
/// must point at a strictly lower tier; dev-dependencies may point anywhere
/// except into a production+dev cycle. Derived from the measured dependency
/// graph; `lint arch --dump` prints the live graph.
///
/// `pub(crate)` so the headers gate (plan 016) can cross-check crate
/// ownership headers against this table.
pub(crate) const TIERS: &[(&str, u8)] = &[
    ("jackin-core", 0),
    ("jackin-telemetry", 0),
    ("jackin-brand", 0),
    ("jackin-dev", 0),
    ("jackin-process", 0),
    ("jackin-term", 0),
    ("jackin-tui", 1),
    ("jackin-build-meta", 1),
    ("jackin-pr-trailers", 1),
    ("jackin-xtask", 1),
    ("jackin-config", 1),
    ("jackin-protocol", 1),
    ("jackin-agent-status", 2),
    ("jackin-diagnostics", 2),
    ("jackin-manifest", 2),
    ("jackin-oppicker", 3),
    ("jackin-docker", 3),
    ("jackin-env", 3),
    ("jackin-instance", 3),
    ("jackin-otlp-testbed", 3),
    ("jackin-launch", 3),
    ("jackin-test-support", 3),
    ("jackin-usage", 3),
    ("jackin-usage-ffi", 4),
    ("jackin-capsule", 4),
    ("jackin-console", 4),
    ("jackin-host", 4),
    ("jackin-image", 4),
    ("jackin-isolation", 4),
    ("jackin-runtime", 5),
    ("jackin", 6),
];

/// Grandfathered production+dev cycles. Each entry is a dev-edge
/// `(from, to)` that closes a cycle with production edges and is allowed
/// until the underlying debt is fixed. Stale rows (cycle no longer
/// present) fail the gate and must be removed.
///
/// Empty after plan 025 moved fakes into `jackin-test-support` (the old
/// `jackin-isolation → jackin-runtime` DEBT-devdep-cycle is gone).
const DEV_CYCLE_ALLOWLIST: &[(&str, &str)] = &[];

#[derive(Args, Debug)]
pub(crate) struct LintArchArgs {
    #[command(flatten)]
    output: FormatArgs,
    /// Print the parsed dep graph (with tier annotations) without checking
    /// the rules. Useful for debugging the gate and re-deriving `TIERS`.
    #[arg(long)]
    dump: bool,
    /// Fail on violations. Without this flag the gate prints violations
    /// but exits 0 (legacy informational mode). Umbrella `lint --strict`
    /// and CI pass `--strict`.
    #[arg(long)]
    strict: bool,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the gate report is its output"
)]
fn emit(line: &str) {
    if report::human_output() {
        println!("{line}");
    }
}

/// Run the dependency-direction gate. `strict` fails on violations;
/// non-strict reports and exits 0. The umbrella `cargo xtask lint` uses this.
pub(crate) fn check(strict: bool) -> Result<()> {
    run(LintArchArgs {
        output: FormatArgs::default(),
        dump: false,
        strict,
    })
}

pub(crate) fn run(args: LintArchArgs) -> Result<()> {
    let format = args.output.resolved();
    report::run_gate(
        format,
        "arch",
        "Cargo.toml",
        "restore the declared crate tiers and dependency direction invariants",
        "cargo xtask lint arch --strict",
        || run_inner(args),
    )
}

fn run_inner(args: LintArchArgs) -> Result<()> {
    let root = repo_root()?;
    // Turso sole-owner is an architecture boundary (roadmap item 8).
    crate::container_paths_gate::check_turso_sole_owner(&root)?;
    // Plan 019: env-pilot curated `pub mod` surface (grows as crates narrow).
    crate::ratchet::check_curated_pub_mods(&root)?;
    check_tui_ownership(&root)?;
    let metadata = read_metadata(&root)?;

    let workspace_members: BTreeSet<String> = {
        let id_to_name: BTreeMap<&str, &str> = metadata
            .packages
            .iter()
            .map(|p| (p.id.as_str(), p.name.as_str()))
            .collect();
        metadata
            .workspace_members
            .iter()
            .filter_map(|id| id_to_name.get(id.as_str()).map(|n| (*n).to_owned()))
            .collect()
    };

    let mut prod_edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut dev_edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for package in &metadata.packages {
        let name = package.name.as_str();
        if !workspace_members.contains(name) {
            continue;
        }
        let mut prod = BTreeSet::new();
        let mut dev = BTreeSet::new();
        for d in &package.dependencies {
            if !workspace_members.contains(d.name.as_str()) {
                continue;
            }
            if d.kind.as_deref() == Some("dev") {
                dev.insert(d.name.clone());
            } else {
                // Keep normal + build deps under the production rule.
                prod.insert(d.name.clone());
            }
        }
        prod_edges.insert(name.to_owned(), prod);
        if !dev.is_empty() {
            dev_edges.insert(name.to_owned(), dev);
        }
    }

    let tier_map: BTreeMap<&str, u8> = TIERS.iter().copied().collect();

    if args.dump {
        for name in &workspace_members {
            let deps = prod_edges.get(name).cloned().unwrap_or_default();
            let mut list: Vec<&str> = deps.iter().map(String::as_str).collect();
            list.sort_unstable();
            let tier = tier_map
                .get(name.as_str())
                .map_or_else(|| "T?".into(), |t| format!("T{t}"));
            emit(&format!("{name} ({tier}) → {}", list.join(", ")));
        }
        return Ok(());
    }

    let problems = evaluate(
        &tier_map,
        &prod_edges,
        &dev_edges,
        &workspace_members,
        DEV_CYCLE_ALLOWLIST,
    );

    if problems.is_empty() {
        let prod_edge_count: usize = prod_edges.values().map(BTreeSet::len).sum();
        emit(&format!(
            "arch gate OK — {} crates tiered, {} production edges checked, {} grandfathered dev cycle(s)",
            workspace_members.len(),
            prod_edge_count,
            DEV_CYCLE_ALLOWLIST.len()
        ));
        return Ok(());
    }

    let message = format!(
        "{} architecture violation(s):\n  {}\n\nfix: see crates/jackin-xtask/src/arch.rs; re-run: cargo xtask lint arch --strict",
        problems.len(),
        problems.join("\n  ")
    );
    if args.strict {
        bail!("{message}");
    }
    emit(&message);
    emit("hint: re-run with --strict to fail on these");
    Ok(())
}

fn check_tui_ownership(root: &std::path::Path) -> Result<()> {
    let mut problems = Vec::new();
    for legacy in ["crates/jackin-launch-tui", "crates/jackin-console-oppicker"] {
        if root.join(legacy).exists() {
            problems.push(format!(
                "{legacy}: retired crate directory exists; use the canonical surface crate name"
            ));
        }
    }
    for duplicate_tui in [
        "crates/jackin/src/console/tui.rs",
        "crates/jackin/src/console/tui",
    ] {
        if root.join(duplicate_tui).exists() {
            problems.push(format!(
                "{duplicate_tui}: root console cannot own a second TUI tree; keep product presentation in jackin-console and host bindings under console/adapter"
            ));
        }
    }
    for duplicate_terminal_adapter in [
        "crates/jackin-runtime/src/runtime/host_colors.rs",
        "crates/jackin-capsule/src/tui/host_colors.rs",
    ] {
        if root.join(duplicate_terminal_adapter).exists() {
            problems.push(format!(
                "{duplicate_terminal_adapter}: OSC host-color handshake is shared protocol behavior; keep its single implementation in jackin-protocol"
            ));
        }
    }

    check_file_excludes(
        &root.join("crates/jackin-core/Cargo.toml"),
        &["ratatui", "crossterm", "termrock", "jackin-tui"],
        &mut problems,
    )?;
    check_rust_tree_excludes(
        &root.join("crates/jackin-core/src"),
        &["ratatui", "crossterm", "termrock", "jackin_tui"],
        &mut problems,
    )?;
    check_file_excludes(
        &root.join("crates/jackin-runtime/Cargo.toml"),
        &["ratatui", "termrock", "jackin-tui"],
        &mut problems,
    )?;
    check_rust_tree_excludes(
        &root.join("crates/jackin-runtime/src"),
        &["ratatui", "termrock", "jackin_tui"],
        &mut problems,
    )?;

    let shared_src = root.join("crates/jackin-tui/src");
    for forbidden in ["theme.rs", "terminal.rs", "run.rs"] {
        if shared_src.join(forbidden).exists() {
            problems.push(format!(
                "crates/jackin-tui/src/{forbidden}: shared product composition cannot own a generic theme, terminal lifecycle, or run loop"
            ));
        }
    }

    if problems.is_empty() {
        return Ok(());
    }
    problems.sort();
    bail!(
        "{} TUI ownership violation(s):\n  {}",
        problems.len(),
        problems.join("\n  ")
    )
}

fn check_file_excludes(
    path: &std::path::Path,
    forbidden: &[&str],
    problems: &mut Vec<String>,
) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("reading TUI ownership input {}", path.display()))?;
    for token in forbidden {
        if source.contains(token) {
            problems.push(format!(
                "{}: contains forbidden TUI dependency `{token}`",
                path.display()
            ));
        }
    }
    Ok(())
}

fn check_rust_tree_excludes(
    dir: &std::path::Path,
    forbidden: &[&str],
    problems: &mut Vec<String>,
) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(dir)
        .with_context(|| format!("reading TUI ownership tree {}", dir.display()))?
    {
        let path = entry.path();
        if path.is_dir() {
            check_rust_tree_excludes(&path, forbidden, problems)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            check_file_excludes(&path, forbidden, problems)?;
        }
    }
    Ok(())
}

/// Pure rule evaluation. Extracted so unit tests need no cargo invocation.
///
/// Returns a sorted list of problem strings. Each failure message names the
/// fix instruction so agents can act without reading the gate source.
pub(crate) fn evaluate(
    tiers: &BTreeMap<&str, u8>,
    prod_edges: &BTreeMap<String, BTreeSet<String>>,
    dev_edges: &BTreeMap<String, BTreeSet<String>>,
    members: &BTreeSet<String>,
    dev_cycle_allowlist: &[(&str, &str)],
) -> Vec<String> {
    let mut problems = Vec::new();

    // 1. Completeness — every workspace member must appear in TIERS.
    for name in members {
        if !tiers.contains_key(name.as_str()) {
            problems.push(format!(
                "{name}: no tier declared — add it to TIERS in crates/jackin-xtask/src/arch.rs (pick 1 + max tier of its internal deps)"
            ));
        }
    }

    // 4. Cycle check over production edges (fires before tier-order so a
    // cycle is reported with a path rather than a cascade of tier fails).
    if let Some(cycle_path) = find_prod_cycle(prod_edges) {
        problems.push(format!(
            "production dependency cycle: {} — production graph must be a DAG; break one of these edges",
            cycle_path.join(" → ")
        ));
    }

    // 2. Production rule: tier(to) < tier(from).
    for (from, tos) in prod_edges {
        let Some(&from_tier) = tiers.get(from.as_str()) else {
            continue; // already reported as missing-tier
        };
        for to in tos {
            let Some(&to_tier) = tiers.get(to.as_str()) else {
                continue;
            };
            if to_tier >= from_tier {
                problems.push(format!(
                    "{from} (T{from_tier}) → {to} (T{to_tier}): production dependencies must point at a strictly lower tier; either re-tier {from} above T{to_tier} in TIERS (and justify in the commit) or remove the dependency"
                ));
            }
        }
    }

    // 3. Dev rule: allow upward, but fail on prod+dev cycles not allowlisted.
    // Also fail on stale allowlist rows (cycle no longer present).
    let mut active_dev_cycles: BTreeSet<(&str, &str)> = BTreeSet::new();
    for (from, tos) in dev_edges {
        for to in tos {
            if prod_path_exists(prod_edges, to, from) {
                active_dev_cycles.insert((from.as_str(), to.as_str()));
            }
        }
    }

    let allowlist: BTreeSet<(&str, &str)> = dev_cycle_allowlist.iter().copied().collect();

    for &(from, to) in &active_dev_cycles {
        if !allowlist.contains(&(from, to)) {
            problems.push(format!(
                "{from} --dev--> {to}: closes a production+dev cycle and is not in DEV_CYCLE_ALLOWLIST; either break the cycle or add it to the allowlist in crates/jackin-xtask/src/arch.rs with a tracking comment"
            ));
        }
    }
    for &(from, to) in &allowlist {
        if !active_dev_cycles.contains(&(from, to)) {
            problems.push(format!(
                "({from}, {to}): listed in DEV_CYCLE_ALLOWLIST but no longer a production+dev cycle — remove the stale allowlist entry"
            ));
        }
    }

    problems.sort();
    problems
}

/// DFS cycle detection over production edges. Returns one cycle path if found.
fn find_prod_cycle(prod_edges: &BTreeMap<String, BTreeSet<String>>) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut color: BTreeMap<&str, Color> = BTreeMap::new();
    for name in prod_edges.keys() {
        color.insert(name.as_str(), Color::White);
    }
    for tos in prod_edges.values() {
        for to in tos {
            color.entry(to.as_str()).or_insert(Color::White);
        }
    }

    let mut path: Vec<String> = Vec::new();

    fn on_back_edge(path: &[String], v: &str) -> Option<Vec<String>> {
        let i = path.iter().position(|n| n == v)?;
        let mut cyc = path[i..].to_vec();
        cyc.push(v.to_owned());
        Some(cyc)
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "cycle DFS over the production graph is naturally nested; extract would obscure the algorithm"
    )]
    fn dfs<'a>(
        u: &'a str,
        color: &mut BTreeMap<&'a str, Color>,
        prod_edges: &'a BTreeMap<String, BTreeSet<String>>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        color.insert(u, Color::Gray);
        path.push(u.to_owned());
        if let Some(tos) = prod_edges.get(u) {
            for v in tos {
                match color.get(v.as_str()).copied().unwrap_or(Color::White) {
                    Color::Gray => {
                        if let Some(cyc) = on_back_edge(path, v) {
                            return Some(cyc);
                        }
                    }
                    Color::White => {
                        if let Some(cyc) = dfs(v, color, prod_edges, path) {
                            return Some(cyc);
                        }
                    }
                    Color::Black => {}
                }
            }
        }
        path.pop();
        color.insert(u, Color::Black);
        None
    }

    let nodes: Vec<&str> = color.keys().copied().collect();
    for n in nodes {
        if color.get(n).copied() == Some(Color::White)
            && let Some(cyc) = dfs(n, &mut color, prod_edges, &mut path)
        {
            return Some(cyc);
        }
    }
    None
}

/// Whether a directed path exists from `start` to `end` over production edges.
fn prod_path_exists(
    prod_edges: &BTreeMap<String, BTreeSet<String>>,
    start: &str,
    end: &str,
) -> bool {
    if start == end {
        return true;
    }
    let mut seen = BTreeSet::new();
    let mut stack = vec![start.to_owned()];
    while let Some(u) = stack.pop() {
        if u == end {
            return true;
        }
        if !seen.insert(u.clone()) {
            continue;
        }
        if let Some(tos) = prod_edges.get(&u) {
            for t in tos {
                stack.push(t.clone());
            }
        }
    }
    false
}

/// Minimal `cargo metadata` v1 schema. Avoids pulling the `cargo_metadata`
/// crate (which has a wider API surface than we need). We pluck only the
/// fields we read; serde ignores the rest.
#[derive(serde::Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(serde::Deserialize)]
struct Package {
    name: String,
    id: String,
    #[serde(default)]
    dependencies: Vec<Dep>,
}

#[derive(serde::Deserialize)]
struct Dep {
    name: String,
    #[serde(default)]
    kind: Option<String>,
}

fn read_metadata(root: &std::path::Path) -> Result<Metadata> {
    let mut meta = crate::cmd::command("cargo");
    meta.args(["metadata", "--format-version=1"])
        .current_dir(root);
    let output = crate::cmd::output_raw(&mut meta).context("running cargo metadata")?;
    if !output.success {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("parsing cargo metadata")
}

#[cfg(test)]
mod tests;
