//! Documentation-tree automation: scaffold and validate Fumadocs `meta.json`
//! sidebars for roadmap items and research dossiers.
//!
//! The docs site is Fumadocs; each directory under `docs/content/docs/` carries
//! a `meta.json` whose `pages` array orders the sidebar. These tasks keep that
//! wiring correct without hand-editing JSON:
//!
//! ```sh
//! cargo xtask change new <slug> --group <group>   # scaffold a roadmap item
//! cargo xtask research scaffold <slug>            # scaffold a research dossier
//! cargo xtask research check                      # validate research meta.json
//! cargo xtask roadmap audit                       # validate roadmap meta.json
//! ```

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

const DOCS_ROOT: &str = "docs/content/docs";
const ROADMAP_REL: &str = "reference/roadmap";
const RESEARCH_REL: &str = "research";

// ---------------------------------------------------------------------------
// CLI surface
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub(crate) enum ChangeCommand {
    /// Scaffold a new roadmap item `.mdx` and register it in a group sidebar.
    New(ChangeNewArgs),
}

#[derive(Args)]
pub(crate) struct ChangeNewArgs {
    /// Kebab-case slug; becomes `<slug>.mdx` under the roadmap directory.
    slug: String,
    /// Sidebar group to register the item under, e.g. `operator-surface` or
    /// `(operator-surface)`. Must be an existing roadmap group directory.
    #[arg(long)]
    group: String,
    /// Sidebar/page title. Defaults to a title-cased form of the slug.
    #[arg(long)]
    title: Option<String>,
}

#[derive(Subcommand)]
pub(crate) enum ResearchCommand {
    /// Scaffold a new research dossier folder and register it in the sidebar.
    Scaffold(ResearchScaffoldArgs),
    /// Validate that every research `meta.json` page resolves and no `.mdx` is
    /// orphaned.
    Check,
}

#[derive(Args)]
pub(crate) struct ResearchScaffoldArgs {
    /// Kebab-case slug; becomes the dossier directory name.
    slug: String,
    /// Dossier title. Defaults to a title-cased form of the slug.
    #[arg(long)]
    title: Option<String>,
}

#[derive(Subcommand)]
pub(crate) enum RoadmapCommand {
    /// Validate that every roadmap `meta.json` page resolves and no item `.mdx`
    /// is orphaned.
    Audit,
    /// Retire a shipped roadmap item. `--plan` prints the worklist; `--apply`
    /// does the mechanical removal (drop the sidebar entry, delete the `.mdx`,
    /// audit, fail on a dangling inbound link); `--partial` marks it partially
    /// implemented and keeps the page.
    Retire(RoadmapRetireArgs),
}

#[derive(Args)]
pub(crate) struct RoadmapRetireArgs {
    /// Roadmap item slug (the `<slug>.mdx` under the roadmap directory).
    slug: String,
    /// Print the retirement worklist — page content, inbound links, and the
    /// sidebar entry — without changing anything. This is the default.
    #[arg(long, conflicts_with_all = ["apply", "partial"])]
    plan: bool,
    /// Apply the mechanical removal: drop the `meta.json` entry, delete the
    /// `.mdx`, run the audit, and fail if any inbound link still resolves to it.
    #[arg(long, conflicts_with_all = ["plan", "partial"])]
    apply: bool,
    /// Mark the item `**Status**: Partially implemented` and keep the page.
    #[arg(long, conflicts_with_all = ["plan", "apply"])]
    partial: bool,
}

pub(crate) fn run_change(command: ChangeCommand) -> Result<()> {
    match command {
        ChangeCommand::New(args) => change_new(args),
    }
}

pub(crate) fn run_research(command: ResearchCommand) -> Result<()> {
    match command {
        ResearchCommand::Scaffold(args) => research_scaffold(args),
        ResearchCommand::Check => validate_tree(&research_dir()?, "research"),
    }
}

pub(crate) fn run_roadmap(command: RoadmapCommand) -> Result<()> {
    match command {
        RoadmapCommand::Audit => validate_tree(&roadmap_dir()?, "roadmap"),
        RoadmapCommand::Retire(args) => {
            let docs_root = repo_root()?.join(DOCS_ROOT);
            roadmap_retire(&docs_root, args)
        }
    }
}

// ---------------------------------------------------------------------------
// Locating the docs tree
// ---------------------------------------------------------------------------

/// Walk up from the current directory to the repo root (the directory that
/// contains `docs/content/docs`).
pub(crate) fn repo_root() -> Result<PathBuf> {
    let start = std::env::current_dir().context("resolving current directory")?;
    for dir in start.ancestors() {
        if dir.join(DOCS_ROOT).is_dir() {
            return Ok(dir.to_path_buf());
        }
    }
    bail!(
        "could not locate the repo root (no `{DOCS_ROOT}` found above {})",
        start.display()
    )
}

fn roadmap_dir() -> Result<PathBuf> {
    Ok(repo_root()?.join(DOCS_ROOT).join(ROADMAP_REL))
}

fn research_dir() -> Result<PathBuf> {
    Ok(repo_root()?.join(DOCS_ROOT).join(RESEARCH_REL))
}

// ---------------------------------------------------------------------------
// meta.json helpers
// ---------------------------------------------------------------------------

/// Read a `meta.json` into a JSON value.
fn read_meta(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Write a JSON value as pretty 2-space `meta.json` with a trailing newline,
/// matching the repo's existing formatting.
fn write_meta(path: &Path, value: &Value) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)
        .with_context(|| format!("serializing {}", path.display()))?;
    text.push('\n');
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

/// Append `entry` to a `meta.json`'s `pages` array if not already present.
fn append_page(meta_path: &Path, entry: &str) -> Result<()> {
    let mut meta = read_meta(meta_path)?;
    let pages = meta
        .get_mut("pages")
        .and_then(Value::as_array_mut)
        .with_context(|| format!("`pages` is not an array in {}", meta_path.display()))?;
    if pages.iter().any(|p| p.as_str() == Some(entry)) {
        return Ok(());
    }
    pages.push(Value::String(entry.to_owned()));
    write_meta(meta_path, &meta)
}

// ---------------------------------------------------------------------------
// Slug + title helpers
// ---------------------------------------------------------------------------

/// Reject slugs that are not lowercase kebab-case (matching existing file
/// names and Fumadocs slug rules).
fn validate_slug(slug: &str) -> Result<()> {
    let ok = !slug.is_empty()
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--");
    if !ok {
        bail!("invalid slug `{slug}`: use lowercase letters, digits, and single hyphens");
    }
    Ok(())
}

/// Title-case a kebab slug for a default page title (`idle-runtime` → `Idle
/// Runtime`).
fn title_from_slug(slug: &str) -> String {
    slug.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Scaffolding
// ---------------------------------------------------------------------------

fn change_new(args: ChangeNewArgs) -> Result<()> {
    change_new_in(&roadmap_dir()?, args)
}

fn change_new_in(roadmap: &Path, args: ChangeNewArgs) -> Result<()> {
    validate_slug(&args.slug)?;
    let title = args.title.unwrap_or_else(|| title_from_slug(&args.slug));

    // Normalize the group to its `(group)` directory name.
    let group_name = args.group.trim_matches(['(', ')'].as_ref());
    let group_dir = roadmap.join(format!("({group_name})"));
    let group_meta = group_dir.join("meta.json");
    if !group_meta.is_file() {
        bail!(
            "roadmap group `({group_name})` not found at {} — pick an existing group",
            group_dir.display()
        );
    }

    let item_path = roadmap.join(format!("{}.mdx", args.slug));
    if item_path.exists() {
        bail!("roadmap item already exists: {}", item_path.display());
    }

    let body = format!(
        "---\ntitle: \"{title}\"\n---\n\n\
         **Status**: Open — design proposal\n\n\
         ## Problem\n\n<!-- What concrete problem or gap does this address? -->\n\n\
         ## Why It Matters\n\n<!-- Why is this worth doing now? -->\n\n\
         ## Design\n\n<!-- Filled in by brainstorming. -->\n\n\
         ## Tasks\n\n<!-- Filled in by planning. -->\n\n\
         ## Related Files\n\n<!-- Source paths this item touches. -->\n"
    );
    fs::write(&item_path, body).with_context(|| format!("writing {}", item_path.display()))?;

    // Group meta references siblings in the parent roadmap dir as `../<slug>`.
    append_page(&group_meta, &format!("../{}", args.slug))?;

    report_created(&[item_path.as_path(), group_meta.as_path()]);
    Ok(())
}

fn research_scaffold(args: ResearchScaffoldArgs) -> Result<()> {
    research_scaffold_in(&research_dir()?, args)
}

fn research_scaffold_in(research: &Path, args: ResearchScaffoldArgs) -> Result<()> {
    validate_slug(&args.slug)?;
    let title = args.title.unwrap_or_else(|| title_from_slug(&args.slug));

    let dossier = research.join(&args.slug);
    if dossier.exists() {
        bail!("research dossier already exists: {}", dossier.display());
    }
    fs::create_dir_all(&dossier).with_context(|| format!("creating {}", dossier.display()))?;

    let index = dossier.join("index.mdx");
    fs::write(
        &index,
        format!(
            "---\ntitle: \"{title}\"\n---\n\n# {title}\n\n\
             Research dossier. Specification: [`prompt`](prompt/).\n\n\
             ## Headline numbers\n\n<!-- Key findings, each with a source. -->\n\n\
             ## How to read\n\n<!-- Chapter map. -->\n"
        ),
    )
    .with_context(|| format!("writing {}", index.display()))?;

    let prompt = dossier.join("prompt.mdx");
    fs::write(
        &prompt,
        format!(
            "---\ntitle: \"{title} Brief\"\n---\n\n# {title} Brief\n\n\
             > **How to run this file:** `/goal Follow {}`. You are the researcher; \
             this brief is your full specification.\n\n\
             ## Mission\n\n<!-- What to produce, the evidence bar, the chapter list. -->\n",
            args.slug
        ),
    )
    .with_context(|| format!("writing {}", prompt.display()))?;

    let meta = dossier.join("meta.json");
    write_meta(
        &meta,
        &json!({ "title": title, "defaultOpen": false, "pages": ["index", "prompt"] }),
    )?;

    // Register the dossier in the parent research sidebar.
    append_page(&research.join("meta.json"), &args.slug)?;

    report_created(&[
        index.as_path(),
        prompt.as_path(),
        meta.as_path(),
        research.join("meta.json").as_path(),
    ]);
    Ok(())
}

fn report_created(paths: &[&Path]) {
    #[expect(
        clippy::print_stdout,
        reason = "jackin-xtask is a CLI; the created-file list is its output"
    )]
    {
        println!("Wrote:");
        for path in paths {
            println!("  {}", path.display());
        }
    }
}

// ---------------------------------------------------------------------------
// Retirement
// ---------------------------------------------------------------------------

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the retirement worklist/report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

/// Remove `entry` from a `meta.json`'s `pages` array. Errors if it is absent.
fn remove_page(meta_path: &Path, entry: &str) -> Result<()> {
    let mut meta = read_meta(meta_path)?;
    let pages = meta
        .get_mut("pages")
        .and_then(Value::as_array_mut)
        .with_context(|| format!("`pages` is not an array in {}", meta_path.display()))?;
    let before = pages.len();
    pages.retain(|p| p.as_str() != Some(entry));
    if pages.len() == before {
        bail!("`{entry}` not found in {}", meta_path.display());
    }
    write_meta(meta_path, &meta)
}

/// Find the `(group)/meta.json` whose `pages` registers `../<slug>`.
fn find_group_meta(roadmap: &Path, slug: &str) -> Result<Option<PathBuf>> {
    let entry = format!("../{slug}");
    for dir in fs::read_dir(roadmap).with_context(|| format!("reading {}", roadmap.display()))? {
        let path = dir?.path();
        let meta = path.join("meta.json");
        if !meta.is_file() {
            continue;
        }
        let value = read_meta(&meta)?;
        let referenced = value
            .get("pages")
            .and_then(Value::as_array)
            .is_some_and(|pages| pages.iter().any(|p| p.as_str() == Some(entry.as_str())));
        if referenced {
            return Ok(Some(meta));
        }
    }
    Ok(None)
}

/// True when `line` references `slug` as a roadmap route (`roadmap/<slug>`) or a
/// sidebar entry (`../<slug>`), bounded so `auth` does not match `auth-health`.
fn line_references_slug(line: &str, slug: &str) -> bool {
    for token in [format!("roadmap/{slug}"), format!("../{slug}")] {
        let mut rest = line;
        while let Some(pos) = rest.find(&token) {
            let after = &rest[pos + token.len()..];
            let bounded = after
                .chars()
                .next()
                .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '-');
            if bounded {
                return true;
            }
            rest = &rest[pos + token.len()..];
        }
    }
    false
}

/// Collect every `(file, line-number, line)` under `docs_root` that links to the
/// slug, skipping `exclude` (the item's own page).
fn inbound_links(
    docs_root: &Path,
    slug: &str,
    exclude: &Path,
) -> Result<Vec<(PathBuf, usize, String)>> {
    let mut hits = Vec::new();
    let mut files = Vec::new();
    collect_text_files(docs_root, &mut files)?;
    for file in files {
        if file == exclude {
            continue;
        }
        let text =
            fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
        for (num, line) in text.lines().enumerate() {
            if line_references_slug(line, slug) {
                hits.push((file.clone(), num + 1, line.trim().to_owned()));
            }
        }
    }
    hits.sort();
    Ok(hits)
}

/// Recursively collect `.mdx` and `.json` files under `dir`.
fn collect_text_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_text_files(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext == "mdx" || ext == "json")
        {
            out.push(path);
        }
    }
    Ok(())
}

fn roadmap_retire(docs_root: &Path, args: RoadmapRetireArgs) -> Result<()> {
    let roadmap = docs_root.join(ROADMAP_REL);
    let item = roadmap.join(format!("{}.mdx", args.slug));
    if !item.is_file() {
        bail!("no roadmap item at {}", item.display());
    }

    if args.partial {
        return retire_partial(&item);
    }
    if args.apply {
        return retire_apply(docs_root, &roadmap, &item, &args.slug);
    }
    retire_plan(docs_root, &roadmap, &item, &args.slug)
}

/// `--plan`: read-only worklist for the agent. Changes nothing.
fn retire_plan(docs_root: &Path, roadmap: &Path, item: &Path, slug: &str) -> Result<()> {
    let content =
        fs::read_to_string(item).with_context(|| format!("reading {}", item.display()))?;
    let group = find_group_meta(roadmap, slug)?;
    let links = inbound_links(docs_root, slug, item)?;

    emit(&format!("Retirement plan for `{slug}` (read-only)\n"));
    emit("1. Move the page content below into canonical docs (operator detail →");
    emit("   guides/commands, design detail → reference); write a ## Completed bullet");
    emit("   in roadmap/index.mdx; repoint the inbound links listed below.");
    emit("2. Then run: cargo xtask roadmap retire <slug> --apply\n");
    match group {
        Some(meta) => emit(&format!(
            "Sidebar entry to drop: `../{slug}` in {}",
            meta.display()
        )),
        None => emit(&format!(
            "WARNING: `../{slug}` is not registered in any roadmap group sidebar"
        )),
    }
    if links.is_empty() {
        emit("\nInbound links: none.");
    } else {
        emit(&format!(
            "\nInbound links ({}) — repoint each before --apply:",
            links.len()
        ));
        for (file, num, line) in &links {
            emit(&format!("  {}:{num}: {line}", file.display()));
        }
    }
    emit(&format!("\n--- {} ---", item.display()));
    emit(content.trim_end());
    Ok(())
}

/// `--partial`: keep the page; set its status to Partially implemented.
fn retire_partial(item: &Path) -> Result<()> {
    let content =
        fs::read_to_string(item).with_context(|| format!("reading {}", item.display()))?;
    let mut replaced = false;
    let updated = content
        .lines()
        .map(|line| {
            if !replaced && line.trim_start().starts_with("**Status**:") {
                replaced = true;
                "**Status**: Partially implemented".to_owned()
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !replaced {
        bail!("no `**Status**:` line found in {}", item.display());
    }
    let updated = format!("{}\n", updated.trim_end());
    fs::write(item, updated).with_context(|| format!("writing {}", item.display()))?;
    emit(&format!(
        "Set {} to `**Status**: Partially implemented` (page kept). Name the remaining phases.",
        item.display()
    ));
    Ok(())
}

/// `--apply`: drop the sidebar entry, delete the page, audit, fail on a dangling
/// inbound link.
fn retire_apply(docs_root: &Path, roadmap: &Path, item: &Path, slug: &str) -> Result<()> {
    let meta = find_group_meta(roadmap, slug)?
        .with_context(|| format!("`../{slug}` is not registered in any roadmap group sidebar"))?;

    // Gate BEFORE any mutation: the only reference allowed to survive is the
    // group's own sidebar entry (which this command removes). Any other inbound
    // link must be repointed first — so check, and bail, while the page and the
    // sidebar are still intact. Checking after deletion would leave a half-retired
    // tree behind on failure.
    let dangling: Vec<_> = inbound_links(docs_root, slug, item)?
        .into_iter()
        .filter(|(file, _, _)| file != &meta)
        .collect();
    if !dangling.is_empty() {
        let list = dangling
            .iter()
            .map(|(file, num, line)| format!("  {}:{num}: {line}", file.display()))
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "{} inbound link(s) still resolve to `{slug}` — repoint them before --apply (nothing changed):\n{list}",
            dangling.len()
        );
    }

    remove_page(&meta, &format!("../{slug}"))?;
    fs::remove_file(item).with_context(|| format!("deleting {}", item.display()))?;
    validate_tree(roadmap, "roadmap")?;
    emit(&format!(
        "Retired `{slug}`: removed `../{slug}` from {}, deleted the page, sidebar audit clean, no dangling links.",
        meta.display()
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a Fumadocs subtree rooted at `root`: every `meta.json` page entry
/// must resolve to a file or directory on disk, and every `.mdx` page in the
/// subtree must be referenced by some `meta.json`. Returns an error listing all
/// problems, or `Ok` when the tree is clean.
fn validate_tree(root: &Path, label: &str) -> Result<()> {
    if !root.is_dir() {
        bail!("{label} directory not found: {}", root.display());
    }

    let mut metas = Vec::new();
    collect_meta_files(root, &mut metas)?;

    let mut problems = Vec::new();
    let mut referenced = BTreeSet::new();

    for meta_path in &metas {
        let meta = read_meta(meta_path)?;
        let dir = meta_path.parent().unwrap_or(root);
        let Some(pages) = meta.get("pages").and_then(Value::as_array) else {
            problems.push(format!("{}: missing `pages` array", meta_path.display()));
            continue;
        };
        for page in pages {
            let Some(entry) = page.as_str() else {
                problems.push(format!("{}: non-string page entry", meta_path.display()));
                continue;
            };
            match resolve_entry(dir, entry) {
                Some(resolved) => {
                    referenced.insert(resolved);
                }
                None => problems.push(format!(
                    "{}: page `{entry}` resolves to nothing on disk",
                    meta_path.display()
                )),
            }
        }
    }

    // Orphan check: every `.mdx` in the subtree must be referenced.
    let mut mdx_files = Vec::new();
    collect_mdx_files(root, &mut mdx_files)?;
    for mdx in mdx_files {
        let canonical = fs::canonicalize(&mdx).unwrap_or(mdx.clone());
        if !referenced.contains(&canonical) {
            problems.push(format!(
                "{}: not referenced by any meta.json (orphaned sidebar page)",
                mdx.display()
            ));
        }
    }

    if problems.is_empty() {
        report_clean(label, metas.len());
        return Ok(());
    }
    problems.sort();
    bail!(
        "{label} sidebar has {} problem(s):\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

/// Resolve a `pages` entry relative to its `meta.json` directory to an existing
/// path, returning the canonicalized target. Handles `slug`, `../slug`,
/// `(group)`, and `index` forms.
fn resolve_entry(dir: &Path, entry: &str) -> Option<PathBuf> {
    let candidates = [
        dir.join(format!("{entry}.mdx")),
        dir.join(entry).join("index.mdx"),
        dir.join(entry).join("meta.json"),
    ];
    for candidate in candidates {
        if candidate.exists() {
            // Canonicalize to the `.mdx` for files; for a directory entry
            // (`meta.json`/`index.mdx` candidate) we key on that file so the
            // orphan check lines up with `collect_mdx_files`.
            return fs::canonicalize(&candidate).ok();
        }
    }
    None
}

/// Recursively collect every `meta.json` under `root`.
fn collect_meta_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let meta = dir.join("meta.json");
    if meta.is_file() {
        out.push(meta);
    }
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_meta_files(&path, out)?;
        }
    }
    Ok(())
}

/// Recursively collect every `.mdx` file under `root`.
fn collect_mdx_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_mdx_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "mdx") {
            out.push(path);
        }
    }
    Ok(())
}

fn report_clean(label: &str, meta_count: usize) {
    #[expect(
        clippy::print_stdout,
        reason = "jackin-xtask is a CLI; the audit result is its output"
    )]
    {
        println!("{label} sidebar OK — {meta_count} meta.json file(s), all pages resolve.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    /// `write_meta` plus parent-dir creation, for building nested test trees.
    fn write_meta_mk(path: &Path, value: &Value) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        write_meta(path, value).unwrap();
    }

    #[test]
    fn title_casing() {
        assert_eq!(
            title_from_slug("idle-runtime-cleanup"),
            "Idle Runtime Cleanup"
        );
        assert_eq!(title_from_slug("orca"), "Orca");
    }

    #[test]
    fn slug_validation() {
        assert!(validate_slug("agent-codenames").is_ok());
        assert!(validate_slug("a1-b2").is_ok());
        assert!(validate_slug("Bad-Slug").is_err());
        assert!(validate_slug("-leading").is_err());
        assert!(validate_slug("double--hyphen").is_err());
        assert!(validate_slug("").is_err());
    }

    #[test]
    fn append_page_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let meta = dir.path().join("meta.json");
        write_meta(&meta, &json!({ "title": "X", "pages": ["index"] })).unwrap();
        append_page(&meta, "alpha").unwrap();
        append_page(&meta, "alpha").unwrap();
        let pages = read_meta(&meta).unwrap();
        let pages = pages["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[1], "alpha");
    }

    #[test]
    fn validate_tree_passes_clean_and_flags_broken_and_orphan() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        // Clean tree: meta lists index + alpha, both present.
        write(&r.join("index.mdx"), "---\ntitle: I\n---\n");
        write(&r.join("alpha.mdx"), "---\ntitle: A\n---\n");
        write_meta(
            &r.join("meta.json"),
            &json!({ "pages": ["index", "alpha"] }),
        )
        .unwrap();
        validate_tree(r, "test").expect("clean tree should pass");

        // Broken reference: page `ghost` has no file.
        write_meta(
            &r.join("meta.json"),
            &json!({ "pages": ["index", "alpha", "ghost"] }),
        )
        .unwrap();
        let err = validate_tree(r, "test").unwrap_err().to_string();
        assert!(err.contains("ghost"), "should flag broken ref: {err}");

        // Orphan: drop `alpha` from pages while the file remains.
        write_meta(&r.join("meta.json"), &json!({ "pages": ["index"] })).unwrap();
        let err = validate_tree(r, "test").unwrap_err().to_string();
        assert!(err.contains("alpha.mdx"), "should flag orphan: {err}");
    }

    #[test]
    fn validate_tree_resolves_group_and_parent_cross_refs() {
        // Mirror the roadmap shape: a `(group)/` whose pages reference a sibling
        // item one level up as `../item`.
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        write(&r.join("index.mdx"), "---\ntitle: I\n---\n");
        write(&r.join("item.mdx"), "---\ntitle: It\n---\n");
        write_meta(
            &r.join("meta.json"),
            &json!({ "pages": ["index", "(grp)"] }),
        )
        .unwrap();
        write_meta_mk(&r.join("(grp)/meta.json"), &json!({ "pages": ["../item"] }));
        validate_tree(r, "test").expect("(group) + ../item should resolve");

        // Break the cross-ref: point at a missing sibling.
        write_meta_mk(
            &r.join("(grp)/meta.json"),
            &json!({ "pages": ["../ghost"] }),
        );
        let err = validate_tree(r, "test").unwrap_err().to_string();
        assert!(err.contains("ghost"), "should flag broken ../ ref: {err}");
        assert!(
            err.contains("item.mdx"),
            "now-unreferenced item is orphaned: {err}"
        );
    }

    #[test]
    fn change_new_in_scaffolds_and_registers() {
        let roadmap = tempfile::tempdir().unwrap();
        let r = roadmap.path();
        write_meta_mk(
            &r.join("(operator-surface)/meta.json"),
            &json!({ "pages": [] }),
        );

        change_new_in(
            r,
            ChangeNewArgs {
                slug: "new-item".to_owned(),
                group: "operator-surface".to_owned(),
                title: None,
            },
        )
        .unwrap();

        let body = fs::read_to_string(r.join("new-item.mdx")).unwrap();
        assert!(
            body.contains("title: \"New Item\""),
            "title-cased frontmatter: {body}"
        );
        assert!(body.contains("## Problem") && body.contains("## Design"));
        let pages = read_meta(&r.join("(operator-surface)/meta.json")).unwrap();
        assert_eq!(pages["pages"].as_array().unwrap()[0], "../new-item");
    }

    #[test]
    fn change_new_in_rejects_unknown_group() {
        let roadmap = tempfile::tempdir().unwrap();
        let err = change_new_in(
            roadmap.path(),
            ChangeNewArgs {
                slug: "x".to_owned(),
                group: "nope".to_owned(),
                title: None,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("nope"), "should name the missing group: {err}");
    }

    #[test]
    fn research_scaffold_in_creates_dossier_and_registers() {
        let research = tempfile::tempdir().unwrap();
        let r = research.path();
        write_meta(&r.join("meta.json"), &json!({ "pages": [] })).unwrap();

        research_scaffold_in(
            r,
            ResearchScaffoldArgs {
                slug: "my-study".to_owned(),
                title: None,
            },
        )
        .unwrap();

        assert!(r.join("my-study/index.mdx").is_file());
        assert!(r.join("my-study/prompt.mdx").is_file());
        let dossier_meta = read_meta(&r.join("my-study/meta.json")).unwrap();
        assert_eq!(dossier_meta["pages"], json!(["index", "prompt"]));
        let parent = read_meta(&r.join("meta.json")).unwrap();
        assert_eq!(parent["pages"].as_array().unwrap()[0], "my-study");
    }

    #[test]
    fn line_references_slug_is_boundary_safe() {
        assert!(line_references_slug(
            "see /reference/roadmap/auth/ for",
            "auth"
        ));
        assert!(line_references_slug("    \"../auth\"", "auth"));
        assert!(!line_references_slug(
            "/reference/roadmap/auth-health/",
            "auth"
        ));
        assert!(!line_references_slug("nothing here", "auth"));
    }

    /// Build a `docs/content/docs` shape with one roadmap item registered in a
    /// group, plus optional extra files. Returns the docs-root temp dir.
    fn roadmap_fixture(extra: &[(&str, &str)]) -> tempfile::TempDir {
        let docs = tempfile::tempdir().unwrap();
        let d = docs.path();
        write_meta_mk(
            &d.join("reference/roadmap/(grp)/meta.json"),
            &json!({ "pages": ["../shipme"] }),
        );
        write(
            &d.join("reference/roadmap/shipme.mdx"),
            "---\ntitle: Ship Me\n---\n\n**Status**: Open\n\n## Problem\n\nbody\n",
        );
        for (rel, body) in extra {
            write(&d.join(rel), body);
        }
        docs
    }

    #[test]
    fn retire_apply_removes_entry_and_page_when_clean() {
        let docs = roadmap_fixture(&[]);
        let d = docs.path();
        roadmap_retire(
            d,
            RoadmapRetireArgs {
                slug: "shipme".to_owned(),
                plan: false,
                apply: true,
                partial: false,
            },
        )
        .expect("clean retire should succeed");
        assert!(
            !d.join("reference/roadmap/shipme.mdx").exists(),
            "page deleted"
        );
        let meta = read_meta(&d.join("reference/roadmap/(grp)/meta.json")).unwrap();
        assert!(
            meta["pages"].as_array().unwrap().is_empty(),
            "sidebar entry dropped"
        );
    }

    #[test]
    fn retire_apply_fails_on_dangling_inbound_link() {
        let docs = roadmap_fixture(&[(
            "guides/foo.mdx",
            "---\ntitle: F\n---\n\nSee [the work](/reference/roadmap/shipme/).\n",
        )]);
        let err = roadmap_retire(
            docs.path(),
            RoadmapRetireArgs {
                slug: "shipme".to_owned(),
                plan: false,
                apply: true,
                partial: false,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("shipme") && err.contains("guides/foo.mdx"),
            "should flag dangling link: {err}"
        );
        // Fail-closed: nothing is mutated when the gate trips.
        let d = docs.path();
        assert!(
            d.join("reference/roadmap/shipme.mdx").exists(),
            "page must survive"
        );
        let meta = read_meta(&d.join("reference/roadmap/(grp)/meta.json")).unwrap();
        assert_eq!(meta["pages"][0], "../shipme", "sidebar entry must survive");
    }

    #[test]
    fn retire_partial_sets_status_and_keeps_page() {
        let docs = roadmap_fixture(&[]);
        let item = docs.path().join("reference/roadmap/shipme.mdx");
        roadmap_retire(
            docs.path(),
            RoadmapRetireArgs {
                slug: "shipme".to_owned(),
                plan: false,
                apply: false,
                partial: true,
            },
        )
        .unwrap();
        let body = fs::read_to_string(&item).unwrap();
        assert!(item.exists(), "page kept");
        assert!(
            body.contains("**Status**: Partially implemented"),
            "status updated: {body}"
        );
    }
}
