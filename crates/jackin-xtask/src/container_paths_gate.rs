//! Shrink-only gate: production `"/jackin` string literals + forbidden-root audit.
//!
//! New container-side paths must go through `jackin_core::container_paths`.
//! Residual literals (Dockerfile template bodies, the chokepoint module
//! itself) are recorded in `container-path-allowlist.toml` and may only shrink.
//!
//! Forbidden absolute roots (`/run`, `/var`, `/opt`, `/etc`, `/tmp/jackin*`) in
//! production string literals must be allowlisted with a reason under
//! `[[forbidden-roots]]` (shrink-only).
//!
//! ```sh
//! cargo xtask lint container-paths
//! cargo xtask lint container-paths --print-allowlist
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;
use syn::visit::Visit;

use crate::docs::repo_root;

const ALLOWLIST_PATH: &str = "container-path-allowlist.toml";
const RERUN: &str = "cargo xtask lint container-paths";
const CHOKEPOINT: &str = "crates/jackin-core/src/container_paths.rs";

#[derive(Args, Debug)]
pub(crate) struct LintContainerPathsArgs {
    /// Print a regenerated allowlist TOML for the current residual set
    /// (does not write the file).
    #[arg(long)]
    print_allowlist: bool,
}

#[derive(Debug, Deserialize)]
struct Allowlist {
    #[serde(default)]
    file: Vec<AllowlistFile>,
    #[serde(default, rename = "forbidden-roots")]
    forbidden_roots: Vec<ForbiddenRootEntry>,
}

#[derive(Debug, Deserialize)]
struct AllowlistFile {
    path: String,
    literals: usize,
    /// Human-readable residual rationale; not read by the gate.
    #[serde(default)]
    #[expect(dead_code, reason = "documentational allowlist field")]
    note: String,
}

#[derive(Debug, Deserialize, Clone)]
struct ForbiddenRootEntry {
    path: String,
    file: String,
    #[serde(default)]
    #[expect(dead_code, reason = "documentational allowlist field")]
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LiteralHit {
    file: String,
    line: usize,
    literal: String,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the gate report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

pub(crate) fn enforce() -> Result<()> {
    run(LintContainerPathsArgs {
        print_allowlist: false,
    })
}

pub(crate) fn run(args: LintContainerPathsArgs) -> Result<()> {
    let root = repo_root()?;
    let jackin_hits = measure_jackin_literals(&root)?;
    let forbidden_hits = measure_forbidden_roots(&root)?;

    if args.print_allowlist {
        emit("# Regenerated residual `/jackin` + forbidden-root ledger. Review, then write to");
        emit(&format!(
            "# {ALLOWLIST_PATH}. Shrink-only — never grow a row."
        ));
        emit("");
        let mut by_file: BTreeMap<String, usize> = BTreeMap::new();
        for hit in &jackin_hits {
            *by_file.entry(hit.file.clone()).or_default() += 1;
        }
        for (path, n) in &by_file {
            emit("[[file]]");
            emit(&format!("path = {path:?}"));
            emit(&format!("literals = {n}"));
            emit("note = \"\"");
            emit("");
        }
        for hit in &forbidden_hits {
            emit("[[forbidden-roots]]");
            emit(&format!("path = {:?}", hit.literal));
            emit(&format!("file = {:?}", hit.file));
            emit("reason = \"\"");
            emit("");
        }
        return Ok(());
    }

    let allowlist = read_allowlist(&root)?;
    let mut problems = Vec::new();

    // --- /jackin ledger (file counts + file:line in growth reports) ---
    let mut measured_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut hits_by_file: BTreeMap<String, Vec<&LiteralHit>> = BTreeMap::new();
    for hit in &jackin_hits {
        *measured_counts.entry(hit.file.clone()).or_default() += 1;
        hits_by_file.entry(hit.file.clone()).or_default().push(hit);
    }

    let mut recorded: BTreeMap<String, usize> = BTreeMap::new();
    for row in &allowlist.file {
        recorded.insert(row.path.clone(), row.literals);
    }

    for (path, budgeted) in &recorded {
        match measured_counts.get(path) {
            None => problems.push(format!(
                "{path}: listed in {ALLOWLIST_PATH} with {budgeted} literals but no longer has any (remove the stale allowlist row)"
            )),
            Some(&n) if n < *budgeted => problems.push(format!(
                "{path}: shrunk from {budgeted} to {n} literals — update {ALLOWLIST_PATH} to {n} (shrink-only ratchet)"
            )),
            Some(&n) if n > *budgeted => {
                let lines = hits_by_file
                    .get(path)
                    .map(|hs| {
                        hs.iter()
                            .map(|h| format!("{}:{}", h.file, h.line))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                problems.push(format!(
                    "{path}: grew from {budgeted} to {n} `/jackin` literals at [{lines}] — route new container paths through jackin_core::container_paths; regenerate: cargo xtask lint container-paths --print-allowlist"
                ));
            }
            Some(_) => {}
        }
    }

    for (path, n) in &measured_counts {
        if !recorded.contains_key(path) {
            let lines = hits_by_file
                .get(path)
                .map(|hs| {
                    hs.iter()
                        .map(|h| format!("{}:{}", h.file, h.line))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            problems.push(format!(
                "{path}: {n} unallowlisted `/jackin` production literal(s) at [{lines}] — route through jackin_core::container_paths; if residual is required (e.g. Dockerfile template), add a shrink-only row via: cargo xtask lint container-paths --print-allowlist"
            ));
        }
    }

    // --- forbidden roots (token-aware string literals, file:line) ---
    let mut allowed_forbidden: BTreeMap<(String, String), usize> = BTreeMap::new();
    for row in &allowlist.forbidden_roots {
        *allowed_forbidden
            .entry((row.file.clone(), row.path.clone()))
            .or_default() += 1;
    }
    let mut measured_forbidden: BTreeMap<(String, String), usize> = BTreeMap::new();
    for hit in &forbidden_hits {
        // Chokepoint module may document forbidden roots — OK.
        if hit.file == CHOKEPOINT {
            continue;
        }
        *measured_forbidden
            .entry((hit.file.clone(), hit.literal.clone()))
            .or_default() += 1;
    }

    for hit in &forbidden_hits {
        if hit.file == CHOKEPOINT {
            continue;
        }
        let key = (hit.file.clone(), hit.literal.clone());
        if !allowed_forbidden.contains_key(&key) {
            problems.push(format!(
                "{}:{}: {:?} — route through jackin_core::container_paths or add a reasoned exception in {ALLOWLIST_PATH} [[forbidden-roots]]",
                hit.file, hit.line, hit.literal
            ));
        }
    }
    for (key, budgeted) in &allowed_forbidden {
        match measured_forbidden.get(key) {
            None => problems.push(format!(
                "{}: forbidden-root {:?} listed in {ALLOWLIST_PATH} but no longer present (remove the stale row)",
                key.0, key.1
            )),
            Some(&n) if n < *budgeted => problems.push(format!(
                "{}: forbidden-root {:?} shrunk from {budgeted} to {n} — update {ALLOWLIST_PATH} (shrink-only)",
                key.0, key.1
            )),
            Some(&n) if n > *budgeted => problems.push(format!(
                "{}: forbidden-root {:?} grew from {budgeted} to {n} — route through jackin_core::container_paths or update {ALLOWLIST_PATH} with reason",
                key.0, key.1
            )),
            Some(_) => {}
        }
    }

    if problems.is_empty() {
        let total: usize = measured_counts.values().sum();
        emit(&format!(
            "container-path gate OK — {} residual `/jackin` literal(s) across {} file(s); {} forbidden-root exception(s) (shrink-only)",
            total,
            measured_counts.len(),
            allowlist.forbidden_roots.len()
        ));
        return Ok(());
    }

    problems.sort();
    bail!(
        "{} container-path violation(s):\n  {}\n\nfix: route new container paths through jackin_core::container_paths; regenerate: cargo xtask lint container-paths --print-allowlist\nre-run: {RERUN}",
        problems.len(),
        problems.join("\n  ")
    )
}

fn measure_jackin_literals(root: &Path) -> Result<Vec<LiteralHit>> {
    // Source-substring ledger (stable vs the pre-syn count of `"\/jackin`).
    // Syn is used for forbidden-root audit (string-literal contents only).
    let crates_dir = root.join("crates");
    let mut hits = Vec::new();
    for entry in walkdir_rs_files(&crates_dir)? {
        let rel = entry
            .strip_prefix(root)
            .unwrap_or(&entry)
            .to_string_lossy()
            .replace('\\', "/");
        if !is_production_src(&rel, &entry) {
            continue;
        }
        let text =
            fs::read_to_string(&entry).with_context(|| format!("reading {}", entry.display()))?;
        for (line_no, line) in text.lines().enumerate() {
            let mut rest = line;
            while let Some(idx) = rest.find("\"/jackin") {
                let after = &rest[idx + 1..]; // drop leading quote
                let end = after.find('"').unwrap_or(after.len());
                let lit = after[..end].to_owned();
                hits.push(LiteralHit {
                    file: rel.clone(),
                    line: line_no + 1,
                    literal: lit,
                });
                rest = &after[end.min(after.len())..];
                if rest.is_empty() {
                    break;
                }
                rest = &rest[rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1)..];
            }
        }
    }
    hits.sort();
    Ok(hits)
}

fn measure_forbidden_roots(root: &Path) -> Result<Vec<LiteralHit>> {
    let crates_dir = root.join("crates");
    let mut hits = Vec::new();
    for entry in walkdir_rs_files(&crates_dir)? {
        let rel = entry
            .strip_prefix(root)
            .unwrap_or(&entry)
            .to_string_lossy()
            .replace('\\', "/");
        if !is_production_src(&rel, &entry) {
            continue;
        }
        let text =
            fs::read_to_string(&entry).with_context(|| format!("reading {}", entry.display()))?;
        let Ok(file) = syn::parse_file(&text) else {
            continue;
        };
        let mut visitor = LiteralVisitor {
            file: rel,
            source: &text,
            hits: &mut hits,
        };
        visitor.visit_file(&file);
    }
    hits.sort();
    Ok(hits)
}

fn is_production_src(rel: &str, entry: &Path) -> bool {
    if rel.contains("jackin-xtask/") {
        return false;
    }
    if !rel.contains("/src/") {
        return false;
    }
    let file_name = entry.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if file_name == "tests.rs" || rel.contains("/tests/") || rel.contains("/tests.rs/") {
        return false;
    }
    if entry.components().any(|c| c.as_os_str() == "tests") {
        return false;
    }
    true
}

fn is_forbidden_root_literal(s: &str) -> bool {
    s == "/run"
        || s.starts_with("/run/")
        || s == "/var"
        || s.starts_with("/var/")
        || s == "/opt"
        || s.starts_with("/opt/")
        || s == "/etc"
        || s.starts_with("/etc/")
        || s.starts_with("/tmp/jackin")
}

struct LiteralVisitor<'a> {
    file: String,
    source: &'a str,
    hits: &'a mut Vec<LiteralHit>,
}

impl<'ast> Visit<'ast> for LiteralVisitor<'_> {
    fn visit_lit_str(&mut self, lit: &'ast syn::LitStr) {
        let value = lit.value();
        if is_forbidden_root_literal(&value) {
            let line = line_of_span(self.source, lit.span());
            self.hits.push(LiteralHit {
                file: self.file.clone(),
                line,
                literal: value,
            });
        }
        syn::visit::visit_lit_str(self, lit);
    }
}

fn line_of_span(source: &str, span: proc_macro2::Span) -> usize {
    // syn/proc_macro2 spans are 1-indexed lines when using the default parser.
    let start = span.start();
    if start.line > 0 {
        return start.line;
    }
    // Fallback: byte offset if available via unstable APIs is unavailable; 1.
    let _ = source;
    1
}

fn walkdir_rs_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(path);
            }
        }
        Ok(())
    }
    walk(dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn read_allowlist(root: &Path) -> Result<Allowlist> {
    let path = root.join(ALLOWLIST_PATH);
    if !path.exists() {
        bail!(
            "missing {ALLOWLIST_PATH} — create it with: cargo xtask lint container-paths --print-allowlist"
        );
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Turso sole-owner rule: only `jackin-usage` may depend on or import Turso.
pub(crate) fn check_turso_sole_owner(root: &Path) -> Result<()> {
    let mut problems = Vec::new();
    let crates_dir = root.join("crates");
    for entry in
        fs::read_dir(&crates_dir).with_context(|| format!("reading {}", crates_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let cargo = path.join("Cargo.toml");
        if !cargo.exists() {
            continue;
        }
        let text = fs::read_to_string(&cargo)?;
        if name != "jackin-usage" && cargo_declares_turso(&text) {
            problems.push(format!(
                "crates/{name}/Cargo.toml: declares turso/libsql dependency — jackin-usage is the sole Turso owner (roadmap Rust-enforcement item 8)"
            ));
        }
    }

    for entry in walkdir_rs_files(&crates_dir)? {
        let rel = entry
            .strip_prefix(root)
            .unwrap_or(&entry)
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with("crates/jackin-usage/") {
            continue;
        }
        if rel.contains("jackin-xtask/") {
            continue;
        }
        let text = fs::read_to_string(&entry)?;
        for (line_no, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            // Token-level: path imports / use, not bare log strings.
            if let Some(col) = turso_import_column(trimmed) {
                let _ = col;
                problems.push(format!(
                    "{rel}:{}: Turso import/path — jackin-usage is the sole Turso owner; move store code there or drop the import",
                    line_no + 1
                ));
            }
        }
    }

    if problems.is_empty() {
        emit("turso sole-owner gate OK — only jackin-usage owns turso/libsql");
        return Ok(());
    }
    problems.sort();
    bail!(
        "{} turso sole-owner violation(s):\n  {}\n\nfix: keep Turso confined to crates/jackin-usage",
        problems.len(),
        problems.join("\n  ")
    )
}

fn cargo_declares_turso(cargo_toml: &str) -> bool {
    for line in cargo_toml.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        if t.starts_with("turso ")
            || t.starts_with("turso=")
            || t.starts_with("libsql ")
            || t.starts_with("libsql=")
        {
            return true;
        }
    }
    false
}

fn turso_import_column(line: &str) -> Option<usize> {
    // Match `use turso` / `use turso::` / `turso::Builder` path forms; ignore
    // string/log mentions by requiring identifier boundaries without quotes.
    if line.contains('"') && !line.contains("use turso") {
        // Likely a string-only mention when no use-form present.
        if !line.contains("turso::") {
            return None;
        }
    }
    for needle in ["use turso::", "use turso ", "use turso;", "turso::"] {
        if let Some(idx) = line.find(needle) {
            // Skip if inside a line comment already handled; skip if inside quotes.
            let before = &line[..idx];
            if before.matches('"').count() % 2 == 1 {
                continue;
            }
            return Some(idx + 1);
        }
    }
    None
}

#[cfg(test)]
mod tests;
