//! Test-file-layout gate from the completed codebase-health track.
//!
//! Enforces the workspace hard rule (see `crates/AGENTS.md` → "Tests in own
//! file"): every module's tests live in a single sibling `tests.rs`, declared
//! exactly as `#[cfg(test)] mod tests;`. Five things are forbidden in `crates/*/src`:
//!
//!   1. An inline `#[cfg(test)] mod <name> { … }` body in any non-`tests.rs`
//!      source file — the body must move to a sibling `tests.rs`.
//!   2. An external `#[cfg(test)]` module declaration other than the exact
//!      two-line `#[cfg(test)] mod tests;` form — Rust resolves the sibling
//!      `tests.rs` without `#[path]`.
//!   3. A direct unit-test function attribute in a non-`tests.rs` source file
//!      — the test must move to a sibling `tests.rs`.
//!   4. A `tests.rs` that declares child modules (`mod x;` / `mod x { … }`) —
//!      all tests stay inline in the one file, never split across sub-modules.
//!   5. A `tests/` directory under `src/` holding split-out test files — the
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
use proc_macro2::LineColumn;
use syn::parse::Parser as _;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned as _;
use syn::{Attribute, Item, ItemMod, Meta, Token, Visibility};

use crate::ratchet::{self, TEST_LAYOUT_FAMILIES};
use crate::report::{self, FormatArgs};

#[derive(Args, Debug)]
pub(crate) struct LintTestsArgs {
    #[command(flatten)]
    output: FormatArgs,
    /// Emit regenerated `ratchet.toml` `test-layout` family keys on stdout.
    /// Prefer `cargo xtask lint ratchet --print test-layout` for the same data.
    #[arg(long)]
    print_allowlist: bool,
}

/// Run the test-layout gate in enforce mode. The umbrella `cargo xtask lint`
/// entry point uses this.
pub(crate) fn enforce() -> Result<()> {
    run(LintTestsArgs {
        output: FormatArgs::default(),
        print_allowlist: false,
    })
}

pub(crate) fn run(args: LintTestsArgs) -> Result<()> {
    let format = args.output.resolved();
    report::run_gate(
        format,
        "test-layout",
        "crates/",
        "move tests into a single sibling tests.rs and update the ratchet row after shrink",
        "cargo xtask lint tests",
        || run_inner(args),
    )
}

fn run_inner(args: LintTestsArgs) -> Result<()> {
    if args.print_allowlist {
        return ratchet::print_families(TEST_LAYOUT_FAMILIES);
    }
    // Scoped ratchet enforce; OK line uses the engine's family-scoped message.
    let outcome = ratchet::check_families_at_root(TEST_LAYOUT_FAMILIES)?;
    if outcome.problems.is_empty() {
        let root = crate::docs::repo_root()?;
        let violations = measure_violations(&root)?;
        if violations.is_empty() {
            emit("test-layout gate OK — 0 violations; no grandfathered entries required");
        } else {
            emit(&format!(
                "test-layout gate OK — {} measured violation(s) match the ratchet baseline",
                violations.len(),
            ));
        }
        return Ok(());
    }
    let mut problems: Vec<&str> = outcome
        .problems
        .iter()
        .map(|p| p.message.as_str())
        .collect();
    problems.sort_unstable();
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
    for entry in crate::fs_util::read_dir_sorted(&crates_dir)? {
        let src = entry.path().join("src");
        if src.is_dir() {
            walk(&src, root, &mut out)?;
        }
    }
    Ok(out)
}

fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
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
                non_tests_rs_violation_at(&rel, &text)
            };
            if let Some(reason) = reason {
                out.insert(rel, reason);
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
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
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

const DIRECT_TEST_REASON: &str =
    "direct test function attribute in non-`tests.rs` file — move the test to a sibling `tests.rs`";
const TEST_MODULE_REASON: &str = "test suite module must be exactly `#[cfg(test)]` followed by `mod tests;` — move tests to the canonical sibling file and remove visibility, aliases, `#[path]`, and feature-qualified suite attributes";
const TESTS_CHILD_REASON: &str =
    "`tests.rs` must not declare child modules — keep every test inline in this one file";
const PARSE_REASON: &str = "Rust source could not be parsed during the test-layout audit — fix the syntax so the gate can inspect it";

/// Fixture-only modules are the one exception to the test-suite name/layout
/// rule. They contain no `#[test]` functions and are consumed by two or more
/// sibling suites or downstream feature-enabled tests; see `crates/AGENTS.md`.
const TEST_SUPPORT_MODULE: &str = "test_support";
const APPROVED_TEST_SUPPORT_PARENTS: &[&str] = &[
    "crates/jackin-config/src/lib.rs",
    "crates/jackin-console/src/tui/input.rs",
    "crates/jackin-env/src/lib.rs",
    "crates/jackin-launch-tui/src/lib.rs",
    "crates/jackin-runtime/src/runtime.rs",
    "crates/jackin/src/console/tui.rs",
];

fn non_tests_rs_violation_at(path: &str, text: &str) -> Option<String> {
    let Ok(file) = syn::parse_file(text) else {
        return Some(PARSE_REASON.to_owned());
    };
    inspect_non_test_items(&file.items, path, text)
}

#[cfg(test)]
fn non_tests_rs_violation(text: &str) -> Option<String> {
    non_tests_rs_violation_at("crates/example/src/lib.rs", text)
}

fn inspect_non_test_items(items: &[Item], path: &str, text: &str) -> Option<String> {
    for item in items {
        match item {
            Item::Fn(function) if has_test_attribute(&function.attrs) => {
                return Some(DIRECT_TEST_REASON.to_owned());
            }
            Item::Mod(module) => {
                if let Some(reason) = test_module_violation(module, path, text) {
                    return Some(reason.to_owned());
                }
                if let Some((_, nested)) = &module.content
                    && let Some(reason) = inspect_non_test_items(nested, path, text)
                {
                    return Some(reason);
                }
            }
            _ => {}
        }
    }
    None
}

fn test_module_violation(module: &ItemMod, path: &str, text: &str) -> Option<&'static str> {
    let name = module.ident.to_string();
    let is_support = name == TEST_SUPPORT_MODULE;
    let test_gated = module.attrs.iter().any(attribute_mentions_test_cfg);
    let suite_named = name == "tests" || name.ends_with("_tests");
    if is_support {
        return (module.content.is_some() || !APPROVED_TEST_SUPPORT_PARENTS.contains(&path))
            .then_some(
                "shared `test_support` must be an external module declared by an audited parent in the fixture registry",
            );
    }
    if !(test_gated || suite_named) {
        return None;
    }
    (!is_exact_canonical_suite(module, text)).then_some(TEST_MODULE_REASON)
}

fn has_test_attribute(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| is_test_path(attr.path()) || cfg_attr_adds_test(attr))
}

fn is_test_path(path: &syn::Path) -> bool {
    path.segments.last().is_some_and(|segment| {
        matches!(
            segment.ident.to_string().as_str(),
            "test" | "rstest" | "test_case"
        )
    })
}

fn cfg_attr_adds_test(attr: &Attribute) -> bool {
    if !attr.path().is_ident("cfg_attr") {
        return false;
    }
    cfg_attr_meta_adds_test(&attr.meta)
}

fn is_test_meta(meta: &Meta) -> bool {
    is_test_path(meta.path()) || cfg_attr_meta_adds_test(meta)
}

fn cfg_attr_meta_adds_test(meta: &Meta) -> bool {
    if !meta.path().is_ident("cfg_attr") {
        return false;
    }
    let Meta::List(list) = meta else {
        return false;
    };
    let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
    let Ok(metas) = parser.parse2(list.tokens.clone()) else {
        return false;
    };
    metas.iter().skip(1).any(is_test_meta)
}

fn attribute_mentions_test_cfg(attr: &Attribute) -> bool {
    (attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
        && meta_tokens_contain_test(&attr.meta)
}

fn meta_tokens_contain_test(meta: &Meta) -> bool {
    let Meta::List(list) = meta else {
        return false;
    };
    tokens_contain_test(list.tokens.clone())
}

fn tokens_contain_test(tokens: proc_macro2::TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(ident) => ident == "test",
        proc_macro2::TokenTree::Group(group) => tokens_contain_test(group.stream()),
        _ => false,
    })
}

fn is_exact_canonical_suite(module: &ItemMod, text: &str) -> bool {
    if module.ident != "tests"
        || !matches!(module.vis, Visibility::Inherited)
        || module.content.is_some()
        || module.semi.is_none()
        || module.attrs.len() != 1
        || !is_exact_cfg_test(&module.attrs[0])
    {
        return false;
    }
    let start = module.attrs[0].span().start();
    let end = module
        .semi
        .as_ref()
        .map_or_else(|| module.span().end(), |semi| semi.span().end());
    let Some(source) = source_between(text, start, end) else {
        return false;
    };
    let mut lines = source.lines();
    lines.next() == Some("#[cfg(test)]")
        && lines
            .next()
            .is_some_and(|line| line.trim_start() == "mod tests;")
        && lines.next().is_none()
}

fn is_exact_cfg_test(attr: &Attribute) -> bool {
    attr.path().is_ident("cfg")
        && matches!(&attr.meta, Meta::List(list) if list.tokens.to_string() == "test")
}

fn source_between(text: &str, start: LineColumn, end: LineColumn) -> Option<&str> {
    let line_starts = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(index, _)| index + 1))
        .collect::<Vec<_>>();
    let start_offset = *line_starts.get(start.line.checked_sub(1)?)? + start.column;
    let end_offset = *line_starts.get(end.line.checked_sub(1)?)? + end.column;
    text.get(start_offset..end_offset)
}

fn tests_rs_violation(text: &str) -> Option<String> {
    let Ok(file) = syn::parse_file(text) else {
        return Some(PARSE_REASON.to_owned());
    };
    contains_module(&file.items).then(|| TESTS_CHILD_REASON.to_owned())
}

fn contains_module(items: &[Item]) -> bool {
    struct ModuleFinder(bool);

    impl<'ast> syn::visit::Visit<'ast> for ModuleFinder {
        fn visit_item_mod(&mut self, _module: &'ast ItemMod) {
            self.0 = true;
        }
    }

    let mut finder = ModuleFinder(false);
    for item in items {
        syn::visit::Visit::visit_item(&mut finder, item);
    }
    finder.0
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
    problems.sort_unstable();

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
    if report::human_output() {
        println!("{message}");
    }
}

#[cfg(test)]
mod tests;
