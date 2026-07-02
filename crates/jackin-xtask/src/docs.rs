//! Documentation-tree automation: scaffold and validate Fumadocs `meta.json`
//! sidebars for roadmap items and research dossiers.
//!
//! The docs site is Fumadocs; each directory under `docs/content/docs/` carries
//! a `meta.json` whose `pages` array orders the sidebar. These tasks keep that
//! wiring correct without hand-editing JSON:
//!
//! ```sh
//! cargo xtask change new <slug> --group <group>   # scaffold a roadmap item
//! cargo xtask docs repo-links                     # validate repo-file links
//! cargo xtask research scaffold <slug>            # scaffold a research dossier
//! cargo xtask research check                      # validate research meta.json
//! cargo xtask roadmap audit                       # validate roadmap meta.json
//! ```

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

const DOCS_ROOT: &str = "docs/content/docs";
const ROADMAP_REL: &str = "roadmap";
const RESEARCH_REL: &str = "research";
const REPO_FILE_PREFIXES: &[&str] = &[
    "crates/", "src/", "docs/", "docker/", ".github/", "scripts/",
];
const REPO_TOP_LEVEL_FILES: &[&str] = &[
    "AGENTS.md",
    "Cargo.lock",
    "Cargo.toml",
    "ENGINEERING.md",
    "PROJECT_STRUCTURE.md",
    "PULL_REQUESTS.md",
    "README.md",
    "TESTING.md",
    "docker-bake.hcl",
    "mise.toml",
    "release.toml",
    "renovate.json",
];
const GITHUB_BLOB_PREFIX: &str = "https://github.com/jackin-project/jackin/blob/main/";
const GITHUB_TREE_PREFIX: &str = "https://github.com/jackin-project/jackin/tree/main/";

// ---------------------------------------------------------------------------
// CLI surface
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub(crate) enum ChangeCommand {
    /// Scaffold a new roadmap item `.mdx` and register it in a group sidebar.
    New(ChangeNewArgs),
}

#[derive(Subcommand)]
pub(crate) enum DocsCommand {
    /// Validate that repository file references use checked link components.
    RepoLinks,
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

pub(crate) fn run_docs(command: DocsCommand) -> Result<()> {
    match command {
        DocsCommand::RepoLinks => check_repo_links(&repo_root()?),
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
// Repository file reference validation
// ---------------------------------------------------------------------------

fn check_repo_links(root: &Path) -> Result<()> {
    let content_root = root.join(DOCS_ROOT);
    check_repo_links_in(root, &content_root)
}

fn check_repo_links_in(root: &Path, content_root: &Path) -> Result<()> {
    if !content_root.is_dir() {
        bail!(
            "docs content directory not found: {}",
            content_root.display()
        );
    }

    let mut files = Vec::new();
    collect_mdx_files(content_root, &mut files)?;

    let mut failures = Vec::new();
    for file in files {
        check_repo_links_file(root, &file, &mut failures)?;
    }

    if failures.is_empty() {
        report_repo_links_clean();
        return Ok(());
    }
    failures.sort();
    bail!(
        "repository file references must be verifiable links ({} problem(s)):\n  {}",
        failures.len(),
        failures.join("\n  ")
    )
}

fn check_repo_links_file(root: &Path, file: &Path, failures: &mut Vec<String>) -> Result<()> {
    let text = fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let display_path = relative(root, file);
    let mut in_fence = false;
    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        check_repo_file_components(root, &display_path, line_no, line, failures);
        check_github_repo_urls(&display_path, line_no, line, failures);
        check_inline_repo_paths(root, &display_path, line_no, line, failures);
    }
    Ok(())
}

fn check_repo_file_components(
    root: &Path,
    display_path: &str,
    line_no: usize,
    line: &str,
    failures: &mut Vec<String>,
) {
    let mut rest = line;
    while let Some(start) = rest.find("<RepoFile") {
        rest = &rest[start + "<RepoFile".len()..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        if let Some(path) = tag_attr(tag, "path")
            && !existing_repo_file(root, &path)
        {
            failures.push(format!(
                "{display_path}:{line_no}: RepoFile path does not exist in the repository: {path}"
            ));
        }
        rest = &rest[end + 1..];
    }
}

fn tag_attr(tag: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{name}={quote}");
        if let Some(start) = tag.find(&needle) {
            let value_start = start + needle.len();
            let value = &tag[value_start..];
            let end = value.find(quote)?;
            return Some(value[..end].to_owned());
        }
    }
    None
}

fn check_github_repo_urls(
    display_path: &str,
    line_no: usize,
    line: &str,
    failures: &mut Vec<String>,
) {
    for path in prefixed_url_paths(line, GITHUB_BLOB_PREFIX) {
        failures.push(format!(
            "{display_path}:{line_no}: use <RepoFile path=\"{path}\" /> instead of a full GitHub blob URL"
        ));
    }
    for url in prefixed_urls(line, GITHUB_TREE_PREFIX) {
        failures.push(format!(
            "{display_path}:{line_no}: use a blob/main file link instead of tree/main so CI can verify it: {url}"
        ));
    }
}

fn prefixed_url_paths(line: &str, prefix: &str) -> Vec<String> {
    prefixed_urls(line, prefix)
        .into_iter()
        .map(|url| url[prefix.len()..].to_owned())
        .collect()
}

fn prefixed_urls(line: &str, prefix: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find(prefix) {
        let candidate = &rest[start..];
        let end = candidate
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '>' | '"' | '\''))
            .unwrap_or(candidate.len());
        urls.push(candidate[..end].to_owned());
        rest = &candidate[end..];
    }
    urls
}

fn check_inline_repo_paths(
    root: &Path,
    display_path: &str,
    line_no: usize,
    line: &str,
    failures: &mut Vec<String>,
) {
    let mut offset = 0;
    while let Some(open_rel) = line[offset..].find('`') {
        let open = offset + open_rel;
        let value_start = open + 1;
        let Some(close_rel) = line[value_start..].find('`') else {
            break;
        };
        let close = value_start + close_rel;
        let value = &line[value_start..close];
        if !is_markdown_link_text(line, open, close + 1 - open)
            && let Some(path) = repo_path_candidate(value)
            && existing_repo_file(root, path)
        {
            failures.push(format!(
                "{display_path}:{line_no}: link existing repo file `{path}` with <RepoFile path=\"{path}\" />"
            ));
        }
        offset = close + 1;
    }
}

fn is_markdown_link_text(line: &str, match_start: usize, match_len: usize) -> bool {
    let before = match_start
        .checked_sub(1)
        .and_then(|idx| line.as_bytes().get(idx))
        .copied();
    let after = line.as_bytes().get(match_start + match_len..);
    before == Some(b'[') && after.is_some_and(|s| s.starts_with(b"]("))
}

fn repo_path_candidate(value: &str) -> Option<&str> {
    let path = value.trim();
    if path.is_empty()
        || path
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, ',' | '*'))
    {
        return None;
    }
    if REPO_FILE_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || REPO_TOP_LEVEL_FILES.contains(&path)
    {
        return Some(path);
    }
    None
}

fn existing_repo_file(root: &Path, path: &str) -> bool {
    let relative = Path::new(path.trim());
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return false;
    }
    root.join(relative).is_file()
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn report_repo_links_clean() {
    #[expect(
        clippy::print_stdout,
        reason = "jackin-xtask is a CLI; the audit result is its output"
    )]
    {
        println!("repo links OK - repository file references are verifiable.");
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
mod tests;
