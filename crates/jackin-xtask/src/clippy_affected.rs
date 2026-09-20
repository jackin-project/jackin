//! Run Clippy over the crates affected by worktree changes.
//!
//! Pre-commit hook entry point (`mise run clippy-affected`):
//!
//! ```sh
//! cargo xtask clippy-affected                  # lint affected members + nested packages
//! cargo xtask clippy-affected --print-selection  # dry run: emit the selection as JSON
//! ```
//!
//! # Closure model
//!
//! Changed paths come from `git status --porcelain` (staged, unstaged, and
//! untracked). Under the stashing pre-commit hook the worktree collapses to
//! the staged snapshot, so this equals the staged set; run manually it
//! covers every worktree edit, which is what Clippy lints.
//!
//! Member selection reuses [`affected_crates::WorkspaceGraph`] unchanged:
//! longest-prefix crate mapping plus transitive reverse dependents, with
//! `Cargo.lock` / root `Cargo.toml` refinement and workspace-wide
//! widening for toolchain, Clippy, and Cargo config. Paths that cannot
//! affect a Rust build (`native/`, docs, `.github/`, mise config, …) are
//! dropped before selection so Swift-only commits skip Clippy entirely.
//!
//! Three gaps the shared graph cannot see are closed here:
//!
//! * Detached (non-member) packages — `crates/*/fuzz`, `vendor/arrayref`
//!   — are linted via `cargo clippy --manifest-path`. A member change
//!   pulls in nested packages with `path` dependencies into it; a nested
//!   change pulls in members whose resolve closure names it (covers the
//!   `[patch]`-replaced `arrayref`).
//! * `crates/jackin-lints` is excluded: it needs the nightly dylint
//!   toolchain (its own `rust-toolchain` pin) and stable Clippy cannot
//!   compile it. No member depends on it.
//! * Cross-crate file inputs (`include_str!`/`include_bytes!`/`include!`
//!   and `#[path]` reaching outside the owning crate, e.g. `docker/`
//!   scripts or sibling-crate fixtures) are found by scanning member and
//!   nested sources at runtime, so new ones are covered without code
//!   changes. Only a non-literal `include_*!` input widens to the full
//!   workspace: it compiles (via `concat!`/`env!`/…), so its target is
//!   genuinely unknowable. Unparseable `#[path]` never compiles (Clippy
//!   fails closed) and out-of-repo literals are unchangeable by any commit,
//!   so both drop silently.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::Args;

use crate::affected_crates::{self, WorkspaceGraph};
use crate::cmd;
use crate::fs_util;

#[derive(Args)]
pub(crate) struct ClippyAffectedArgs {
    /// Emit the selected members and nested packages as JSON instead of
    /// running Clippy.
    #[arg(long)]
    print_selection: bool,
}

/// Detached packages the hook must never lint: nightly-only (dylint) and
/// unbuildable under the stable hook toolchain.
const NIGHTLY_ONLY_NESTED: &[&str] = &["crates/jackin-lints"];

/// Exact Clippy flags shared with CI (velnor per-unit commands in
/// `.github/ci/project.toml` and the `lint` partition of `cargo xtask ci`,
/// which runs them workspace-wide instead of per package).
const CLIPPY_FLAGS: &[&str] = &[
    "--locked",
    "--profile",
    "test",
    "--all-targets",
    "--all-features",
];

/// A detached (non-workspace-member) package discovered under the repo.
struct NestedPackage {
    /// Workspace-relative directory containing its `Cargo.toml`.
    dir: PathBuf,
    /// `[package] name`, for the resolve-graph reverse edge.
    name: String,
    /// Workspace members it reaches via `path` dependencies.
    member_dependencies: BTreeSet<String>,
    /// A `path` dependency resolved outside every known root: include this
    /// package whenever any member is selected.
    depends_on_unknown: bool,
    /// Nightly-only: never select, drop paths under it.
    excluded: bool,
}

/// Who owns a changed path for Clippy purposes.
enum Owner {
    Member(PathBuf),
    Nested(usize),
}

/// Cross-boundary compile inputs: input path (workspace-relative) to owners.
#[derive(Default)]
struct InputOwners {
    members: BTreeMap<PathBuf, BTreeSet<String>>,
    nested: BTreeMap<PathBuf, BTreeSet<usize>>,
    unknown: bool,
    /// Files that forced `unknown`, for diagnostics.
    unknown_sources: Vec<PathBuf>,
}

impl InputOwners {
    fn mark_unknown(&mut self, file: &Path) {
        self.unknown = true;
        if !self.unknown_sources.contains(&file.to_path_buf()) {
            self.unknown_sources.push(file.to_path_buf());
        }
    }
}

pub(crate) fn run(args: ClippyAffectedArgs) -> Result<()> {
    let root = repo_root()?;
    std::env::set_current_dir(&root).context("entering repo root")?;
    let status = cmd::output(cmd::command("git").args([
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
    ]))?;
    let paths = parse_status_porcelain(&status)?;
    let metadata = affected_crates::cargo_metadata_online()?;
    let graph = WorkspaceGraph::from_metadata(metadata)?;
    let nested = discover_nested_packages(&root, &graph)?;
    let mut inputs = InputOwners::default();
    collect_cross_boundary_inputs(&root, &graph, &nested, &mut inputs)?;

    let selection = select(&graph, &nested, &inputs, &paths)?;
    if args.print_selection {
        let output = serde_json::to_string(&serde_json::json!({
            "members": selection.members,
            "nested": selection
                .nested
                .iter()
                .map(|index| nested[*index].dir.clone())
                .collect::<Vec<_>>(),
        }))
        .context("serializing clippy-affected selection")?;
        emit(&output)?;
        return Ok(());
    }
    if selection.members.is_empty() && selection.nested.is_empty() {
        emit("clippy-affected: no Rust changes; skipping")?;
        return Ok(());
    }
    if !selection.members.is_empty() {
        let mut command = cmd::command("cargo");
        command.args(member_clippy_args(&selection.members));
        cmd::run_streaming(&mut command)?;
    }
    for index in selection.nested {
        let mut command = cmd::command("cargo");
        command.args(nested_clippy_args(&nested[index].dir));
        cmd::run_streaming(&mut command)?;
    }
    Ok(())
}

struct Selection {
    members: Vec<String>,
    nested: BTreeSet<usize>,
}

fn select(
    graph: &WorkspaceGraph,
    nested: &[NestedPackage],
    inputs: &InputOwners,
    paths: &[PathBuf],
) -> Result<Selection> {
    let mut member_paths = Vec::new();
    let mut selected_nested = BTreeSet::new();
    for path in paths {
        // Cross-boundary inputs first: an embedded doc or script can carry
        // any extension, so this lookup runs before the docs skip below.
        if let Some(owners) = inputs.members.get(path) {
            member_paths.extend(owners.iter().map(|name| member_dir(graph, name)));
        }
        if let Some(owners) = inputs.nested.get(path) {
            selected_nested.extend(owners.iter().copied());
        }
        if affected_crates::is_documentation(path) {
            continue;
        }
        match classify(graph, nested, path) {
            None => {}
            // Member files collapse to their root: the shared graph maps
            // them back identically. Workspace config paths pass through
            // verbatim for lock/manifest/wide special-casing.
            Some(Owner::Member(root)) => member_paths.push(root),
            Some(Owner::Nested(index)) => {
                selected_nested.insert(index);
            }
        }
    }

    let lock_packages = paths
        .iter()
        .any(|path| path == Path::new("Cargo.lock"))
        .then(worktree_lock_packages)
        .transpose()?
        .flatten();
    let workspace_dependencies = paths
        .iter()
        .any(|path| path == Path::new("Cargo.toml"))
        .then(worktree_workspace_dependencies)
        .transpose()?
        .flatten();
    let mut members = graph.affected_with_dependencies(
        &member_paths,
        lock_packages.as_ref(),
        workspace_dependencies.as_ref(),
    );
    let wide = inputs.unknown || members == graph.all_names();
    if wide {
        members = graph.all_names();
        selected_nested.extend(
            nested
                .iter()
                .enumerate()
                .filter(|(_, package)| !package.excluded)
                .map(|(index, _)| index),
        );
    } else {
        let selected: BTreeSet<&str> = members.iter().map(String::as_str).collect();
        for (index, package) in nested.iter().enumerate() {
            if package.excluded {
                continue;
            }
            if (package.depends_on_unknown && !members.is_empty())
                || package
                    .member_dependencies
                    .iter()
                    .any(|member| selected.contains(member.as_str()))
            {
                selected_nested.insert(index);
            }
        }
    }
    for index in selected_nested.clone() {
        members.extend(graph.members_depending_on_package(&nested[index].name));
    }
    members.sort();
    members.dedup();
    Ok(Selection {
        members,
        nested: selected_nested,
    })
}

/// Map one changed path to its Clippy owner, or `None` when the path cannot
/// affect a Rust build.
fn classify(graph: &WorkspaceGraph, nested: &[NestedPackage], path: &Path) -> Option<Owner> {
    if is_excluded_nested(nested, path) {
        return None;
    }
    if let Some(index) = enclosing_nested(nested, path) {
        return Some(Owner::Nested(index));
    }
    if is_workspace_config(path) {
        return Some(Owner::Member(path.to_path_buf()));
    }
    enclosing_member_root(graph, path).map(Owner::Member)
}

fn is_excluded_nested(nested: &[NestedPackage], path: &Path) -> bool {
    nested
        .iter()
        .any(|package| package.excluded && path.starts_with(&package.dir))
}

fn enclosing_nested(nested: &[NestedPackage], path: &Path) -> Option<usize> {
    nested
        .iter()
        .enumerate()
        .filter(|(_, package)| path.starts_with(&package.dir))
        .max_by_key(|(_, package)| package.dir.components().count())
        .map(|(index, _)| index)
}

fn enclosing_member_root(graph: &WorkspaceGraph, path: &Path) -> Option<PathBuf> {
    graph
        .member_roots()
        .values()
        .filter(|root| path.starts_with(root))
        .max_by_key(|root| root.components().count())
        .cloned()
}

/// Root-level files (plus `.cargo/`) that can change a Rust build. Everything
/// else outside member and nested roots — `native/`, docs, `.github/`,
/// mise config, scripts — is Clippy-irrelevant.
fn is_workspace_config(path: &Path) -> bool {
    matches!(
        path.to_string_lossy().as_ref(),
        "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml" | "clippy.toml"
    ) || path.starts_with(".cargo")
}

fn member_dir(graph: &WorkspaceGraph, name: &str) -> PathBuf {
    graph
        .member_roots()
        .iter()
        .find_map(|(id, root)| (graph.member_name(id) == Some(name)).then(|| root.clone()))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// `git status --porcelain=v1 -z` fields: `XY␣path` entries, except renames
/// and copies which append the other side as a second bare field.
fn parse_status_porcelain(output: &[u8]) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut fields = output.split(|byte| *byte == 0);
    while let Some(field) = fields.next() {
        if field.is_empty() {
            continue;
        }
        if field.len() > 3 && field[2] == b' ' {
            let renames = field[0] == b'R' || field[0] == b'C';
            paths.push(utf8_path(&field[3..])?);
            if renames {
                let other = fields
                    .next()
                    .filter(|field| !field.is_empty())
                    .context("rename entry without second path")?;
                paths.push(utf8_path(other)?);
            }
        } else {
            paths.push(utf8_path(field)?);
        }
    }
    Ok(paths)
}

fn utf8_path(field: &[u8]) -> Result<PathBuf> {
    String::from_utf8(field.to_vec())
        .map(PathBuf::from)
        .map_err(|error| anyhow!("Git returned a non-UTF-8 path: {error}"))
}

fn repo_root() -> Result<PathBuf> {
    let root = cmd::output_string(cmd::command("git").args(["rev-parse", "--show-toplevel"]))?;
    Ok(PathBuf::from(root.trim()))
}

/// `Cargo.lock` refinement against worktree contents (under the stashing hook
/// the worktree is the staged snapshot). `Ok(None)` means "unprovable":
/// the caller widens to the full workspace.
fn worktree_lock_packages() -> Result<Option<BTreeSet<String>>> {
    let head = affected_crates::git_file("HEAD", Path::new("Cargo.lock"));
    let worktree = std::fs::read("Cargo.lock");
    let (Ok(head), Ok(worktree)) = (head, worktree) else {
        return Ok(None);
    };
    Ok(Some(affected_crates::changed_lock_packages_from_contents(
        &head, &worktree,
    )?))
}

fn worktree_workspace_dependencies() -> Result<Option<BTreeSet<String>>> {
    let head = affected_crates::git_file("HEAD", Path::new("Cargo.toml"));
    let worktree = std::fs::read("Cargo.toml");
    let (Ok(head), Ok(worktree)) = (head, worktree) else {
        return Ok(None);
    };
    affected_crates::changed_workspace_dependencies_from_contents(&head, &worktree)
}

fn discover_nested_packages(root: &Path, graph: &WorkspaceGraph) -> Result<Vec<NestedPackage>> {
    let mut manifests = BTreeSet::new();
    collect_manifests(root, root, &mut manifests)?;
    let member_dirs: BTreeSet<PathBuf> = graph.member_roots().values().cloned().collect();
    let mut nested = Vec::new();
    for dir in manifests {
        if dir.as_os_str().is_empty() || member_dirs.contains(&dir) {
            continue;
        }
        let excluded = NIGHTLY_ONLY_NESTED
            .iter()
            .any(|prefix| dir == Path::new(prefix));
        let manifest_path = root.join(&dir).join("Cargo.toml");
        let manifest = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let value = toml::from_str::<toml::Value>(&manifest)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;
        let name = value
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            .with_context(|| format!("{} has no package name", manifest_path.display()))?
            .to_owned();
        let mut package = NestedPackage {
            dir,
            name,
            member_dependencies: BTreeSet::new(),
            depends_on_unknown: false,
            excluded,
        };
        if !excluded {
            for dependency in path_dependencies(&value) {
                let resolved = normalize(&package.dir.join(&dependency));
                match enclosing_member_name(graph, &resolved) {
                    Some(member) => {
                        package.member_dependencies.insert(member);
                    }
                    None => package.depends_on_unknown = true,
                }
            }
        }
        nested.push(package);
    }
    nested.sort_by(|left, right| left.dir.cmp(&right.dir));
    Ok(nested)
}

fn collect_manifests(root: &Path, dir: &Path, manifests: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("{} is outside {}", path.display(), root.display()))?;
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| name == ".git" || name == "target")
            {
                continue;
            }
            collect_manifests(root, &path, manifests)?;
        } else if path.file_name().is_some_and(|name| name == "Cargo.toml") {
            manifests.insert(
                relative
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

fn path_dependencies(manifest: &toml::Value) -> Vec<PathBuf> {
    ["dependencies", "dev-dependencies", "build-dependencies"]
        .into_iter()
        .filter_map(|section| manifest.get(section))
        .filter_map(toml::Value::as_table)
        .flat_map(|table| table.values())
        .filter_map(|dependency| dependency.get("path"))
        .filter_map(toml::Value::as_str)
        .map(PathBuf::from)
        .collect()
}

fn enclosing_member_name(graph: &WorkspaceGraph, path: &Path) -> Option<String> {
    graph
        .member_roots()
        .iter()
        .filter(|(_, root)| path.starts_with(root))
        .max_by_key(|(_, root)| root.components().count())
        .and_then(|(id, _)| graph.member_name(id).map(str::to_owned))
}

/// Scan member and nested `.rs` sources for file inputs that cross the
/// owning root (`include_str!`/`include_bytes!`/`include!`, `#[path]`).
fn collect_cross_boundary_inputs(
    root: &Path,
    graph: &WorkspaceGraph,
    nested: &[NestedPackage],
    inputs: &mut InputOwners,
) -> Result<()> {
    let mut scopes: Vec<(PathBuf, Option<String>, Option<usize>)> = graph
        .member_roots()
        .iter()
        .filter_map(|(id, dir)| {
            graph
                .member_name(id)
                .map(|name| (dir.clone(), Some(name.to_owned()), None))
        })
        .collect();
    scopes.extend(
        nested
            .iter()
            .enumerate()
            .filter(|(_, package)| !package.excluded)
            .map(|(index, package)| (package.dir.clone(), None, Some(index))),
    );
    for (scope, member, nested_index) in scopes {
        let dir = root.join(&scope);
        if !dir.is_dir() {
            continue;
        }
        collect_scope_inputs(root, &dir, &scope, member.as_deref(), nested_index, inputs)?;
    }
    Ok(())
}

fn collect_scope_inputs(
    root: &Path,
    dir: &Path,
    scope: &Path,
    member: Option<&str>,
    nested_index: Option<usize>,
    inputs: &mut InputOwners,
) -> Result<()> {
    for entry in fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect_scope_inputs(root, &path, scope, member, nested_index, inputs)?;
            continue;
        }
        if path.extension().is_some_and(|extension| extension == "rs") {
            let source = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("{} is outside {}", path.display(), root.display()))?
                .to_path_buf();
            scan_source_inputs(&source, &relative, scope, member, nested_index, inputs);
        }
    }
    Ok(())
}

/// Textual scan: every `include_*!(…)` / `#[path = …]` literal that resolves
/// outside `scope` is recorded; anything unresolvable sets `unknown` (wide).
fn scan_source_inputs(
    source: &str,
    file: &Path,
    scope: &Path,
    member: Option<&str>,
    nested_index: Option<usize>,
    inputs: &mut InputOwners,
) {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(rest) = source[index..].strip_prefix("include_str!") {
            index += "include_str!".len()
                + consume_include(rest, file, scope, member, nested_index, inputs);
        } else if let Some(rest) = source[index..].strip_prefix("include_bytes!") {
            index += "include_bytes!".len()
                + consume_include(rest, file, scope, member, nested_index, inputs);
        } else if source[index..].starts_with("include!")
            && !source[index..].starts_with("include_")
        {
            index += "include!".len()
                + consume_include(
                    &source[index + "include!".len()..],
                    file,
                    scope,
                    member,
                    nested_index,
                    inputs,
                );
        } else if let Some(rest) = source[index..].strip_prefix("#[path") {
            // `#[pathname = …]` is not the module attribute; skip it.
            if rest.starts_with(|char: char| char.is_whitespace() || char == '=') {
                index += "#[path".len()
                    + consume_path_attribute(rest, file, scope, member, nested_index, inputs);
            } else {
                index += "#[path".len();
            }
        } else {
            index += source[index..].chars().next().map_or(1, char::len_utf8);
        }
    }
}

/// Consume the delimiter after an `include_*!`; returns bytes advanced
/// within `rest`. Macro text without a following delimiter is a mention in
/// prose, a comment, or a string fragment — skipped silently. A delimiter
/// followed by anything but a plain string literal widens the selection.
fn consume_include(
    rest: &str,
    file: &Path,
    scope: &Path,
    member: Option<&str>,
    nested_index: Option<usize>,
    inputs: &mut InputOwners,
) -> usize {
    let trimmed = skip_trivia(rest);
    if !matches!(trimmed.as_bytes().first(), Some(b'(' | b'[' | b'{')) {
        return 0;
    }
    let after_opener = &trimmed[1..];
    let head = rest.len() - after_opener.len();
    let candidate = skip_trivia(after_opener);
    if let Some((literal, len)) = parse_string_literal(candidate) {
        record_literal(literal, file, scope, member, nested_index, inputs);
        head + (after_opener.len() - candidate.len()) + len
    } else {
        inputs.mark_unknown(file);
        1
    }
}

fn consume_path_attribute(
    rest: &str,
    file: &Path,
    scope: &Path,
    member: Option<&str>,
    nested_index: Option<usize>,
    inputs: &mut InputOwners,
) -> usize {
    // Unlike `include_*!`, a non-literal `#[path = …]` never compiles, so
    // Clippy fails closed on it and silent recovery is sound here.
    let trimmed = skip_trivia(rest);
    let Some(after_equals) = trimmed.strip_prefix('=') else {
        return 0;
    };
    let head = rest.len() - after_equals.len();
    let candidate = skip_trivia(after_equals);
    if let Some((literal, len)) = parse_string_literal(candidate) {
        record_literal(literal, file, scope, member, nested_index, inputs);
        head + (after_equals.len() - candidate.len()) + len
    } else {
        1
    }
}

/// Skip whitespace and comments (nesting block comments); unterminated
/// input consumes the rest, which then fails to parse as an invocation.
fn skip_trivia(mut source: &str) -> &str {
    loop {
        let trimmed = source.trim_start_matches(|char: char| char.is_whitespace());
        if let Some(rest) = trimmed.strip_prefix("//") {
            match rest.find('\n') {
                Some(index) => source = &rest[index + 1..],
                None => return "",
            }
        } else if trimmed.starts_with("/*") {
            match skip_block_comment(trimmed) {
                Some(rest) => source = rest,
                None => return "",
            }
        } else {
            return trimmed;
        }
    }
}

fn skip_block_comment(source: &str) -> Option<&str> {
    let bytes = source.as_bytes();
    let mut depth = 0_usize;
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == b'/' && bytes[index + 1] == b'*' {
            depth += 1;
            index += 2;
        } else if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            depth -= 1;
            index += 2;
            if depth == 0 {
                return Some(&source[index..]);
            }
        } else {
            index += 1;
        }
    }
    None
}

/// Parse a `"…"` literal; `None` on non-literals and non-trivial escapes.
fn parse_string_literal(source: &str) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut literal = String::new();
    let mut index = 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return Some((literal, index + 1)),
            b'\\' => {
                index += 1;
                match bytes.get(index) {
                    Some(b'"') => literal.push('"'),
                    Some(b'\\') => literal.push('\\'),
                    _ => return None,
                }
                index += 1;
            }
            _ => {
                let char = source[index..].chars().next()?;
                index += char.len_utf8();
                literal.push(char);
            }
        }
    }
    None
}

fn record_literal(
    literal: String,
    file: &Path,
    scope: &Path,
    member: Option<&str>,
    nested_index: Option<usize>,
    inputs: &mut InputOwners,
) {
    let Some(parent) = file.parent() else {
        inputs.mark_unknown(file);
        return;
    };
    let resolved = normalize(&parent.join(&literal));
    if resolved.starts_with(scope) {
        return;
    }
    // Escaping the repo root (or an empty literal, which never compiles):
    // no commit can affect the outcome, so drop rather than widen.
    if resolved.components().next().is_none()
        || matches!(resolved.components().next(), Some(Component::ParentDir))
    {
        return;
    }
    if let Some(name) = member {
        inputs
            .members
            .entry(resolved.clone())
            .or_default()
            .insert(name.to_owned());
    }
    if let Some(index) = nested_index {
        inputs.nested.entry(resolved).or_default().insert(index);
    }
}

/// Lexical `..`/`.` resolution without touching the filesystem (symlink-safe).
fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if parts
                    .last()
                    .is_some_and(|last| matches!(last, Component::Normal(_)))
                {
                    parts.pop();
                } else {
                    parts.push(component);
                }
            }
            Component::CurDir => {}
            other => parts.push(other),
        }
    }
    parts.into_iter().collect()
}

fn member_clippy_args(members: &[String]) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["clippy".into()];
    args.extend(CLIPPY_FLAGS.iter().map(OsString::from));
    for member in members {
        args.push("-p".into());
        args.push(member.into());
    }
    args.extend(["--".into(), "-D".into(), "warnings".into()]);
    args
}

fn nested_clippy_args(dir: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["clippy".into()];
    args.push("--manifest-path".into());
    args.push(dir.join("Cargo.toml").into());
    args.extend(CLIPPY_FLAGS.iter().map(OsString::from));
    args.extend(["--".into(), "-D".into(), "warnings".into()]);
    args
}

fn emit(line: &str) -> Result<()> {
    writeln!(io::stdout().lock(), "{line}").context("writing clippy-affected output")
}

#[cfg(test)]
mod tests;
