//! Spec↔test linkage gate: `cargo xtask docs specs`.
//!
//! Every INV row in `docs/content/docs/reference/developer-reference/specs/*.mdx`
//! must carry a `Tests` cell. Cited test paths use the greppable form
//! `crate::module::tests::fn_name` (or the literal `MISSING`). Broken
//! citations fail; `MISSING` cells warn.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

const SPECS_REL: &str = "docs/content/docs/reference/developer-reference/specs";

pub(super) fn check_specs(root: &Path) -> Result<()> {
    let specs_dir = root.join(SPECS_REL);
    if !specs_dir.is_dir() {
        bail!("specs directory missing: {SPECS_REL}");
    }

    let mut inv_rows = 0usize;
    let mut cited_ok = 0usize;
    let mut missing = 0usize;
    let mut problems = Vec::new();
    let mut warnings = Vec::new();

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
                missing += 1;
                warnings.push(format!("{rel}: {} Tests = MISSING", row.inv));
                continue;
            }
            for citation in split_citations(cell) {
                match verify_citation(root, &citation) {
                    Ok(()) => cited_ok += 1,
                    Err(reason) => problems.push(format!(
                        "{rel}: {} citation `{citation}` — {reason}",
                        row.inv
                    )),
                }
            }
        }
    }

    for w in &warnings {
        emit_warn(w);
    }

    if !problems.is_empty() {
        bail!(
            "{} spec citation problem(s):\n  {}",
            problems.len(),
            problems.join("\n  ")
        );
    }

    emit(&format!(
        "spec gate OK — {inv_rows} INV rows, {cited_ok} cited tests verified, {missing} MISSING"
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

/// Map `crate_name::module::path::tests::fn_name` to a source file and check
/// that `fn fn_name` exists.
fn verify_citation(root: &Path, citation: &str) -> Result<(), String> {
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
        if has_fn(&text, fn_name) {
            return Ok(());
        }
        return Err(format!(
            "fn `{fn_name}` not found in {}",
            relative(root, path)
        ));
    }

    Err(format!(
        "no tests.rs at expected path for crate `{crate_name}` module `{}`",
        module_parts.join("::")
    ))
}

fn has_fn(text: &str, fn_name: &str) -> bool {
    for line in text.lines() {
        let t = line.trim();
        for prefix in [
            "pub(crate) fn ",
            "pub fn ",
            "fn ",
            "async fn ",
            "pub async fn ",
            "pub(crate) async fn ",
        ] {
            if let Some(rest) = t.strip_prefix(prefix)
                && rest.starts_with(fn_name)
                && is_fn_name_boundary(&rest[fn_name.len()..])
            {
                return true;
            }
        }
    }
    false
}

fn is_fn_name_boundary(after: &str) -> bool {
    after.is_empty()
        || after.starts_with('(')
        || after.starts_with('<')
        || after.starts_with("::")
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

fn emit_warn(line: &str) {
    #[expect(
        clippy::print_stdout,
        reason = "jackin-xtask is a CLI; MISSING warnings are part of the gate report"
    )]
    {
        println!("warning: {line}");
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn parse_requires_tests_column() {
        let text = r#"
| INV | Description | Verify by |
|---|---|---|
| INV-1 | Trust first | `foo` |
"#;
        let rows = parse_inv_rows(text);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].tests.is_none());
    }

    #[test]
    fn parse_reads_tests_cell() {
        let text = r#"
| INV | Description | Verify by | Tests |
|---|---|---|---|
| INV-1 | Trust first | `foo` | `jackin_runtime::runtime::launch::tests::load_namespaced_agent_registers_source_and_trusts_on_accept` |
| INV-2 | Missing | `bar` | MISSING |
"#;
        let rows = parse_inv_rows(text);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].tests.as_ref().unwrap().contains("load_namespaced"));
        assert_eq!(rows[1].tests.as_deref(), Some("MISSING"));
    }

    #[test]
    fn verify_missing_fn_fails() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let tests = root.join("crates/jackin-runtime/src/runtime/launch/tests.rs");
        fs::create_dir_all(tests.parent().unwrap()).unwrap();
        fs::write(&tests, "fn real_test() {}\n").unwrap();
        let err = verify_citation(
            root,
            "jackin_runtime::runtime::launch::tests::not_a_real_test",
        )
        .unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn verify_existing_fn_ok() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let tests = root.join("crates/jackin-runtime/src/runtime/launch/tests.rs");
        fs::create_dir_all(tests.parent().unwrap()).unwrap();
        fs::write(&tests, "async fn real_test() {}\n").unwrap();
        verify_citation(root, "jackin_runtime::runtime::launch::tests::real_test").unwrap();
    }
}
