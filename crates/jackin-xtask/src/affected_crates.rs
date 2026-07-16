//! Select CI test crates from changed paths and workspace dependencies.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::Args;
use serde::Deserialize;

use crate::cmd;

#[derive(Args)]
pub(crate) struct AffectedCratesArgs {
    /// Git revision at the base of the diff.
    #[arg(long, required_unless_present = "all")]
    base: Option<String>,
    /// Git revision at the head of the diff.
    #[arg(long, default_value = "HEAD")]
    head: String,
    /// Select every workspace crate (for manual/full CI runs).
    #[arg(long)]
    all: bool,
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: BTreeSet<String>,
    resolve: Option<Resolve>,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    manifest_path: PathBuf,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Dependency {
    pkg: String,
}

struct WorkspaceGraph {
    names: BTreeMap<String, String>,
    roots: BTreeMap<String, PathBuf>,
    dependents: BTreeMap<String, BTreeSet<String>>,
}

pub(crate) fn run(args: AffectedCratesArgs) -> Result<()> {
    let metadata = cargo_metadata()?;
    let graph = WorkspaceGraph::from_metadata(metadata)?;
    let selected = if args.all {
        graph.all_names()
    } else {
        let base = args.base.as_deref().context("--base is required")?;
        graph.affected(&changed_paths(base, &args.head)?)
    };

    println!("{}", serde_json::to_string(&selected)?);
    Ok(())
}

fn cargo_metadata() -> Result<Metadata> {
    let output =
        cmd::output(cmd::command("cargo").args(["metadata", "--format-version", "1", "--locked"]))?;
    serde_json::from_slice(&output).context("parsing cargo metadata")
}

fn changed_paths(base: &str, head: &str) -> Result<Vec<PathBuf>> {
    let range = format!("{base}...{head}");
    let output =
        cmd::output(cmd::command("git").args(["diff", "--name-only", "-z", &range, "--"]))?;
    output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            String::from_utf8(path.to_vec())
                .map(PathBuf::from)
                .map_err(|error| anyhow!("Git returned a non-UTF-8 path: {error}"))
        })
        .collect()
}

impl WorkspaceGraph {
    fn from_metadata(metadata: Metadata) -> Result<Self> {
        let mut workspace_packages = metadata
            .packages
            .into_iter()
            .filter(|package| metadata.workspace_members.contains(&package.id))
            .collect::<Vec<_>>();
        let root = workspace_packages
            .first()
            .and_then(|package| package.manifest_path.parent())
            .and_then(|crate_dir| crate_dir.parent())
            .and_then(Path::parent)
            .context("finding workspace root from package manifests")?
            .to_path_buf();
        let mut names = BTreeMap::new();
        let mut roots = BTreeMap::new();
        for package in workspace_packages.drain(..) {
            let crate_root = package
                .manifest_path
                .parent()
                .context("workspace package manifest has no parent")?
                .strip_prefix(&root)
                .context("workspace package is outside the workspace root")?
                .to_path_buf();
            roots.insert(package.id.clone(), crate_root);
            names.insert(package.id, package.name);
        }

        let mut dependents = BTreeMap::<String, BTreeSet<String>>::new();
        let resolve = metadata
            .resolve
            .context("cargo metadata omitted resolve graph")?;
        for node in resolve.nodes {
            if !names.contains_key(&node.id) {
                continue;
            }
            for dependency in node.deps {
                if names.contains_key(&dependency.pkg) {
                    dependents
                        .entry(dependency.pkg)
                        .or_default()
                        .insert(node.id.clone());
                }
            }
        }
        Ok(Self {
            names,
            roots,
            dependents,
        })
    }

    fn all_names(&self) -> Vec<String> {
        let mut names = self.names.values().cloned().collect::<Vec<_>>();
        names.sort();
        names
    }

    fn affected(&self, paths: &[PathBuf]) -> Vec<String> {
        if paths.iter().any(|path| is_workspace_wide(path)) {
            return self.all_names();
        }

        let mut selected = BTreeSet::new();
        for path in paths {
            let Some((id, _)) = self
                .roots
                .iter()
                .filter(|(_, root)| path.starts_with(root))
                .max_by_key(|(_, root)| root.components().count())
            else {
                return self.all_names();
            };
            selected.insert(id.clone());
        }

        let mut queue = VecDeque::from_iter(selected.iter().cloned());
        while let Some(package) = queue.pop_front() {
            for dependent in self.dependents.get(&package).into_iter().flatten() {
                if selected.insert(dependent.clone()) {
                    queue.push_back(dependent.clone());
                }
            }
        }
        let mut names = selected
            .into_iter()
            .filter_map(|id| self.names.get(&id).cloned())
            .collect::<Vec<_>>();
        names.sort();
        names
    }
}

fn is_workspace_wide(path: &Path) -> bool {
    let text = path.to_string_lossy();
    matches!(
        text.as_ref(),
        "Cargo.toml"
            | "Cargo.lock"
            | "rust-toolchain.toml"
            | "clippy.toml"
            | "flaky-tests.toml"
            | ".config/nextest.toml"
            | "mise.toml"
            | "mise.lock"
    ) || text.starts_with(".cargo/")
        || text.starts_with(".github/workflows/")
        || text.starts_with(".github/actions/")
}

#[cfg(test)]
mod tests;
