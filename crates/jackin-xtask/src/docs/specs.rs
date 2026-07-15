//! Spec↔test linkage gate: `cargo xtask docs specs`.
//!
//! Every INV row in `docs/content/docs/reference/developer-reference/specs/*.mdx`
//! must carry a `Tests` cell. Cited test paths use the greppable form
//! `crate::module::tests::fn_name`. Broken citations fail; `MISSING` cells fail
//! (no warning-only escape hatch — add a real test or drop the row).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use syn::parse::Parser as _;
use syn::punctuated::Punctuated;
use syn::{Attribute, Item, Meta, Token};

const SPECS_REL: &str = "docs/content/docs/reference/developer-reference/specs";

pub(super) fn check_specs(root: &Path) -> Result<()> {
    let specs_dir = root.join(SPECS_REL);
    if !specs_dir.is_dir() {
        bail!("specs directory missing: {SPECS_REL}");
    }

    let mut inv_rows = 0usize;
    let mut cited_ok = 0usize;
    let mut problems = Vec::new();
    let mut crates_for_reconcile: BTreeSet<String> = BTreeSet::new();
    let mut citations: Vec<(String, String, String)> = Vec::new();

    for entry in
        fs::read_dir(&specs_dir).with_context(|| format!("reading {}", specs_dir.display()))?
    {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "mdx") {
            continue;
        }
        if path.file_name().is_some_and(|n| n == "index.mdx") {
            continue;
        }
        let rel = relative(root, &path);
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let rows = parse_inv_rows(&text);
        if rows.is_empty() {
            continue;
        }
        for row in rows {
            inv_rows += 1;
            let Some(tests_cell) = row.tests.as_deref() else {
                problems.push(format!(
                    "{rel}: {} lacks a Tests cell (add a fourth column)",
                    row.inv
                ));
                continue;
            };
            let cell = tests_cell.trim();
            if cell.is_empty() {
                problems.push(format!("{rel}: {} has an empty Tests cell", row.inv));
                continue;
            }
            if cell == "MISSING" || cell == "`MISSING`" {
                problems.push(format!(
                    "{rel}: {} Tests = MISSING (add a real cited test; MISSING fails the gate)",
                    row.inv
                ));
                continue;
            }
            for citation in split_citations(cell) {
                match verify_citation(root, &citation) {
                    Ok(crate_name) => {
                        cited_ok += 1;
                        crates_for_reconcile.insert(crate_name);
                        citations.push((rel.clone(), row.inv.clone(), citation));
                    }
                    Err(reason) => problems.push(format!(
                        "{rel}: {} citation `{citation}` — {reason}",
                        row.inv
                    )),
                }
            }
        }
    }

    if problems.is_empty()
        && !citations.is_empty()
        && let Err(msg) = reconcile_with_nextest(root, &crates_for_reconcile, &citations)
    {
        problems.push(msg);
    }

    if !problems.is_empty() {
        bail!(
            "{} spec citation problem(s):\n  {}",
            problems.len(),
            problems.join("\n  ")
        );
    }

    let reconciled =
        std::env::var_os("JACKIN_SPECS_RECONCILE").is_some() || std::env::var_os("CI").is_some();
    emit(&format!(
        "spec gate OK — {inv_rows} INV rows, {cited_ok} cited tests verified (syn{})",
        if reconciled {
            " + nextest reconcile"
        } else {
            "; set JACKIN_SPECS_RECONCILE=1 for runner list"
        }
    ));
    Ok(())
}

#[derive(Debug)]
struct InvRow {
    inv: String,
    tests: Option<String>,
}

/// Parse markdown table rows that start with `| INV-`.
fn parse_inv_rows(text: &str) -> Vec<InvRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.is_empty() {
            continue;
        }
        let inv = cells[0];
        if !inv.starts_with("INV-") {
            continue;
        }
        let tests = if cells.len() >= 4 {
            Some(cells[3].to_owned())
        } else {
            None
        };
        rows.push(InvRow {
            inv: inv.to_owned(),
            tests,
        });
    }
    rows
}

fn split_citations(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = cell;
    while let Some(start) = rest.find('`') {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('`') {
            let token = rest[..end].trim();
            if !token.is_empty() && token != "MISSING" {
                out.push(token.to_owned());
            }
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    if out.is_empty() {
        let bare = cell.trim().trim_matches('`');
        if !bare.is_empty() && bare != "MISSING" {
            out.push(bare.to_owned());
        }
    }
    out
}

/// Map `crate_name::module::path::tests::fn_name` to a source file, require a
/// recognized test attribute via `syn`, and return the crate name for runner
/// reconciliation.
fn verify_citation(root: &Path, citation: &str) -> Result<String, String> {
    let parts: Vec<&str> = citation.split("::").collect();
    if parts.len() < 3 {
        return Err("expected form crate::module::tests::fn_name".into());
    }
    let crate_name = parts[0];
    let fn_name = parts[parts.len() - 1];
    if parts[parts.len() - 2] != "tests" {
        return Err("citation must end with ::tests::fn_name".into());
    }
    let module_parts = &parts[1..parts.len() - 2];

    let crate_dir = crate_name.replace('_', "-");
    let mut candidates: Vec<PathBuf> = Vec::new();

    if module_parts.is_empty() {
        candidates.push(
            root.join("crates")
                .join(&crate_dir)
                .join("src")
                .join("tests.rs"),
        );
    } else {
        let mut path = root.join("crates").join(&crate_dir).join("src");
        for seg in module_parts {
            path.push(seg);
        }
        path.push("tests.rs");
        candidates.push(path);
    }

    for path in &candidates {
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        return match find_test_fn(&text, fn_name) {
            TestFnStatus::Ok => Ok(crate_name.to_owned()),
            TestFnStatus::HelperOnly => Err(format!(
                "fn `{fn_name}` exists in {} but lacks a test attribute (#[test]/#[tokio::test]/#[rstest]) — helpers are not coverage",
                relative(root, path)
            )),
            TestFnStatus::Missing => Err(format!(
                "fn `{fn_name}` not found in {}",
                relative(root, path)
            )),
            TestFnStatus::ParseError(msg) => Err(format!(
                "failed to parse {} as Rust: {msg}",
                relative(root, path)
            )),
        };
    }

    Err(format!(
        "no tests.rs at expected path for crate `{crate_name}` module `{}`",
        module_parts.join("::")
    ))
}

#[derive(Debug)]
enum TestFnStatus {
    Ok,
    HelperOnly,
    Missing,
    ParseError(String),
}

fn find_test_fn(text: &str, fn_name: &str) -> TestFnStatus {
    let file = match syn::parse_file(text) {
        Ok(f) => f,
        Err(e) => return TestFnStatus::ParseError(e.to_string()),
    };
    let mut found_helper = false;
    for item in file.items {
        match inspect_item(&item, fn_name) {
            Some(true) => return TestFnStatus::Ok,
            Some(false) => found_helper = true,
            None => {}
        }
    }
    if found_helper {
        TestFnStatus::HelperOnly
    } else {
        TestFnStatus::Missing
    }
}

fn inspect_item(item: &Item, fn_name: &str) -> Option<bool> {
    match item {
        Item::Fn(func) if func.sig.ident == fn_name => Some(has_test_attribute(&func.attrs)),
        Item::Mod(module) => {
            if let Some((_, items)) = &module.content {
                let mut found_helper = false;
                for nested in items {
                    match inspect_item(nested, fn_name) {
                        Some(true) => return Some(true),
                        Some(false) => found_helper = true,
                        None => {}
                    }
                }
                if found_helper {
                    return Some(false);
                }
            }
            None
        }
        _ => None,
    }
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
    let Meta::List(list) = &attr.meta else {
        return false;
    };
    let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
    let Ok(metas) = parser.parse2(list.tokens.clone()) else {
        return false;
    };
    metas.iter().skip(1).any(|meta| is_test_path(meta.path()))
}

/// Reconcile cited test function names against `cargo nextest list` for the
/// packages that appear in citations.
///
/// Opt-in: package listing is ~30–60s, so the default gate is syn-only. Enable
/// with `JACKIN_SPECS_RECONCILE=1` or when `CI=true` (docs CI sets `CI`).
fn reconcile_with_nextest(
    root: &Path,
    crates: &BTreeSet<String>,
    citations: &[(String, String, String)],
) -> Result<(), String> {
    let reconcile =
        std::env::var_os("JACKIN_SPECS_RECONCILE").is_some() || std::env::var_os("CI").is_some();
    if !reconcile {
        return Ok(());
    }
    static CACHE: OnceLock<Result<BTreeMap<String, BTreeSet<String>>, String>> = OnceLock::new();
    let by_crate = CACHE.get_or_init(|| load_nextest_tests(root, crates));
    let by_crate = match by_crate {
        Ok(m) => m,
        Err(e) if e.contains("not installed") || e.contains("No such file") => {
            emit(&format!("warning: nextest reconciliation skipped ({e})"));
            return Ok(());
        }
        Err(e) => return Err(format!("nextest reconciliation failed: {e}")),
    };

    let mut missing = Vec::new();
    for (rel, inv, citation) in citations {
        let parts: Vec<&str> = citation.split("::").collect();
        let crate_name = parts[0];
        let fn_name = parts[parts.len() - 1];
        let Some(names) = by_crate.get(crate_name) else {
            missing.push(format!(
                "{rel}: {inv} citation `{citation}` — crate not present in nextest list"
            ));
            continue;
        };
        // nextest binary IDs look like `jackin_console::tui::state::manager::tests::fn`
        // or `jackin_console::path::tests::fn` — match on trailing `::fn_name`.
        let suffix = format!("::{fn_name}");
        let hit = names
            .iter()
            .any(|id| id.ends_with(&suffix) || id == fn_name);
        if !hit {
            missing.push(format!(
                "{rel}: {inv} citation `{citation}` — test not listed by `cargo nextest list` for package `{crate_name}`"
            ));
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing.join("\n  "))
    }
}

fn load_nextest_tests(
    root: &Path,
    crates: &BTreeSet<String>,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for crate_name in crates {
        let pkg = crate_name.replace('_', "-");
        let mut cmd = crate::cmd::command("cargo");
        cmd.current_dir(root)
            .args(["nextest", "list", "-p", &pkg, "--message-format", "json"]);
        let output = crate::cmd::output_raw(&mut cmd)
            .map_err(|e| format!("spawn cargo nextest list -p {pkg}: {e}"))?;
        if !output.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("no such command") || stderr.contains("is not installed") {
                return Err("cargo-nextest not installed".into());
            }
            return Err(format!(
                "cargo nextest list -p {pkg} failed: {}",
                stderr.chars().take(400).collect::<String>()
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut names = BTreeSet::new();
        // nextest JSON lines or single object — accept either.
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                collect_test_ids(&v, &mut names);
            }
        }
        if names.is_empty()
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&stdout)
        {
            collect_test_ids(&v, &mut names);
        }
        out.insert(crate_name.clone(), names);
    }
    Ok(out)
}

fn collect_test_ids(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                out.insert(name.to_owned());
            }
            if let Some(id) = map.get("id").and_then(|v| v.as_str()) {
                out.insert(id.to_owned());
            }
            // nextest list JSON carries case names under `testcases` / `tests`
            // maps or arrays; also recurse all values so suite nesting is covered.
            for (k, v) in map {
                if matches!(k.as_str(), "testcases" | "tests") {
                    collect_testcase_entries(v, out);
                }
                collect_test_ids(v, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                collect_test_ids(item, out);
            }
        }
        serde_json::Value::String(s) if s.contains("::") => {
            out.insert(s.clone());
        }
        _ => {}
    }
}

fn collect_testcase_entries(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Array(arr) => {
            for item in arr {
                collect_test_ids(item, out);
            }
        }
        serde_json::Value::Object(obj) => {
            for name in obj.keys() {
                out.insert(name.clone());
            }
        }
        _ => {}
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        |p| p.to_string_lossy().replace('\\', "/"),
    )
}

fn emit(line: &str) {
    #[expect(
        clippy::print_stdout,
        reason = "jackin-xtask is a CLI; the spec report is its output"
    )]
    {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests;
