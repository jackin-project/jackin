use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;
use syn::visit::{self, Visit};

use crate::report::{self, FormatArgs};

const CONFIG_PATH: &str = "ratchet.toml";
const RERUN: &str = "cargo xtask lint ratchet";

pub(crate) const FILE_SIZE_FAMILIES: &[&str] = &["file-size-production", "file-size-test"];
pub(crate) const TEST_LAYOUT_FAMILIES: &[&str] = &["test-layout"];
pub(crate) const SUPPRESSION_FAMILIES: &[&str] = &["bare-allow-per-crate", "expect-per-lint-crate"];

#[derive(Debug, Args)]
pub(crate) struct LintRatchetArgs {
    #[command(flatten)]
    output: FormatArgs,
    #[arg(long)]
    print: Option<String>,
    #[arg(long = "only")]
    only: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Config {
    family: Vec<Family>,
}

#[derive(Debug, Clone, Deserialize)]
struct Family {
    id: String,
    #[serde(default = "default_kind")]
    kind: String,
    provider: String,
    #[serde(default)]
    cap: Option<usize>,
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default)]
    entry: Vec<Entry>,
}

fn default_kind() -> String {
    "numeric".into()
}
fn default_mode() -> String {
    "enforce".into()
}

#[derive(Debug, Clone, Deserialize)]
struct Entry {
    key: String,
    #[serde(default)]
    bound: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NumericVerdict {
    Ok,
    StaleMissing,
    StaleUnderCap { measured: usize },
    Shrink { measured: usize, budgeted: usize },
    Growth { measured: usize, budgeted: usize },
    UnlistedOverCap { measured: usize, cap: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PresenceVerdict {
    Stale,
    New { reason: String },
}

#[must_use]
pub(crate) fn check_numeric_entry(
    measured: Option<usize>,
    budgeted: usize,
    cap: usize,
) -> NumericVerdict {
    match measured {
        None => NumericVerdict::StaleMissing,
        Some(m) if m <= cap => NumericVerdict::StaleUnderCap { measured: m },
        Some(m) if m < budgeted => NumericVerdict::Shrink {
            measured: m,
            budgeted,
        },
        Some(m) if m > budgeted => NumericVerdict::Growth {
            measured: m,
            budgeted,
        },
        Some(_) => NumericVerdict::Ok,
    }
}

#[must_use]
pub(crate) fn check_numeric_unlisted(measured: usize, cap: usize) -> NumericVerdict {
    if measured > cap {
        NumericVerdict::UnlistedOverCap { measured, cap }
    } else {
        NumericVerdict::Ok
    }
}

#[must_use]
pub(crate) fn check_presence(
    violations: &BTreeMap<String, String>,
    allowed: &BTreeSet<String>,
) -> Vec<(String, PresenceVerdict)> {
    let mut out = Vec::new();
    for key in allowed {
        if !violations.contains_key(key) {
            out.push((key.clone(), PresenceVerdict::Stale));
        }
    }
    for (key, reason) in violations {
        if !allowed.contains(key) {
            out.push((
                key.clone(),
                PresenceVerdict::New {
                    reason: reason.clone(),
                },
            ));
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FamilyProblem {
    pub family: String,
    pub key: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct FamilyCheckOutcome {
    pub problems: Vec<FamilyProblem>,
    pub report_lines: Vec<String>,
}

pub(crate) fn enforce() -> Result<()> {
    run(LintRatchetArgs {
        output: FormatArgs::default(),
        print: None,
        only: Vec::new(),
    })
}

pub(crate) fn print_families(ids: &[&str]) -> Result<()> {
    let root = crate::docs::repo_root()?;
    let config = read_config(&root.join(CONFIG_PATH))?;
    for id in ids {
        print_family(&root, &config, id)?;
    }
    Ok(())
}

pub(crate) fn family_cap(id: &str) -> Result<usize> {
    let root = crate::docs::repo_root()?;
    let config = read_config(&root.join(CONFIG_PATH))?;
    let family = config
        .family
        .iter()
        .find(|f| f.id == id)
        .with_context(|| format!("unknown family {id:?} in {CONFIG_PATH}"))?;
    Ok(family.cap.unwrap_or(0))
}

pub(crate) fn check_families_at_root(ids: &[&str]) -> Result<FamilyCheckOutcome> {
    let root = crate::docs::repo_root()?;
    let config = read_config(&root.join(CONFIG_PATH))?;
    check_families(&root, &config, Some(ids))
}

pub(crate) fn run(args: LintRatchetArgs) -> Result<()> {
    let format = args.output.resolved();
    report::run_gate(
        format,
        "ratchet",
        CONFIG_PATH,
        "remove growth or tighten the matching ratchet.toml entry after shrink",
        RERUN,
        || run_inner(args),
    )
}

fn run_inner(args: LintRatchetArgs) -> Result<()> {
    let root = crate::docs::repo_root()?;
    let config = read_config(&root.join(CONFIG_PATH))?;
    if let Some(family_id) = args.print.as_deref() {
        if !args.only.is_empty() {
            bail!("--print cannot be combined with --only");
        }
        return print_family(&root, &config, family_id);
    }

    let only: Vec<&str> = args.only.iter().map(String::as_str).collect();
    let selected = (!only.is_empty()).then_some(only.as_slice());
    let outcome = check_families(&root, &config, selected)?;
    emit_outcome(
        &outcome,
        selected.map_or(config.family.len(), <[&str]>::len),
    )
}

fn emit_outcome(outcome: &FamilyCheckOutcome, family_count: usize) -> Result<()> {
    if !outcome.report_lines.is_empty() {
        for line in &outcome.report_lines {
            emit(line);
        }
    }
    if outcome.problems.is_empty() {
        emit(&format!(
            "ratchet OK — {family_count} families (config {CONFIG_PATH})"
        ));
        return Ok(());
    }
    let mut problems: Vec<&str> = outcome
        .problems
        .iter()
        .map(|p| p.message.as_str())
        .collect();
    problems.sort_unstable();
    bail!(
        "{} ratchet violation(s):\n  {}\n\nFix the listed rows, then re-run `{RERUN}`.",
        problems.len(),
        problems.join("\n  ")
    )
}

fn check_families(
    root: &Path,
    config: &Config,
    only: Option<&[&str]>,
) -> Result<FamilyCheckOutcome> {
    let mut problems: Vec<FamilyProblem> = Vec::new();
    let mut report_lines: Vec<String> = Vec::new();

    for family in &config.family {
        if let Some(ids) = only
            && !ids.contains(&family.id.as_str())
        {
            continue;
        }
        match family.kind.as_str() {
            "numeric" => {
                let measured = invoke_provider(root, &family.provider)?;
                let artifact_ceiling = is_artifact_ceiling(family);
                if absent_scheduled_artifact(family, &measured) {
                    report_lines.push(format!(
                        "{} (scheduled enforce; skipped - artifact absent)",
                        family.id
                    ));
                    continue;
                }
                let cap = family.cap.unwrap_or(0);
                let budgeted: BTreeMap<&str, usize> = family
                    .entry
                    .iter()
                    .filter_map(|e| e.bound.map(|b| (e.key.as_str(), b)))
                    .collect();
                for (key, bound) in &budgeted {
                    let measured_opt = match measured.get(*key).copied() {
                        some @ Some(_) => some,
                        None if cap == 0 => Some(0),
                        None => None,
                    };
                    let v = check_numeric_entry(measured_opt, *bound, cap);
                    let ceiling_only = is_ceiling_only(family, artifact_ceiling);
                    let msg = match v {
                        NumericVerdict::Ok => None,
                        NumericVerdict::StaleMissing => Some(format!(
                            "{id}/{key}: budgeted but file missing — delete the stale budget row; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::StaleUnderCap { measured } if ceiling_only => {
                            report_lines.push(format!(
                                "{}/{key}: measured {measured} under ceiling {bound} (headroom OK)",
                                family.id
                            ));
                            None
                        }
                        NumericVerdict::StaleUnderCap { measured } => Some(format!(
                            "{id}/{key}: measured {measured} ≤ cap {cap} — no longer needs grandfathering; delete the budget row; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::Shrink { measured, budgeted } if ceiling_only => {
                            report_lines.push(format!(
                                "{}/{key}: measured {measured} under ceiling {budgeted} (headroom OK)",
                                family.id
                            ));
                            None
                        }
                        NumericVerdict::Shrink { measured, budgeted } => Some(format!(
                            "{id}/{key}: measured {measured} < budgeted {budgeted} — shrink the budget row to {measured}; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::Growth { measured, budgeted } => Some(format!(
                            "{id}/{key}: grew from {budgeted} to {measured} — refactor or intentionally raise only via budget update; regenerate: {RERUN} --print {id}",
                            id = family.id
                        )),
                        NumericVerdict::UnlistedOverCap { .. } => None,
                    };
                    if let Some(message) = msg {
                        problems.push(FamilyProblem {
                            family: family.id.clone(),
                            key: (*key).to_owned(),
                            message,
                        });
                    }
                }
                for (key, m) in &measured {
                    if budgeted.contains_key(key.as_str()) {
                        continue;
                    }
                    if let NumericVerdict::UnlistedOverCap { measured, cap } =
                        check_numeric_unlisted(*m, cap)
                    {
                        problems.push(FamilyProblem {
                            family: family.id.clone(),
                            key: key.clone(),
                            message: format!(
                                "{id}/{key}: {measured} exceeds cap {cap} (unlisted) — refactor or add a budget row; regenerate: {RERUN} --print {id}",
                                id = family.id
                            ),
                        });
                    }
                }
                if family.mode == "report" {
                    report_lines.push(format!(
                        "{} (report-only) — {} key(s) measured",
                        family.id,
                        measured.len()
                    ));
                    problems.retain(|p| p.family != family.id);
                }
            }
            "presence" => {
                let allowed: BTreeSet<String> =
                    family.entry.iter().map(|e| e.key.clone()).collect();
                let measured = invoke_provider_presence(root, &family.provider)?;
                for (key, verdict) in check_presence(&measured, &allowed) {
                    let message = match verdict {
                        PresenceVerdict::Stale => format!(
                            "{id}/{key}: listed but no longer violates — remove the stale allowlist entry; regenerate: {RERUN} --print {id}",
                            id = family.id
                        ),
                        PresenceVerdict::New { reason } => format!(
                            "{id}/{key}: {reason} — fix or allowlist via `{RERUN} --print {id}`",
                            id = family.id
                        ),
                    };
                    problems.push(FamilyProblem {
                        family: family.id.clone(),
                        key,
                        message,
                    });
                }
            }
            other => bail!("unknown family kind {other:?} in {CONFIG_PATH}"),
        }
    }

    if let Some(ids) = only {
        let known: BTreeSet<&str> = config.family.iter().map(|f| f.id.as_str()).collect();
        for id in ids {
            if !known.contains(id) {
                bail!("unknown family {id:?} in {CONFIG_PATH}");
            }
        }
    }

    Ok(FamilyCheckOutcome {
        problems,
        report_lines,
    })
}

fn is_artifact_ceiling(family: &Family) -> bool {
    family.mode == "artifact-ceiling"
}

fn absent_scheduled_artifact(family: &Family, measured: &BTreeMap<String, usize>) -> bool {
    is_artifact_ceiling(family) && measured.is_empty()
}

fn is_ceiling_only(family: &Family, artifact_ceiling: bool) -> bool {
    family.id == "suite-time" || artifact_ceiling
}

fn read_config(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading {CONFIG_PATH} at {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {CONFIG_PATH}"))
}

fn invoke_provider(root: &Path, provider: &str) -> Result<BTreeMap<String, usize>> {
    match provider {
        "file_lines_production" => measure_file_lines(root, false),
        "file_lines_test" => measure_file_lines(root, true),
        "bare_allow_per_crate" => {
            let m = crate::suppressions::measure(root)?;
            Ok(m.bare_by_crate)
        }
        "expect_per_lint_crate" => {
            let m = crate::suppressions::measure(root)?;
            Ok(m.expect_by_lint_crate
                .into_iter()
                .map(|((lint, crate_name), n)| (expect_key(&lint, &crate_name), n))
                .collect())
        }
        "agent_doc_bytes" => measure_agent_doc_bytes(root),
        "build_times" => measure_build_times(root),
        "export_volume_measured" => measure_export_volume_measured(root),
        "perf_dhat_budgets" => measure_perf_dhat_budgets(root),
        "test_layout_violations" => Ok(BTreeMap::new()),
        "public_surface_pub_mods" => measure_public_surface_pub_mods(root),
        "rust_function_complexity" => measure_rust_function_complexity(root),
        "suite_time" => measure_suite_time(root),
        other => bail!("unknown ratchet provider {other:?}"),
    }
}

fn measure_rust_function_complexity(root: &Path) -> Result<BTreeMap<String, usize>> {
    let mut files = Vec::new();
    collect_production_rust_files(&root.join("crates"), &mut files)?;
    collect_production_rust_files(&root.join("src"), &mut files)?;
    files.sort();

    let mut measured = BTreeMap::new();
    for path in files {
        let source = fs::read_to_string(&path)
            .with_context(|| format!("reading complexity source {}", path.display()))?;
        let syntax = syn::parse_file(&source)
            .with_context(|| format!("parsing complexity source {}", path.display()))?;
        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
        let mut functions = FunctionComplexityVisitor {
            path: &relative,
            measured: &mut measured,
        };
        functions.visit_file(&syntax);
    }
    Ok(measured)
}

fn collect_production_rust_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if path.file_name().is_some_and(|name| name == "tests") {
                continue;
            }
            collect_production_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_none_or(|name| name != "tests.rs")
        {
            files.push(path);
        }
    }
    Ok(())
}

struct FunctionComplexityVisitor<'a> {
    path: &'a str,
    measured: &'a mut BTreeMap<String, usize>,
}

impl<'ast> Visit<'ast> for FunctionComplexityVisitor<'_> {
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        self.record(&function.sig.ident.to_string(), &function.block);
    }

    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        self.record(&function.sig.ident.to_string(), &function.block);
    }
}

impl FunctionComplexityVisitor<'_> {
    fn record(&mut self, _name: &str, block: &syn::Block) {
        let mut counter = DecisionVisitor::default();
        counter.visit_block(block);
        if counter.count == 0 {
            return;
        }
        let crate_key = self
            .path
            .strip_prefix("crates/")
            .and_then(|path| path.split('/').nth(1))
            .unwrap_or("jackin-root");
        let entry = self.measured.entry(crate_key.to_owned()).or_default();
        *entry = (*entry).max(counter.count);
    }
}

#[derive(Default)]
struct DecisionVisitor {
    count: usize,
}

impl<'ast> Visit<'ast> for DecisionVisitor {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.count += 1;
        visit::visit_expr_if(self, node);
    }

    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        self.count += node.arms.len().saturating_sub(1);
        visit::visit_expr_match(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.count += 1;
        visit::visit_expr_for_loop(self, node);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.count += 1;
        visit::visit_expr_while(self, node);
    }

    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.count += 1;
        visit::visit_expr_loop(self, node);
    }

    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        if matches!(node.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) {
            self.count += 1;
        }
        visit::visit_expr_binary(self, node);
    }

    fn visit_expr_try(&mut self, node: &'ast syn::ExprTry) {
        self.count += 1;
        visit::visit_expr_try(self, node);
    }
}

fn measure_public_surface_pub_mods(root: &Path) -> Result<BTreeMap<String, usize>> {
    let crates_dir = root.join("crates");
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    if !crates_dir.is_dir() {
        return Ok(out);
    }
    for group in crate::fs_util::read_dir_sorted(&crates_dir)? {
        if !group.file_type()?.is_dir() {
            continue;
        }
        for entry in crate::fs_util::read_dir_sorted(&group.path())? {
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let lib = entry.path().join("src/lib.rs");
            if !lib.is_file() {
                continue;
            }
            let text =
                fs::read_to_string(&lib).with_context(|| format!("reading {}", lib.display()))?;
            let mut count = 0usize;
            for line in text.lines() {
                let t = line.trim_start();
                if t.starts_with("pub mod ") || t.starts_with("pub(crate) mod ") {
                    count += 1;
                }
            }
            out.insert(name, count);
        }
    }
    Ok(out)
}

const CURATED_PUB_MODS: &[(&str, &[&str])] = &[
    ("jackin-env", &["test_support"]),
    ("jackin-config", &["fixtures"]),
    ("jackin-core", &["container_paths"]),
];

pub(crate) fn check_curated_pub_mods(root: &Path) -> Result<()> {
    let mut problems = Vec::new();
    for &(crate_name, allowed) in CURATED_PUB_MODS {
        let Some(dir) = crate::fs_util::crate_dir(root, crate_name) else {
            problems.push(format!(
                "crates/*/{crate_name}/src/lib.rs: curated crate missing lib.rs"
            ));
            continue;
        };
        let lib = dir.join("src/lib.rs");
        if !lib.is_file() {
            problems.push(format!(
                "crates/*/{crate_name}/src/lib.rs: curated crate missing lib.rs"
            ));
            continue;
        }
        let text =
            fs::read_to_string(&lib).with_context(|| format!("reading {}", lib.display()))?;
        let allowed_set: BTreeSet<&str> = allowed.iter().copied().collect();
        for (idx, line) in text.lines().enumerate() {
            let t = line.trim_start();
            let Some(rest) = t.strip_prefix("pub mod ") else {
                continue;
            };
            let name = rest
                .split(|c: char| c == ';' || c == '{' || c.is_whitespace())
                .next()
                .unwrap_or("");
            if name.is_empty() {
                continue;
            }
            if !allowed_set.contains(name) {
                let rel = lib
                    .strip_prefix(root)
                    .unwrap_or(&lib)
                    .to_string_lossy()
                    .replace('\\', "/");
                problems.push(format!(
                    "{rel}:{}: unexpected `pub mod {name}` — \
                     curated surface allows only {allowed:?} (plan 019 env pilot guard)",
                    idx + 1
                ));
            }
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    bail!(
        "{} curated pub-mod surface violation(s):\n  {}\nRerun: cargo xtask lint arch",
        problems.len(),
        problems.join("\n  ")
    )
}

pub(crate) fn expect_key(lint: &str, crate_name: &str) -> String {
    format!("{lint}@{crate_name}")
}

fn invoke_provider_presence(root: &Path, provider: &str) -> Result<BTreeMap<String, String>> {
    match provider {
        "test_layout_violations" => crate::layout_lint::measure_violations(root),
        other => bail!("unknown presence provider {other:?}"),
    }
}

fn measure_file_lines(root: &Path, tests_only: bool) -> Result<BTreeMap<String, usize>> {
    let counts = crate::lint::measure_lines(root)?;
    let mut out = BTreeMap::new();
    for (path, lines) in counts {
        let relative_path = path.strip_prefix(root).unwrap_or(&path);
        let is_test = crate::lint::is_test_source_path(relative_path);
        if is_test != tests_only {
            continue;
        }
        let rel = relative_path.to_string_lossy().replace('\\', "/");
        out.insert(rel, lines);
    }
    Ok(out)
}

fn measure_perf_dhat_budgets(root: &Path) -> Result<BTreeMap<String, usize>> {
    let path = root.join("crates/apps/jackin-capsule/src/perf_budgets.rs");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading perf budgets at {}", path.display()))?;
    let mut out = BTreeMap::new();
    for (key, name) in [
        (
            "focused_full_snapshot_max_blocks",
            "FOCUSED_FULL_SNAPSHOT_MAX_BLOCKS",
        ),
        (
            "focused_full_snapshot_max_bytes",
            "FOCUSED_FULL_SNAPSHOT_MAX_BYTES",
        ),
        (
            "focused_borrowed_view_max_blocks",
            "FOCUSED_BORROWED_VIEW_MAX_BLOCKS",
        ),
        (
            "focused_borrowed_view_max_bytes",
            "FOCUSED_BORROWED_VIEW_MAX_BYTES",
        ),
    ] {
        let needle = format!("const {name}: usize = ");
        let Some(rest) = text.split(&needle).nth(1) else {
            bail!("missing {name} in {}", path.display());
        };
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let value: usize = digits
            .parse()
            .with_context(|| format!("parsing {name} value from {}", path.display()))?;
        out.insert(key.to_owned(), value);
    }
    Ok(out)
}

fn measure_export_volume_measured(root: &Path) -> Result<BTreeMap<String, usize>> {
    let artifact = root.join("target/telemetry-volume-ratchet.json");
    if !artifact.is_file() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(&artifact).with_context(|| {
        format!(
            "reading measured volume at {} — run `cargo nextest run -p jackin-diagnostics --all-features -E 'test(conformance_export_volume)'` first",
            artifact.display()
        )
    })?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing measured volume at {}", artifact.display()))?;
    let mut out = BTreeMap::new();
    for key in [
        "default_mode_logs",
        "default_mode_spans",
        "default_mode_metrics",
    ] {
        let Some(n) = value.get(key).and_then(serde_json::Value::as_u64) else {
            bail!(
                "measured volume at {} missing required key `{key}`",
                artifact.display()
            );
        };
        out.insert(key.to_owned(), n as usize);
    }
    Ok(out)
}

fn measure_agent_doc_bytes(root: &Path) -> Result<BTreeMap<String, usize>> {
    let mut out = BTreeMap::new();
    let candidates = ["AGENTS.md", "crates/AGENTS.md", "CLAUDE.md"];
    for rel in candidates {
        let path = root.join(rel);
        if path.is_file() {
            let n = fs::metadata(&path).map_or(0, |m| m.len() as usize);
            out.insert(rel.to_owned(), n);
        }
    }
    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        for group in crate::fs_util::read_dir_sorted(&crates_dir)? {
            if !group.file_type()?.is_dir() {
                continue;
            }
            for entry in crate::fs_util::read_dir_sorted(&group.path())? {
                let readme = entry.path().join("README.md");
                if readme.is_file() {
                    let rel = readme
                        .strip_prefix(root)
                        .unwrap_or(&readme)
                        .to_string_lossy()
                        .replace('\\', "/");
                    let n = fs::metadata(&readme).map_or(0, |m| m.len() as usize);
                    out.insert(rel, n);
                }
            }
        }
    }
    Ok(out)
}

fn measure_build_times(root: &Path) -> Result<BTreeMap<String, usize>> {
    let path = root.join("target/build-times.json");
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading build times at {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing build times at {}", path.display()))?;
    let crates = value
        .as_object()
        .with_context(|| format!("build times at {} must be a JSON object", path.display()))?;
    let mut out = BTreeMap::new();
    for (crate_name, timings) in crates {
        for field in ["clean_s", "incremental_s"] {
            let seconds = timings
                .get(field)
                .and_then(serde_json::Value::as_u64)
                .with_context(|| {
                    format!(
                        "build times at {} missing integer {crate_name}.{field}",
                        path.display()
                    )
                })?;
            out.insert(format!("{crate_name}.{field}"), seconds as usize);
        }
    }
    Ok(out)
}

fn junit_seconds_to_ms(raw: &str) -> u64 {
    let (whole, frac) = raw.split_once('.').unwrap_or((raw, ""));
    let Ok(secs) = whole.parse::<u64>() else {
        return 0;
    };
    let mut ms_part = 0u64;
    let mut digits = frac.chars().filter(char::is_ascii_digit).take(3);
    if let Some(a) = digits.next() {
        ms_part += u64::from(a.to_digit(10).unwrap_or(0)) * 100;
    }
    if let Some(b) = digits.next() {
        ms_part += u64::from(b.to_digit(10).unwrap_or(0)) * 10;
    }
    if let Some(c) = digits.next() {
        ms_part += u64::from(c.to_digit(10).unwrap_or(0));
    }
    secs.saturating_mul(1000).saturating_add(ms_part)
}

pub(crate) fn measure_suite_time(root: &Path) -> Result<BTreeMap<String, usize>> {
    let mut out = BTreeMap::new();
    let candidates = [
        root.join("target/nextest/ci/junit.xml"),
        root.join("target/nextest/default/junit.xml"),
    ];
    let total_ms = if let Some(path) = candidates.into_iter().find(|p| p.is_file()) {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading suite junit at {}", path.display()))?;
        let mut total_ms: u64 = 0;
        for part in text.split("time=\"").skip(1) {
            let Some(end) = part.find('"') else {
                continue;
            };
            let raw = &part[..end];
            total_ms = total_ms.saturating_add(junit_seconds_to_ms(raw));
        }
        total_ms
    } else {
        0
    };
    out.insert("junit_total_ms".to_owned(), total_ms as usize);
    Ok(out)
}

fn print_family(root: &Path, config: &Config, family_id: &str) -> Result<()> {
    let family = config
        .family
        .iter()
        .find(|f| f.id == family_id)
        .with_context(|| format!("unknown family {family_id:?}"))?;
    match family.kind.as_str() {
        "numeric" => {
            let measured = invoke_provider(root, &family.provider)?;
            emit(&format!("# ratchet family {family_id} regenerated entries"));
            for (key, bound) in &measured {
                let print = match (family.mode.as_str(), family.cap) {
                    ("report", _) => true,
                    (_, Some(0)) => *bound > 0,
                    (_, Some(cap)) => *bound > cap,
                    (_, None) => *bound > 0,
                };
                if print {
                    emit(&format!("[[family.entry]]\nkey = {key:?}\nbound = {bound}"));
                }
            }
        }
        "presence" => {
            let measured = invoke_provider_presence(root, &family.provider)?;
            emit(&format!("# ratchet family {family_id} regenerated keys"));
            for key in measured.keys() {
                emit(&format!("[[family.entry]]\nkey = {key:?}"));
            }
        }
        other => bail!("unknown kind {other:?}"),
    }
    Ok(())
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; ratchet output is user-facing"
)]
fn emit(line: &str) {
    if report::human_output() {
        println!("{line}");
    }
}
