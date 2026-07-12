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
//! cargo xtask lint tests --print-allowlist  # emit fresh ratchet family keys
//! ```
//!
//! Production enforcement is a thin shim over [`crate::ratchet`] for the
//! `test-layout` presence family in `ratchet.toml`. Measurement
//! (`measure_violations`) stays here for the ratchet provider. Pure `check`
//! helpers below exist only for unit characterization tests.

use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::ratchet::{self, TEST_LAYOUT_FAMILIES};

#[derive(Args, Debug)]
pub(crate) struct LintTestsArgs {
    /// Emit regenerated `ratchet.toml` `test-layout` family keys on stdout.
    /// Prefer `cargo xtask lint ratchet --print test-layout` for the same data.
    #[arg(long)]
    print_allowlist: bool,
}

/// Run the test-layout gate in enforce mode. The umbrella `cargo xtask lint`
/// entry point uses this.
pub(crate) fn enforce() -> Result<()> {
    run(LintTestsArgs {
        print_allowlist: false,
    })
}

pub(crate) fn run(args: LintTestsArgs) -> Result<()> {
    if args.print_allowlist {
        return ratchet::print_families(TEST_LAYOUT_FAMILIES);
    }
    // Scoped ratchet enforce; OK line uses the engine's family-scoped message.
    let outcome = ratchet::check_families_at_root(TEST_LAYOUT_FAMILIES)?;
    if outcome.problems.is_empty() {
        let root = crate::docs::repo_root()?;
        let violations = measure_violations(&root)?;
        emit(&format!(
            "test-layout gate OK — {} file(s) scanned-as-violations, all grandfathered (ratchet.toml family test-layout)",
            violations.len(),
        ));
        return Ok(());
    }
    let mut problems: Vec<&str> = outcome.problems.iter().map(|p| p.message.as_str()).collect();
    problems.sort();
    bail!(
        "{} test-layout violation(s):\n  {}\n\nMove tests into a sibling `tests.rs` (see crates/AGENTS.md). To refresh the allowlist, run `cargo xtask lint tests --print-allowlist` (or `cargo xtask lint ratchet --print test-layout`).",
        problems.len(),
        problems.join("\n  ")
    )
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

/// Pure presence check (unit characterization tests).
#[cfg(test)]
fn check(violations: &BTreeMap<String, String>, allowed: &BTreeSet<String>) -> Result<()> {
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
            "{path}: listed in ratchet.toml family test-layout but no longer violates (remove the stale allowlist entry)"
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

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; gate output is its user-facing result"
)]
fn emit(message: &str) {
    println!("{message}");
}

#[cfg(test)]
mod tests;
