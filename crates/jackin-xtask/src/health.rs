//! Report-only code-health dashboard (codebase-health-enforcement Phase 0).
//!
//! ```sh
//! cargo xtask health                  # human report
//! cargo xtask health --format json    # machine-readable
//! cargo xtask health --write-baseline # refresh code-health-baseline.toml
//! ```
//!
//! Report-only by design — not wired into `cargo xtask lint` or CI as a
//! failing gate. Phase 7 decides which metrics become budgets.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use serde::Serialize;

use crate::docs::repo_root;

const BASELINE_PATH: &str = "code-health-baseline.toml";
const CRATES_GLOB: &str = "crates";
const LARGE_MODULE_LINES: usize = 300;
const TOP_PRODUCTION: usize = 15;
const TOP_TESTS: usize = 10;

#[derive(Args, Debug)]
pub(crate) struct HealthArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
    /// Write aggregate floors to `code-health-baseline.toml` at the repo root.
    #[arg(long)]
    write_baseline: bool,
    /// Minimum crate count for a helper name to count as a duplicate family.
    #[arg(long, default_value_t = 3)]
    min_crates: usize,
}

#[derive(Clone, Copy, Debug, ValueEnum, Default)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the health report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

#[derive(Debug, Serialize)]
struct FileLines {
    path: String,
    lines: usize,
}

#[derive(Debug, Serialize)]
struct SuppressionSummary {
    allow_attrs: usize,
    expect_attrs: usize,
    bare_allow_attrs: usize,
    bare_expect_attrs: usize,
    by_lint: BTreeMap<String, usize>,
    by_crate: BTreeMap<String, CrateSuppressions>,
    bare_by_crate: BTreeMap<String, usize>,
}

#[derive(Debug, Default, Serialize)]
struct CrateSuppressions {
    allow: usize,
    expect: usize,
    bare_allow: usize,
    bare_expect: usize,
}

#[derive(Debug, Serialize)]
struct PubSurface {
    pub_items: usize,
    pub_mods: usize,
}

#[derive(Debug, Serialize)]
struct DocBytes {
    path: String,
    bytes: usize,
    token_approx: usize,
}

#[derive(Debug, Serialize)]
struct DuplicateHelper {
    name: String,
    crates: Vec<String>,
    locations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AdvisoryNote {
    bare_allow_ratio: f64,
    bare_allow_attrs: usize,
    allow_attrs: usize,
    note: String,
}

#[derive(Debug, Serialize)]
struct Report {
    largest_production_files: Vec<FileLines>,
    largest_test_files: Vec<FileLines>,
    untested_large_modules: Vec<FileLines>,
    suppressions: SuppressionSummary,
    pub_surface: BTreeMap<String, PubSurface>,
    agent_docs: Vec<DocBytes>,
    duplicate_helpers: Vec<DuplicateHelper>,
    advisory: AdvisoryNote,
    verification_map: BTreeMap<String, String>,
}

pub(crate) fn run(args: HealthArgs) -> Result<()> {
    let root = repo_root()?;
    let report = collect(&root, args.min_crates)?;

    if args.write_baseline {
        write_baseline(&root, &report)?;
        emit(&format!("wrote {}", root.join(BASELINE_PATH).display()));
    }

    match args.format {
        OutputFormat::Human => print_human(&report),
        OutputFormat::Json => {
            let json =
                serde_json::to_string_pretty(&report).context("serializing health report")?;
            emit(&json);
        }
    }
    Ok(())
}

fn collect(root: &Path, min_crates: usize) -> Result<Report> {
    let line_counts = measure_rs_files(root)?;
    let (largest_production_files, largest_test_files) = largest_files(&line_counts, root);
    let untested_large_modules = untested_large(root, &line_counts);
    let suppressions = scan_suppressions(root)?;
    let pub_surface = scan_pub_surface(root)?;
    let agent_docs = scan_agent_docs(root)?;
    let duplicate_helpers = find_duplicate_helpers(root, min_crates)?;
    let verification_map = build_verification_map(root)?;

    let allow_attrs = suppressions.allow_attrs;
    let bare_allow_attrs = suppressions.bare_allow_attrs;
    let bare_ratio = if allow_attrs == 0 {
        0.0
    } else {
        bare_allow_attrs as f64 / allow_attrs as f64
    };

    Ok(Report {
        largest_production_files,
        largest_test_files,
        untested_large_modules,
        suppressions,
        pub_surface,
        agent_docs,
        duplicate_helpers,
        advisory: AdvisoryNote {
            bare_allow_ratio: bare_ratio,
            bare_allow_attrs,
            allow_attrs,
            note: String::from(
                "Advisory tool depth (llvm-cov, miri, mutants) lands with plan 035 scheduled lanes; this section reports bare-vs-reasoned suppression ratio only.",
            ),
        },
        verification_map,
    })
}

fn measure_rs_files(root: &Path) -> Result<BTreeMap<PathBuf, usize>> {
    let crates_dir = root.join(CRATES_GLOB);
    if !crates_dir.is_dir() {
        bail!("`{CRATES_GLOB}/` not found under {}", root.display());
    }
    let mut out = BTreeMap::new();
    for path in walk_rs_paths(&crates_dir)? {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        out.insert(path, text.lines().count());
    }
    Ok(out)
}

pub(crate) fn walk_rs_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in
            fs::read_dir(&current).with_context(|| format!("reading {}", current.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn largest_files(
    counts: &BTreeMap<PathBuf, usize>,
    root: &Path,
) -> (Vec<FileLines>, Vec<FileLines>) {
    let mut production: Vec<FileLines> = Vec::new();
    let mut tests: Vec<FileLines> = Vec::new();
    for (path, &lines) in counts {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let entry = FileLines {
            path: rel(root, path),
            lines,
        };
        if name == "tests.rs" {
            tests.push(entry);
        } else {
            production.push(entry);
        }
    }
    production.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.path.cmp(&b.path)));
    tests.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.path.cmp(&b.path)));
    production.truncate(TOP_PRODUCTION);
    tests.truncate(TOP_TESTS);
    (production, tests)
}

fn untested_large(root: &Path, counts: &BTreeMap<PathBuf, usize>) -> Vec<FileLines> {
    let mut out = Vec::new();
    for (path, &lines) in counts {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name == "tests.rs" || lines <= LARGE_MODULE_LINES {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let has_sibling = path
            .parent()
            .is_some_and(|p| p.join(stem).join("tests.rs").is_file());
        if has_sibling {
            continue;
        }
        out.push(FileLines {
            path: rel(root, path),
            lines,
        });
    }
    out.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.path.cmp(&b.path)));
    out
}

/// Parse `#[allow(...)]` / `#[expect(...)]` (and inner `#!` forms) with a
/// syntax-aware `syn` walk. Returns `(is_allow, lint_names, has_reason)` per
/// attribute. Comma-containing reason strings never leak as fake lint names.
pub(crate) fn parse_suppression_attrs(source: &str) -> Vec<(bool, Vec<String>, bool)> {
    let file = match syn::parse_file(source) {
        Ok(file) => file,
        Err(err) => {
            // Hard error for real files is handled by the caller path that
            // names the path; in-test fixtures may be fragments — wrap as a
            // module so item-level attributes still parse.
            let wrapped = format!("mod __jackin_suppression_fragment {{\n{source}\n}}");
            match syn::parse_file(&wrapped) {
                Ok(file) => file,
                Err(_) => panic!("suppression parser: syn failed: {err}"),
            }
        }
    };
    let mut visitor = SuppressionVisitor { out: Vec::new() };
    syn::visit::Visit::visit_file(&mut visitor, &file);
    visitor.out
}

struct SuppressionVisitor {
    out: Vec<(bool, Vec<String>, bool)>,
}

impl<'ast> syn::visit::Visit<'ast> for SuppressionVisitor {
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        collect_suppression_attr(attr, &mut self.out);
        syn::visit::visit_attribute(self, attr);
    }
}

fn collect_suppression_attr(attr: &syn::Attribute, out: &mut Vec<(bool, Vec<String>, bool)>) {
    let path = attr.path();
    // Unwrap one level of cfg_attr(…, allow/expect(...)).
    if path.is_ident("cfg_attr") {
        if let syn::Meta::List(list) = &attr.meta {
            if let Ok(nested) = list.parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            ) {
                for meta in nested.iter().skip(1) {
                    collect_meta_suppression(meta, out);
                }
            }
        }
        return;
    }
    collect_meta_suppression(&attr.meta, out);
}

fn collect_meta_suppression(meta: &syn::Meta, out: &mut Vec<(bool, Vec<String>, bool)>) {
    let syn::Meta::List(list) = meta else {
        return;
    };
    let is_allow = list.path.is_ident("allow");
    let is_expect = list.path.is_ident("expect");
    if !is_allow && !is_expect {
        return;
    }
    let Ok(nested) =
        list.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
    else {
        return;
    };
    let mut lints = Vec::new();
    let mut has_reason = false;
    for item in nested {
        match item {
            syn::Meta::Path(path) => {
                let name = path_to_lint_name(&path);
                if !name.is_empty() {
                    lints.push(name);
                }
            }
            syn::Meta::NameValue(nv) if nv.path.is_ident("reason") => {
                has_reason = true;
            }
            syn::Meta::List(_inner) => {
                // Nested lists are not lint names.
            }
            _ => {}
        }
    }
    if !lints.is_empty() {
        out.push((is_allow, lints, has_reason));
    }
}

fn path_to_lint_name(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|seg| seg.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

pub(crate) fn crate_name_from_path(root: &Path, path: &Path) -> String {
    let rel_path = path.strip_prefix(root.join(CRATES_GLOB)).unwrap_or(path);
    rel_path.components().next().map_or_else(
        || String::from("unknown"),
        |c| c.as_os_str().to_string_lossy().into_owned(),
    )
}

fn record_suppression(
    is_allow: bool,
    has_reason: bool,
    lints: &[String],
    crate_name: &str,
    summary: &mut SuppressionSummary,
) {
    let entry = summary.by_crate.entry(crate_name.to_owned()).or_default();
    if is_allow {
        summary.allow_attrs += 1;
        entry.allow += 1;
        if !has_reason {
            summary.bare_allow_attrs += 1;
            entry.bare_allow += 1;
            *summary
                .bare_by_crate
                .entry(crate_name.to_owned())
                .or_default() += 1;
        }
    } else {
        summary.expect_attrs += 1;
        entry.expect += 1;
        if !has_reason {
            summary.bare_expect_attrs += 1;
            entry.bare_expect += 1;
        }
    }
    for lint in lints {
        *summary.by_lint.entry(lint.clone()).or_default() += 1;
    }
}

fn scan_suppressions(root: &Path) -> Result<SuppressionSummary> {
    let crates_dir = root.join(CRATES_GLOB);
    let mut summary = SuppressionSummary {
        allow_attrs: 0,
        expect_attrs: 0,
        bare_allow_attrs: 0,
        bare_expect_attrs: 0,
        by_lint: BTreeMap::new(),
        by_crate: BTreeMap::new(),
        bare_by_crate: BTreeMap::new(),
    };
    for path in walk_rs_paths(&crates_dir)? {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let crate_name = crate_name_from_path(root, &path);
        for (is_allow, lints, has_reason) in parse_suppression_attrs(&text) {
            record_suppression(is_allow, has_reason, &lints, &crate_name, &mut summary);
        }
    }
    Ok(summary)
}

fn count_pub_line(line: &str, surface: &mut PubSurface) {
    let trimmed = line.trim_start();
    let Some(after) = trimmed.strip_prefix("pub ") else {
        return;
    };
    let kind = after.split_whitespace().next().unwrap_or("");
    match kind {
        "fn" | "struct" | "enum" | "trait" | "type" | "const" | "mod" | "use" => {
            surface.pub_items += 1;
            if kind == "mod" {
                surface.pub_mods += 1;
            }
        }
        _ => {}
    }
}

fn scan_pub_surface(root: &Path) -> Result<BTreeMap<String, PubSurface>> {
    let crates_dir = root.join(CRATES_GLOB);
    let mut out: BTreeMap<String, PubSurface> = BTreeMap::new();
    for path in walk_rs_paths(&crates_dir)? {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let crate_name = crate_name_from_path(root, &path);
        let surface = out.entry(crate_name).or_insert(PubSurface {
            pub_items: 0,
            pub_mods: 0,
        });
        for line in text.lines() {
            count_pub_line(line, surface);
        }
    }
    Ok(out)
}

fn leading_doc_bytes(text: &str) -> usize {
    let mut bytes = 0usize;
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("//!") {
            bytes += line.len() + 1;
        } else if t.is_empty() {
            // blank lines inside the module-doc block still count as part of
            // the leading header only when we have already seen a `//!` line.
            if bytes == 0 {
                break;
            }
        } else {
            break;
        }
    }
    bytes
}

fn push_doc(path: PathBuf, root: &Path, seen: &mut BTreeSet<String>, out: &mut Vec<DocBytes>) {
    if !path.is_file() {
        return;
    }
    let key = rel(root, &path);
    if !seen.insert(key.clone()) {
        return;
    }
    let bytes = if path
        .file_name()
        .is_some_and(|n| n == "lib.rs" || n == "main.rs")
    {
        fs::read_to_string(&path).map_or(0, |text| leading_doc_bytes(&text))
    } else {
        fs::metadata(&path).map_or(0, |m| m.len() as usize)
    };
    out.push(DocBytes {
        path: key,
        bytes,
        token_approx: bytes / 4,
    });
}

fn scan_agent_docs(root: &Path) -> Result<Vec<DocBytes>> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    push_doc(root.join("AGENTS.md"), root, &mut seen, &mut out);
    push_doc(root.join("crates/AGENTS.md"), root, &mut seen, &mut out);

    let crates_dir = root.join(CRATES_GLOB);
    if !crates_dir.is_dir() {
        return Ok(out);
    }
    for entry in fs::read_dir(&crates_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir = entry.path();
        for name in ["AGENTS.md", "README.md"] {
            push_doc(dir.join(name), root, &mut seen, &mut out);
        }
        for name in ["lib.rs", "main.rs"] {
            push_doc(dir.join("src").join(name), root, &mut seen, &mut out);
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn normalize_fn_name(name: &str) -> String {
    let mut s = String::from(name);
    for prefix in ["try_", "with_"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = String::from(rest);
        }
    }
    if let Some(rest) = s.strip_suffix("_impl") {
        s = String::from(rest);
    }
    s
}

fn record_fn_defs(
    text: &str,
    crate_name: &str,
    loc: &str,
    map: &mut BTreeMap<String, BTreeMap<String, Vec<String>>>,
) {
    for line in text.lines() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("fn ") else {
            continue;
        };
        let fname = rest
            .split(|c: char| c == '(' || c.is_whitespace())
            .next()
            .unwrap_or("");
        if fname.is_empty() || fname.starts_with('_') {
            continue;
        }
        let norm = normalize_fn_name(fname);
        map.entry(norm)
            .or_default()
            .entry(crate_name.to_owned())
            .or_default()
            .push(format!("{loc}::{fname}"));
    }
}

fn find_duplicate_helpers(root: &Path, min_crates: usize) -> Result<Vec<DuplicateHelper>> {
    let mut map: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let crates_dir = root.join(CRATES_GLOB);
    for path in walk_rs_paths(&crates_dir)? {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "tests.rs" {
            continue;
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let crate_name = crate_name_from_path(root, &path);
        let loc = rel(root, &path);
        record_fn_defs(&text, &crate_name, &loc, &mut map);
    }

    let mut out = Vec::new();
    for (name, crates) in map {
        if crates.len() < min_crates {
            continue;
        }
        let mut crate_list: Vec<String> = crates.keys().cloned().collect();
        crate_list.sort();
        let mut locations: Vec<String> = crates.into_values().flatten().collect();
        locations.sort();
        out.push(DuplicateHelper {
            name,
            crates: crate_list,
            locations,
        });
    }
    out.sort_by(|a, b| {
        b.crates
            .len()
            .cmp(&a.crates.len())
            .then_with(|| a.name.cmp(&b.name))
    });
    out.truncate(50);
    Ok(out)
}

fn build_verification_map(root: &Path) -> Result<BTreeMap<String, String>> {
    #[derive(serde::Deserialize)]
    struct Metadata {
        packages: Vec<Package>,
        workspace_members: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct Package {
        name: String,
        id: String,
    }

    let mut meta_cmd = Command::new("cargo");
    meta_cmd
        .args(["metadata", "--format-version=1", "--no-deps"])
        .current_dir(root);
    let output = crate::cmd::output_raw(&mut meta_cmd).context("running cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let meta: Metadata =
        serde_json::from_slice(&output.stdout).context("parsing cargo metadata")?;
    let member_ids: BTreeSet<&str> = meta.workspace_members.iter().map(String::as_str).collect();
    let mut map = BTreeMap::new();
    for pkg in meta.packages {
        if !member_ids.contains(pkg.id.as_str()) {
            continue;
        }
        let cmd = if pkg.name == "jackin" {
            String::from("cargo nextest run -p jackin (E2E: --features e2e --profile docker-e2e)")
        } else {
            format!("cargo nextest run -p {}", pkg.name)
        };
        map.insert(pkg.name, cmd);
    }
    Ok(map)
}

fn print_human(report: &Report) {
    emit("# Code-health dashboard (Phase 0)");
    emit("");
    emit("## Largest production files (Phase 2/4 sizing)");
    for f in &report.largest_production_files {
        emit(&format!("  {:>5}  {}", f.lines, f.path));
    }
    emit("");
    emit("## Largest tests.rs files (Phase 3 sizing)");
    for f in &report.largest_test_files {
        emit(&format!("  {:>5}  {}", f.lines, f.path));
    }
    emit("");
    emit(&format!(
        "## Untested large modules >{LARGE_MODULE_LINES} lines (Phase 3 coverage-map report)"
    ));
    emit(&format!("  count: {}", report.untested_large_modules.len()));
    for f in report.untested_large_modules.iter().take(25) {
        emit(&format!("  {:>5}  {}", f.lines, f.path));
    }
    if report.untested_large_modules.len() > 25 {
        emit(&format!(
            "  … {} more",
            report.untested_large_modules.len() - 25
        ));
    }
    emit("");
    emit("## Suppressions (Phase 1 ratchet input)");
    let s = &report.suppressions;
    emit(&format!(
        "  allow_attrs={} expect_attrs={} bare_allow={} bare_expect={}",
        s.allow_attrs, s.expect_attrs, s.bare_allow_attrs, s.bare_expect_attrs
    ));
    emit("  top bare-allow crates:");
    let mut bare: Vec<_> = s.bare_by_crate.iter().collect();
    bare.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    for (crate_name, count) in bare.into_iter().take(10) {
        emit(&format!("    {crate_name}: {count}"));
    }
    emit("");
    emit("## Public surface proxy (Phase 2 pub-surface report)");
    let mut pubs: Vec<_> = report.pub_surface.iter().collect();
    pubs.sort_by(|a, b| b.1.pub_items.cmp(&a.1.pub_items).then_with(|| a.0.cmp(b.0)));
    for (name, surface) in pubs.into_iter().take(12) {
        emit(&format!(
            "  {name}: pub_items={} pub_mods={}",
            surface.pub_items, surface.pub_mods
        ));
    }
    emit("");
    emit("## Agent-doc bytes (Phase 6/7 context-economy budgets)");
    let mut total = 0usize;
    for d in &report.agent_docs {
        total += d.bytes;
        emit(&format!(
            "  {:>7} B (~{} tok)  {}",
            d.bytes, d.token_approx, d.path
        ));
    }
    emit(&format!("  total_bytes={total}"));
    emit("");
    emit("## Duplicate helper families (Phase 0 dashboard)");
    emit(&format!(
        "  families_reported={}",
        report.duplicate_helpers.len()
    ));
    for h in report.duplicate_helpers.iter().take(15) {
        emit(&format!(
            "  {} ({} crates): {}",
            h.name,
            h.crates.len(),
            h.crates.join(", ")
        ));
    }
    emit("");
    emit("## Advisory (Phase 1 scheduled lanes feed)");
    emit(&format!(
        "  bare_allow_ratio={:.3} ({}/{})",
        report.advisory.bare_allow_ratio,
        report.advisory.bare_allow_attrs,
        report.advisory.allow_attrs
    ));
    emit(&format!("  note: {}", report.advisory.note));
    emit("");
    emit("## Verification map (Phase 6 narrowest-command)");
    emit(&format!(
        "  workspace_members={}",
        report.verification_map.len()
    ));
}

fn toml_key(path: &str) -> String {
    path.replace(['/', '.'], "_")
}

fn write_baseline(root: &Path, report: &Report) -> Result<()> {
    let mut out = String::new();
    out.push_str(
        "# Generated by cargo xtask health --write-baseline. Phase 0 baseline; Phase 7's ratchet engine consumes these floors.\n",
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    out.push_str(&format!("# Generated at unix_time={now}\n"));
    out.push_str(
        "# suite-wall-time: seeded by plan 013 from CI junit artifacts, not locally computable\n\n",
    );

    out.push_str("[suppressions]\n");
    out.push_str(&format!(
        "allow_attrs = {}\n",
        report.suppressions.allow_attrs
    ));
    out.push_str(&format!(
        "expect_attrs = {}\n",
        report.suppressions.expect_attrs
    ));
    out.push_str(&format!(
        "bare_allow_attrs = {}\n",
        report.suppressions.bare_allow_attrs
    ));
    out.push_str(&format!(
        "bare_expect_attrs = {}\n\n",
        report.suppressions.bare_expect_attrs
    ));

    out.push_str("[suppressions.bare_by_crate]\n");
    for (k, v) in &report.suppressions.bare_by_crate {
        out.push_str(&format!("{k} = {v}\n"));
    }
    out.push('\n');

    out.push_str("[suppressions.by_lint]\n");
    for (k, v) in &report.suppressions.by_lint {
        let key = k.replace(':', "_");
        out.push_str(&format!("\"{key}\" = {v}\n"));
    }
    out.push('\n');

    out.push_str("[pub_surface]\n");
    for (k, v) in &report.pub_surface {
        out.push_str(&format!("{k} = {}\n", v.pub_items));
    }
    out.push('\n');

    out.push_str("[agent_docs]\n");
    for d in &report.agent_docs {
        let key = toml_key(&d.path);
        out.push_str(&format!("\"{key}\" = {}\n", d.bytes));
    }
    out.push('\n');

    out.push_str("[largest_production]\n");
    for f in &report.largest_production_files {
        let key = toml_key(&f.path);
        out.push_str(&format!("\"{key}\" = {}\n", f.lines));
    }

    fs::write(root.join(BASELINE_PATH), out).with_context(|| format!("writing {BASELINE_PATH}"))?;
    Ok(())
}

#[cfg(test)]
mod tests;
