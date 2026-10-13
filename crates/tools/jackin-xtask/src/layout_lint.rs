//! Test-file-layout gate.
//!
//! Enforces the workspace hard rule: every module's tests live in a sibling
//! `tests.rs`, declared exactly as `#[cfg(test)] mod tests;`. Large suites
//! split into canonical case files: `tests.rs` holds shared imports/fixtures
//! plus plain `mod <case>;` declarations resolving to `tests/<case>.rs`
//! (mirrors rust-repository-policy `GAP-TEST-002`). Forbidden in `crates/*/src`:
//!
//!   1. An inline `#[cfg(test)] mod <name> { … }` body in any non-`tests.rs`
//!      source file — the body must move to a sibling `tests.rs`.
//!   2. An external `#[cfg(test)]` module declaration other than the exact
//!      two-line `#[cfg(test)] mod tests;` form — Rust resolves the sibling
//!      `tests.rs` without `#[path]`.
//!   3. A direct unit-test function attribute in a non-`tests.rs` source file
//!      — the test must move to a sibling `tests.rs`.
//!   4. A `tests.rs` child module that is not a plain private `mod <case>;`
//!      resolving to `tests/<case>.rs` — no inline bodies, no `#[path]`,
//!      no visibility, no test-mentioning gates; no modules nested deeper.
//!   5. A `tests/<file>.rs` that is not declared by its sibling `tests.rs`,
//!      that declares sub-modules of its own, or that nests under a
//!      sub-directory — case files are flat and parent-declared.
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

use std::collections::{BTreeMap, BTreeSet};
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
        "{} test-layout violation(s):\n  {}\n\nMove tests into a sibling `tests.rs` with canonical case splits. To refresh the allowlist, run `cargo xtask lint tests --print-allowlist` (or `cargo xtask lint ratchet --print test-layout`).",
        problems.len(),
        problems.join("\n  ")
    )
}

/// Walk every `crates/<group>/<package>/src` tree and collect
/// `relative path → reason` for each file that breaks the test-layout rule.
pub(crate) fn measure_violations(root: &Path) -> Result<BTreeMap<String, String>> {
    let crates_dir = root.join("crates");
    if !crates_dir.is_dir() {
        bail!("`crates/` not found under {}", root.display());
    }
    let mut out = BTreeMap::new();
    for group in crate::fs_util::read_dir_sorted(&crates_dir)? {
        if !group.file_type()?.is_dir() {
            continue;
        }
        for entry in crate::fs_util::read_dir_sorted(&group.path())? {
            let src = entry.path().join("src");
            if src.is_dir() {
                walk(&src, root, &mut out)?;
            }
        }
    }
    Ok(out)
}

fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                // Canonical case files: flat, parent-declared, mod-free.
                check_tests_dir(&path, root, out)?;
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

/// Validate a `tests/` case directory against its sibling `tests.rs`:
/// every `.rs` file directly inside must be declared by the parent and
/// contain no module declarations; sub-directories are forbidden.
fn check_tests_dir(dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
    let declared = dir
        .parent()
        .and_then(|parent| fs::read_to_string(parent.join("tests.rs")).ok())
        .map(|text| declared_case_modules(&text))
        .unwrap_or_default();
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        let rel = rel_path(&path, root);
        if path.is_dir() {
            // Fixture/data directories (no `.rs` inside) are not test files.
            if dir_contains_rs(&path)? {
                out.insert(rel, TESTS_NESTING_REASON.to_owned());
            }
            continue;
        }
        if !path.extension().is_some_and(|ext| ext == "rs") {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !declared.contains(&stem) {
            out.insert(rel.clone(), TESTS_STRAY_REASON.to_owned());
            continue;
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        if case_file_violation(&text) {
            out.insert(rel, TESTS_CHILD_REASON.to_owned());
        }
    }
    Ok(())
}

/// Top-level `mod <name>;` declarations in a `tests.rs` (canonical or not;
/// malformed decls are reported against `tests.rs` itself by rule 4).
fn declared_case_modules(text: &str) -> BTreeSet<String> {
    let Ok(file) = syn::parse_file(text) else {
        return BTreeSet::new();
    };
    file.items
        .iter()
        .filter_map(|item| match item {
            Item::Mod(module) if module.content.is_none() => Some(module.ident.to_string()),
            _ => None,
        })
        .collect()
}

/// True when a directory tree holds any `.rs` file.
fn dir_contains_rs(dir: &Path) -> Result<bool> {
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        if path.is_dir() {
            if dir_contains_rs(&path)? {
                return Ok(true);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// True when a case file breaks the flat-and-mod-free rule.
fn case_file_violation(text: &str) -> bool {
    let Ok(file) = syn::parse_file(text) else {
        return true;
    };
    contains_module(&file.items)
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
const TESTS_CHILD_REASON: &str = "case file must be flat and mod-free — move nested modules into the parent `tests.rs` case list";
const TESTS_DECL_REASON: &str = "`tests.rs` child must be a plain private `mod <case>;` resolving to `tests/<case>.rs` — no inline bodies, `#[path]`, visibility, or test-mentioning gates";
const TESTS_STRAY_REASON: &str = "case file is not declared by its sibling `tests.rs` — add a plain `mod <case>;` or delete the file";
const TESTS_NESTING_REASON: &str =
    "case files must sit directly under `tests/` — nested directories are not canonical";
const TESTS_NESTED_REASON: &str = "`tests.rs` must not nest modules below the top level — declare each case once with `mod <case>;`";
const PARSE_REASON: &str = "Rust source could not be parsed during the test-layout audit — fix the syntax so the gate can inspect it";

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

fn test_module_violation(module: &ItemMod, _path: &str, text: &str) -> Option<&'static str> {
    let name = module.ident.to_string();
    let test_gated = module.attrs.iter().any(attribute_mentions_test_cfg);
    let suite_named = name == "tests" || name.ends_with("_tests");
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
    for item in &file.items {
        match item {
            Item::Mod(module) if !is_canonical_case_decl(module) => {
                return Some(TESTS_DECL_REASON.to_owned());
            }
            Item::Mod(_) => {}
            _ => {
                if contains_module(std::slice::from_ref(item)) {
                    return Some(TESTS_NESTED_REASON.to_owned());
                }
            }
        }
    }
    None
}

/// A canonical case declaration: private, external, no `#[path]`, no
/// test-mentioning gate (platform gates like `#[cfg(unix)]` are fine).
fn is_canonical_case_decl(module: &ItemMod) -> bool {
    if !matches!(module.vis, Visibility::Inherited)
        || module.content.is_some()
        || module.semi.is_none()
    {
        return false;
    }
    !module
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("path") || attribute_mentions_test_cfg(attr))
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
        "{} test-layout violation(s):\n  {}\n\nMove tests into a sibling `tests.rs` with canonical case splits. To refresh the allowlist, run `cargo xtask lint tests --print-allowlist`.",
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
