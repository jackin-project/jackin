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
    /// Read a previously resolved Cargo metadata document instead of invoking Cargo.
    #[arg(long, value_name = "PATH")]
    metadata: Option<PathBuf>,
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
    #[serde(default)]
    features: BTreeSet<String>,
}

#[derive(Deserialize)]
struct Dependency {
    pkg: String,
}

struct WorkspaceGraph {
    names: BTreeMap<String, String>,
    package_names: BTreeMap<String, String>,
    roots: BTreeMap<String, PathBuf>,
    dependents: BTreeMap<String, BTreeSet<String>>,
    resolved_dependencies: BTreeMap<String, BTreeSet<String>>,
    resolved_features: BTreeMap<String, BTreeSet<String>>,
}

pub(crate) fn run(args: AffectedCratesArgs) -> Result<()> {
    let metadata = cargo_metadata(args.metadata.as_deref())?;
    let graph = WorkspaceGraph::from_metadata(metadata)?;
    let selected = if args.all {
        graph.all_names()
    } else {
        let base = args.base.as_deref().context("--base is required")?;
        let paths = changed_paths(base, &args.head)?;
        let lock_packages = paths
            .iter()
            .any(|path| path == Path::new("Cargo.lock"))
            .then(|| changed_lock_packages(base, &args.head))
            .transpose()?;
        let workspace_dependencies = paths
            .iter()
            .any(|path| path == Path::new("Cargo.toml"))
            .then(|| changed_workspace_dependencies(base, &args.head))
            .transpose()?
            .flatten();
        graph.affected_with_dependencies(
            &paths,
            lock_packages.as_ref(),
            workspace_dependencies.as_ref(),
        )
    };

    let output = if args.cache_keys {
        serde_json::to_string(&graph.cache_keys(&selected, &args.head)?)?
    } else {
        serde_json::to_string(&selected)?
    };
    writeln!(io::stdout().lock(), "{output}").context("writing affected crate JSON")?;
    Ok(())
}

fn cargo_metadata(snapshot: Option<&Path>) -> Result<Metadata> {
    if let Some(path) = snapshot {
        let contents = std::fs::read(path)
            .with_context(|| format!("reading Cargo metadata snapshot {}", path.display()))?;
        return serde_json::from_slice(&contents)
            .with_context(|| format!("parsing Cargo metadata snapshot {}", path.display()));
    }
    let output = cmd::output(cmd::command("cargo").args([
        "metadata",
        "--format-version",
        "1",
        "--locked",
        "--offline",
    ]))?;
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
        let package_names = metadata
            .packages
            .iter()
            .map(|package| (package.id.clone(), package.name.clone()))
            .collect::<BTreeMap<_, _>>();
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
        let mut resolved_dependencies = BTreeMap::<String, BTreeSet<String>>::new();
        let mut resolved_features = BTreeMap::<String, BTreeSet<String>>::new();
        let resolve = metadata
            .resolve
            .context("cargo metadata omitted resolve graph")?;
        for node in resolve.nodes {
            let id = node.id;
            resolved_features.insert(id.clone(), node.features);
            for dependency in node.deps {
                resolved_dependencies
                    .entry(id.clone())
                    .or_default()
                    .insert(dependency.pkg.clone());
                if names.contains_key(&id) && names.contains_key(&dependency.pkg) {
                    dependents
                        .entry(dependency.pkg)
                        .or_default()
                        .insert(id.clone());
                }
            }
        }
        Ok(Self {
            names,
            package_names,
            roots,
            dependents,
            resolved_dependencies,
            resolved_features,
        })
    }

    fn all_names(&self) -> Vec<String> {
        let mut names = self.names.values().cloned().collect::<Vec<_>>();
        names.sort();
        names
    }

    #[cfg(test)]
    fn affected(&self, paths: &[PathBuf]) -> Vec<String> {
        self.affected_with_dependencies(paths, None, None)
    }

    fn affected_with_dependencies(
        &self,
        paths: &[PathBuf],
        changed_lock_packages: Option<&BTreeSet<String>>,
        changed_workspace_dependencies: Option<&BTreeSet<String>>,
    ) -> Vec<String> {
        let relevant = paths
            .iter()
            .filter(|path| !is_documentation(path) && !is_ci_orchestration(path))
            .collect::<Vec<_>>();
        if relevant.iter().any(|path| is_workspace_wide(path)) {
            return self.all_names();
        }

        let mut selected = BTreeSet::new();
        for path in relevant {
            let changed_dependencies = match path.as_path() {
                path if path == Path::new("Cargo.lock") => changed_lock_packages,
                path if path == Path::new("Cargo.toml") => changed_workspace_dependencies,
                _ => None,
            };
            if matches!(path.as_path(), path if path == Path::new("Cargo.lock") || path == Path::new("Cargo.toml"))
            {
                let Some(changed) = changed_dependencies else {
                    return self.all_names();
                };
                let known_names = self.package_names.values().collect::<BTreeSet<_>>();
                if changed.iter().any(|name| !known_names.contains(name)) {
                    return self.all_names();
                }
                selected.extend(
                    self.names
                        .keys()
                        .filter(|id| self.depends_on_changed_package(id, changed))
                        .cloned(),
                );
                continue;
            }
            if is_docker_test_input(path) {
                let Some(id) = self
                    .names
                    .iter()
                    .find_map(|(id, name)| (name == "jackin").then(|| id.clone()))
                else {
                    return self.all_names();
                };
                selected.insert(id);
                continue;
            }
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
        let global_ids = [Path::new("rust-toolchain.toml"), Path::new(".cargo")]
            .into_iter()
            .map(|path| git_object_id(head, path))
            .collect::<Result<Vec<_>>>()?;
        let workspace_contract = workspace_manifest_contract(head)?;

        selected
            .iter()
            .map(|name| {
                let id = ids_by_name
                    .get(name.as_str())
                    .with_context(|| format!("selected crate {name} is absent from metadata"))?;
                let closure = self.resolved_forward_closure(id);
                let mut hash = Sha256::new();
                hash.update(workspace_contract.as_bytes());
                for object_id in &global_ids {
                    hash.update(object_id.as_bytes());
                }
                for dependency in closure {
                    if let Some(tree_id) = tree_ids.get(&dependency) {
                        hash.update(b"workspace\0");
                        hash.update(
                            self.names
                                .get(&dependency)
                                .context("workspace dependency has no package name")?
                                .as_bytes(),
                        );
                        hash.update(b"\0");
                        hash.update(tree_id.as_bytes());
                    } else {
                        hash.update(b"external\0");
                        hash.update(dependency.as_bytes());
                    }
                    for feature in self
                        .resolved_features
                        .get(&dependency)
                        .into_iter()
                        .flatten()
                    {
                        hash.update(b"\0feature\0");
                        hash.update(feature.as_bytes());
                    }
                }
                Ok((name.clone(), hex::encode(hash.finalize())))
            })
            .collect()
    }

    fn resolved_forward_closure(&self, root: &str) -> BTreeSet<String> {
        let mut selected = BTreeSet::from([root.to_owned()]);
        let mut queue = VecDeque::from([root.to_owned()]);
        while let Some(package) = queue.pop_front() {
            for dependency in self
                .resolved_dependencies
                .get(&package)
                .into_iter()
                .flatten()
            {
                if selected.insert(dependency.clone()) {
                    queue.push_back(dependency.clone());
                }
            }
        }
        selected
    }

    fn depends_on_changed_package(&self, root: &str, changed: &BTreeSet<String>) -> bool {
        self.resolved_forward_closure(root)
            .iter()
            .filter_map(|dependency| self.package_names.get(dependency))
            .any(|name| changed.contains(name))
    }
}

fn changed_lock_packages(base: &str, head: &str) -> Result<BTreeSet<String>> {
    let base_lock = git_file(base, Path::new("Cargo.lock"))?;
    let head_lock = git_file(head, Path::new("Cargo.lock"))?;
    let base_packages = lock_packages(&base_lock)?;
    let head_packages = lock_packages(&head_lock)?;
    Ok(base_packages
        .iter()
        .filter(|(identity, value)| head_packages.get(*identity) != Some(*value))
        .chain(
            head_packages
                .iter()
                .filter(|(identity, value)| base_packages.get(*identity) != Some(*value)),
        )
        .map(|((name, _, _), _)| name.clone())
        .collect())
}

fn changed_workspace_dependencies(base: &str, head: &str) -> Result<Option<BTreeSet<String>>> {
    let mut base_manifest = manifest_value(&git_file(base, Path::new("Cargo.toml"))?)?;
    let mut head_manifest = manifest_value(&git_file(head, Path::new("Cargo.toml"))?)?;
    let base_dependencies = take_workspace_dependencies(&mut base_manifest);
    let head_dependencies = take_workspace_dependencies(&mut head_manifest);
    if base_manifest != head_manifest {
        return Ok(None);
    }
    let changed = base_dependencies
        .iter()
        .filter(|(name, value)| head_dependencies.get(*name) != Some(*value))
        .chain(
            head_dependencies
                .iter()
                .filter(|(name, value)| base_dependencies.get(*name) != Some(*value)),
        )
        .map(|(name, value)| dependency_package_name(name, value))
        .collect();
    Ok(Some(changed))
}

fn workspace_manifest_contract(revision: &str) -> Result<String> {
    let mut manifest = manifest_value(&git_file(revision, Path::new("Cargo.toml"))?)?;
    drop(take_workspace_dependencies(&mut manifest));
    let normalized = toml::to_string(&manifest).context("serializing workspace Cargo contract")?;
    Ok(hex::encode(Sha256::digest(normalized.as_bytes())))
}

fn manifest_value(contents: &[u8]) -> Result<toml::Value> {
    let document = std::str::from_utf8(contents).context("Cargo.toml is not UTF-8")?;
    toml::from_str(document).context("parsing workspace Cargo.toml")
}

fn take_workspace_dependencies(value: &mut toml::Value) -> toml::Table {
    value
        .get_mut("workspace")
        .and_then(toml::Value::as_table_mut)
        .and_then(|workspace| workspace.remove("dependencies"))
        .and_then(|dependencies| dependencies.as_table().cloned())
        .unwrap_or_default()
}

fn dependency_package_name(name: &str, value: &toml::Value) -> String {
    value
        .as_table()
        .and_then(|dependency| dependency.get("package"))
        .and_then(toml::Value::as_str)
        .unwrap_or(name)
        .to_owned()
}

fn git_file(revision: &str, path: &Path) -> Result<Vec<u8>> {
    let object = format!("{revision}:{}", path.to_string_lossy());
    cmd::output(cmd::command("git").args(["show", &object]))
}

type LockIdentity = (String, String, String);

fn lock_packages(contents: &[u8]) -> Result<BTreeMap<LockIdentity, toml::Value>> {
    let document = std::str::from_utf8(contents).context("Cargo.lock is not UTF-8")?;
    let value = toml::from_str::<toml::Value>(document).context("parsing Cargo.lock")?;
    value
        .get("package")
        .and_then(toml::Value::as_array)
        .context("Cargo.lock has no package array")?
        .iter()
        .map(|package| {
            let name = package
                .get("name")
                .and_then(toml::Value::as_str)
                .context("locked package has no name")?;
            let version = package
                .get("version")
                .and_then(toml::Value::as_str)
                .context("locked package has no version")?;
            let source = package
                .get("source")
                .and_then(toml::Value::as_str)
                .unwrap_or_default();
            Ok((
                (name.into(), version.into(), source.into()),
                package.clone(),
            ))
        })
        .collect()
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
        "rust-toolchain.toml"
            | "clippy.toml"
            | "flaky-tests.toml"
            | ".config/nextest.toml"
            | "mise.toml"
            | "mise.lock"
    ) || text.starts_with(".cargo/")
}

fn is_ci_orchestration(path: &Path) -> bool {
    path.starts_with(".github/workflows") || path.starts_with(".github/actions")
}

fn is_docker_test_input(path: &Path) -> bool {
    path == Path::new("docker-bake.hcl") || path.starts_with("docker")
}

fn is_documentation(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("md" | "mdx")
    )
}

#[cfg(test)]
mod tests;
