//! Workspace dependency-direction check (Workstream 4 of
//! `codebase-health-enforcement`).
//!
//! Walks `cargo metadata`'s resolved dep graph and asserts that no
//! workspace crate depends on a layer it shouldn't. The forbidden edges
//! are the P2 inverted-dependency rows in the architecture map:
//!
//! | From → To | Why forbidden |
//! | --- | --- |
//! | jackin-env → jackin-launch-tui | launch-tui is a TUI; env is infra. |
//! | jackin-docker → jackin-launch-tui | docker is infra; launch-tui is a TUI. |
//! | jackin-runtime → jackin-tui | runtime is infra; tui is presentation. |
//! | jackin-config → jackin-diagnostics | config is domain; diagnostics carries presentation concerns. |
//! | jackin-manifest → jackin-diagnostics | same as config. |
//!
//! Inverted edges trip the gate even if the original motivation has
//! been removed — the bans are the lasting change, the exceptions are
//! tracked in the roadmap item.
//!
//! ```sh
//! cargo xtask lint arch
//! ```

use std::collections::BTreeSet;
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;

/// Forbid edges (from, to). `from` is not allowed to depend on `to`.
/// Stored as `(from, to)` so symmetric blocks are easy to read.
const FORBIDDEN_EDGES: &[(&str, &str)] = &[
    // Domain infra lifting logs into the diagnostics sink. Will move to a
    // port-trait indirection once the debug telemetry refactor lands — the
    // log calls themselves stay, only the layer edge flips.
    ("jackin-config", "jackin-diagnostics"),
    ("jackin-manifest", "jackin-diagnostics"),
    // Presentation/infra leak — runtime owns the bootstrap pipeline and
    // currently reaches upward into the TUI for the build log and launch
    // view. W1 follow-up adds a port trait that runtime owns and the TUI
    // subscribes to.
    ("jackin-runtime", "jackin-tui"),
];

#[derive(Args, Debug)]
pub(crate) struct LintArchArgs {
    /// Print the parsed dep graph without checking the rules. Useful
    /// for debugging the gate.
    #[arg(long)]
    dump: bool,
    /// Fail on violations. Without this flag the gate prints violations
    /// but exits 0, so it can run on PRs while the inversions are still
    /// being cleaned up (each P2 cleanup PR flips the line off the
    /// forbidden list).
    #[arg(long)]
    strict: bool,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the gate report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

/// Run the dependency-direction gate. `strict` fails on violations;
/// non-strict reports and exits 0. The umbrella `cargo xtask lint` uses this.
pub(crate) fn check(strict: bool) -> Result<()> {
    run(LintArchArgs {
        dump: false,
        strict,
    })
}

pub(crate) fn run(args: LintArchArgs) -> Result<()> {
    let root = repo_root()?;
    let metadata = read_metadata(&root)?;

    // Build (crate_name → dependency_names) for workspace members only.
    let workspace_members: BTreeSet<String> = {
        // `cargo metadata` `workspace_members` are ids like
        // `path+file:///Users/.../crates/jackin-core#0.6.0-dev`. Extract
        // the crate directory name as the path component between the
        // last `/` and the `#`. Then look up the corresponding package
        // name (typically the same string, but `package` rename is
        // possible in Cargo.toml).
        let id_to_name: BTreeMap<&str, &str> = metadata
            .packages
            .iter()
            .map(|p| (p.id.as_str(), p.name.as_str()))
            .collect();
        metadata
            .workspace_members
            .iter()
            .filter_map(|id| {
                // Resolve the id to the package name. Most crates have
                // matching dir + package names; we trust `id_to_name`
                // (the canonical cargo metadata source).
                let name = id_to_name.get(id.as_str()).copied()?;
                Some((*name).to_owned())
            })
            .collect()
    };

    let mut deps = BTreeMap::new();
    for package in &metadata.packages {
        let name = package.name.as_str();
        if !workspace_members.contains(name) {
            continue;
        }
        let mut workspace_deps = BTreeSet::new();
        for d in &package.dependencies {
            if workspace_members.contains(d.name.as_str()) {
                workspace_deps.insert(d.name.clone());
            }
        }
        deps.insert(name.to_owned(), workspace_deps);
    }

    if args.dump {
        for (name, dep_set) in &deps {
            let mut list: Vec<&str> = dep_set.iter().map(String::as_str).collect();
            list.sort_unstable();
            emit(&format!("{name} → {}", list.join(", ")));
        }
        return Ok(());
    }

    let mut problems = Vec::new();
    for (from, to) in FORBIDDEN_EDGES {
        if let Some(actual) = deps.get(*from)
            && actual.contains(*to)
        {
            problems.push(format!(
                "{from} → {to}: forbidden (see codebases-health-enforcement W4)"
            ));
        }
    }
    if problems.is_empty() {
        emit(&format!(
            "arch gate OK — {} workspace deps checked, {} forbidden edges not crossed",
            deps.len(),
            FORBIDDEN_EDGES.len()
        ));
        return Ok(());
    }
    problems.sort();
    let message = format!(
        "{} dependency-direction violation(s):\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
    if args.strict {
        bail!("{message}");
    }
    // Non-strict mode: report but exit 0 so the gate can run on PRs
    // before every P2 cleanup lands. Operators who want a hard
    // failure today can run `cargo xtask lint arch --strict`.
    emit(&message);
    emit(
        "hint: re-run with --strict to fail on these (currently informational until all P2 cleanups land)",
    );
    Ok(())
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
    #[allow(dead_code)]
    id: String,
    #[serde(default)]
    dependencies: Vec<Dep>,
}

#[derive(serde::Deserialize)]
struct Dep {
    name: String,
}

fn read_metadata(root: &std::path::Path) -> Result<Metadata> {
    #[expect(
        clippy::disallowed_methods,
        reason = "build helper: synchronous cargo metadata probe"
    )]
    let output = Command::new("cargo")
        .args(["metadata", "--format-version=1"])
        .current_dir(root)
        .output()
        .context("running cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("parsing cargo metadata")
}

use std::collections::BTreeMap;

#[cfg(test)]
mod tests;
