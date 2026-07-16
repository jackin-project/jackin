//! Select CI test crates from changed paths and workspace dependencies.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::Args;
use serde::Deserialize;
use sha2::{Digest, Sha256};

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
    /// Emit a JSON object of crate names to dependency-closure cache keys.
    #[arg(long)]
    cache_keys: bool,
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
    dependencies: BTreeMap<String, BTreeSet<String>>,
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

    let output = if args.cache_keys {
        serde_json::to_string(&graph.cache_keys(&selected, &args.head)?)?
    } else {
        serde_json::to_string(&selected)?
    };
    writeln!(io::stdout().lock(), "{output}").context("writing affected crate JSON")?;
    Ok(())
}

fn cargo_metadata() -> Result<Metadata> {
    let output =
        cmd::output(cmd::command("cargo").args(["metadata", "--format-version", "1", "--locked"]))?;
    serde_json::from_slice(&output).context("parsing cargo metadata")
}

fn changed_paths(base: &str, head: &str) -> Result<Vec<PathBuf>> {
    // CI checks out the PR merge result and fetches only the base tip. A
    // two-tree diff is exact for that shape and avoids downloading history
    // solely to reconstruct a merge base.
    let range = format!("{base}..{head}");
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
        let mut dependencies = BTreeMap::<String, BTreeSet<String>>::new();
        let resolve = metadata
            .resolve
            .context("cargo metadata omitted resolve graph")?;
        for node in resolve.nodes {
            if !names.contains_key(&node.id) {
                continue;
            }
            for dependency in node.deps {
                if names.contains_key(&dependency.pkg) {
                    dependencies
                        .entry(node.id.clone())
                        .or_default()
                        .insert(dependency.pkg.clone());
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
            dependencies,
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

        let mut queue = selected.iter().cloned().collect::<VecDeque<_>>();
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

    fn cache_keys(&self, selected: &[String], head: &str) -> Result<BTreeMap<String, String>> {
        let ids_by_name = self
            .names
            .iter()
            .map(|(id, name)| (name.as_str(), id.as_str()))
            .collect::<BTreeMap<_, _>>();
        let mut tree_ids = BTreeMap::new();
        for (id, root) in &self.roots {
            tree_ids.insert(id, git_object_id(head, root)?);
        }
        let global_ids = [
            Path::new("Cargo.lock"),
            Path::new("Cargo.toml"),
            Path::new("rust-toolchain.toml"),
            Path::new(".cargo"),
        ]
        .into_iter()
        .map(|path| git_object_id(head, path))
        .collect::<Result<Vec<_>>>()?;

        selected
            .iter()
            .map(|name| {
                let id = ids_by_name
                    .get(name.as_str())
                    .with_context(|| format!("selected crate {name} is absent from metadata"))?;
                let closure = self.forward_closure(id);
                let mut hash = Sha256::new();
                for object_id in &global_ids {
                    hash.update(object_id.as_bytes());
                }
                for dependency in closure {
                    hash.update(
                        tree_ids
                            .get(&dependency)
                            .context("workspace dependency has no Git tree id")?
                            .as_bytes(),
                    );
                }
                Ok((name.clone(), hex::encode(hash.finalize())))
            })
            .collect()
    }

    fn forward_closure(&self, root: &str) -> BTreeSet<String> {
        let mut selected = BTreeSet::from([root.to_owned()]);
        let mut queue = VecDeque::from([root.to_owned()]);
        while let Some(package) = queue.pop_front() {
            for dependency in self.dependencies.get(&package).into_iter().flatten() {
                if selected.insert(dependency.clone()) {
                    queue.push_back(dependency.clone());
                }
            }
        }
        selected
    }
}

fn git_object_id(head: &str, path: &Path) -> Result<String> {
    let revision = format!("{head}:{}", path.to_string_lossy());
    Ok(
        cmd::output_string(cmd::command("git").args(["rev-parse", &revision]))?
            .trim()
            .to_owned(),
    )
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
