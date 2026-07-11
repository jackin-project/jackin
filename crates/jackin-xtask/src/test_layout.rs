//! Test-file-layout gate (codebase-health-enforcement).
//!
//! Enforces the workspace hard rule (see `crates/AGENTS.md` → "Tests in own
//! file"): every module's tests live in a single sibling `tests.rs`, declared
//! with `#[cfg(test)] mod tests;`. Four things are forbidden in `crates/*/src`:
//!
//!   1. An inline `#[cfg(test)] mod <name> { … }` body in any non-`tests.rs`
//!      source file — the body must move to a sibling `tests.rs`.
//!   2. A direct unit-test function attribute in a non-`tests.rs` source file
//!      — the test must move to a sibling `tests.rs`.
//!   3. A `tests.rs` that declares child modules (`mod x;` / `mod x { … }`) —
//!      all tests stay inline in the one file, never split across sub-modules.
//!   4. A `tests/` directory under `src/` holding split-out test files — the
//!      canonical layout is `foo.rs` + `foo/tests.rs`, not `foo/tests/*.rs`.
//!
//! ```sh
//! cargo xtask lint tests                  # enforce, fail on new violations
//! cargo xtask lint tests --print-allowlist  # emit a fresh allowlist TOML
//! ```
//!
//! Files that violate today are grandfathered in `test-layout-allowlist.toml`;
//! the gate fails on any violation **not** in the allowlist, and (shrink-only
//! ratchet) on any allowlisted path that no longer violates — the file was
//! fixed or never existed, so its entry must be removed. The list may only
//! shrink (the same ratchet as the file-size gate). Integration tests under
//! `crates/*/tests/` (a sibling of `src/`, not under it) are Cargo's own test
//! target and are not scanned.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::{Deserialize, Serialize};

use crate::docs::repo_root;

const ALLOWLIST_PATH: &str = "test-layout-allowlist.toml";

#[derive(Args, Debug)]
pub(crate) struct LintTestsArgs {
    /// Emit the current set of violating files as a fresh allowlist TOML on
    /// stdout and exit. Use after fixing or discovering files: redirect over
    /// `test-layout-allowlist.toml` (pruning fixed entries) and commit.
    #[arg(long)]
    print_allowlist: bool,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Allowlist {
    /// Repo-relative paths of files grandfathered as known violations.
    #[serde(default)]
    files: Vec<String>,
}

/// Run the test-layout gate in enforce mode. The umbrella `cargo xtask lint`
/// entry point uses this.
pub(crate) fn enforce() -> Result<()> {
    run(LintTestsArgs {
        print_allowlist: false,
    })
}

pub(crate) fn run(args: LintTestsArgs) -> Result<()> {
    let root = repo_root()?;
    let violations = measure_violations(&root)?;

    if args.print_allowlist {
        print_allowlist(&violations);
        return Ok(());
    }

    let allowed = read_allowlist(&root)?;
    check(&violations, &allowed)
}

/// Walk every `crates/*/src` tree and collect `relative path → reason` for each
/// file that breaks the test-layout rule.
pub(crate) fn measure_violations(root: &Path) -> Result<BTreeMap<String, String>> {
    let crates_dir = root.join("crates");
    if !crates_dir.is_dir() {
        bail!("`crates/` not found under {}", root.display());
    }
    let mut out = BTreeMap::new();
    for entry in
        fs::read_dir(&crates_dir).with_context(|| format!("reading {}", crates_dir.display()))?
    {
        let src = entry?.path().join("src");
        if src.is_dir() {
            walk(&src, root, &mut out)?;
        }
    }
    Ok(out)
}

fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                // A `tests/` directory under src/ is the split-test anti-pattern.
                collect_rs(
                    &path,
                    root,
                    out,
                    "split-out test files: use a single sibling `tests.rs`, not a `tests/` directory",
                )?;
                continue;
            }
            walk(&path, root, out)?;
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "rs") {
            let rel = rel_path(&path, root);
            let text =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            let is_tests_rs = path.file_name().is_some_and(|n| n == "tests.rs");
            let reason = if is_tests_rs {
                tests_rs_violation(&text)
            } else {
                non_tests_rs_violation(&text)
            };
            if let Some(reason) = reason {
                out.insert(rel, reason.to_owned());
            }
        }
    }
    Ok(())
}

fn collect_rs(
    dir: &Path,
    root: &Path,
    out: &mut BTreeMap<String, String>,
    reason: &str,
) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rs(&path, root, out, reason)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.insert(rel_path(&path, root), reason.to_owned());
        }
    }
    Ok(())
}

fn rel_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// `Some(reason)` if a non-`tests.rs` file has an inline `#[cfg(test)] mod X { … }`.
/// A `#[cfg(test)] mod tests;` *declaration* (semicolon) is the correct form and
/// is not flagged.
fn non_tests_rs_violation(text: &str) -> Option<&'static str> {
    direct_test_attr_violation(text).or_else(|| inline_test_module_violation(text))
}

/// `Some(reason)` if a non-`tests.rs` file has a direct test function attribute.
/// Comments that mention test attributes are not flagged.
fn direct_test_attr_violation(text: &str) -> Option<&'static str> {
    text.lines().find_map(|line| {
        let t = line.trim_start();
        is_test_attr(t).then_some(
            "direct test function attribute in non-`tests.rs` file — move the test to a sibling `tests.rs`",
        )
    })
}

fn is_test_attr(line: &str) -> bool {
    const TEST_ATTR: &str = concat!("#", "[test]");
    const TOKIO_TEST_ATTR: &str = concat!("#", "[tokio::test");
    const RSTEST_ATTR: &str = concat!("#", "[rstest");

    line == TEST_ATTR || line.starts_with(TOKIO_TEST_ATTR) || line.starts_with(RSTEST_ATTR)
}

fn inline_test_module_violation(text: &str) -> Option<&'static str> {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with("#[cfg(test)]") {
            continue;
        }
        // The declaration the attribute applies to is the next significant line
        // (skip blank lines, comments, and stacked attributes).
        for next in &lines[i + 1..] {
            let t = next.trim();
            if t.is_empty() || t.starts_with("//") || t.starts_with("#[") {
                continue;
            }
            if mod_has_body(t) == Some(true) {
                return Some(
                    "inline `#[cfg(test)]` test module — move the body to a sibling `tests.rs` and use `mod tests;`",
                );
            }
            break;
        }
    }
    None
}

/// `Some(reason)` if a `tests.rs` declares any child module.
fn tests_rs_violation(text: &str) -> Option<&'static str> {
    text.lines().find_map(|line| {
        mod_has_body(line.trim()).map(|_| {
            "`tests.rs` must not declare child modules — keep every test inline in this one file"
        })
    })
}

/// Classify a trimmed line as a module declaration:
/// `Some(true)` = `mod X { … }` (inline body), `Some(false)` = `mod X;`
/// (file declaration), `None` = not a module declaration. Tolerates an optional
/// `pub` / `pub(…)` visibility prefix.
fn mod_has_body(line: &str) -> Option<bool> {
    let mut rest = line.trim_start();
    if let Some(after_pub) = rest.strip_prefix("pub") {
        let after_pub = after_pub.trim_start();
        rest = after_pub
            .strip_prefix('(')
            .and_then(|r| r.split_once(')'))
            .map_or(after_pub, |(_, tail)| tail.trim_start());
    }
    let after_mod = rest.strip_prefix("mod ")?;
    Some(after_mod.contains('{'))
}

fn read_allowlist(root: &Path) -> Result<BTreeSet<String>> {
    let path = root.join(ALLOWLIST_PATH);
    if !path.exists() {
        return Ok(BTreeSet::new());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let allowlist: Allowlist =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(allowlist.files.into_iter().collect())
}

fn check(violations: &BTreeMap<String, String>, allowed: &BTreeSet<String>) -> Result<()> {
    // Shrink-only ratchet: an allowlist row whose path is not a current
    // violation — the file was fixed, or never existed — is stale and must be
    // removed. The gate fails on stale rows instead of only printing a note.
    let stale: Vec<&String> = allowed
        .iter()
        .filter(|p| !violations.contains_key(*p))
        .collect();

    let new: Vec<(&String, &String)> = violations
        .iter()
        .filter(|(p, _)| !allowed.contains(*p))
        .collect();

    if stale.is_empty() && new.is_empty() {
        emit(&format!(
            "test-layout gate OK — {} file(s) scanned-as-violations, all grandfathered ({} allowlisted)",
            violations.len(),
            allowed.len()
        ));
        return Ok(());
    }

    let mut problems: Vec<String> = Vec::new();
    for path in &stale {
        problems.push(format!(
            "{path}: listed in {ALLOWLIST_PATH} but no longer violates (remove the stale allowlist entry)"
        ));
    }
    for (path, reason) in &new {
        problems.push(format!("{path}: {reason}"));
    }
    problems.sort();

    bail!(
        "{} test-layout violation(s):\n  {}\n\nMove tests into a sibling `tests.rs` (see crates/AGENTS.md). To refresh the allowlist, run `cargo xtask lint tests --print-allowlist`.",
        problems.len(),
        problems.join("\n  ")
    )
}

fn print_allowlist(violations: &BTreeMap<String, String>) {
    let allowlist = Allowlist {
        files: violations.keys().cloned().collect(),
    };
    let body = toml::to_string_pretty(&allowlist)
        .unwrap_or_else(|err| format!("# failed to serialize allowlist: {err}\n"));
    emit(&format!(
        "# Test-layout ratchet — grandfathered violations of the one-tests.rs rule.\n# The gate (`cargo xtask lint tests`) fails on any violation NOT listed here.\n# Delete an entry when its file is fixed; the list may only shrink.\n{body}"
    ));
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; gate output is its user-facing result"
)]
fn emit(message: &str) {
    println!("{message}");
}

#[cfg(test)]
mod tests;
