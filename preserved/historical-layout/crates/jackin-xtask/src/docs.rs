//! Documentation-tree automation: scaffold and validate Fumadocs `meta.json`
//! sidebars for roadmap items and research dossiers.
//!
//! The docs site is Fumadocs; each directory under `docs/content/` carries
//! a `meta.json` whose `pages` array orders the sidebar. These tasks keep that
//! wiring correct without hand-editing JSON:
//!
//! ```sh
//! cargo xtask change new <slug> --group <group>   # scaffold a roadmap item
//! cargo xtask docs repo-links                     # validate repo-file links
//! cargo xtask docs brand                          # brand-prose lint (RULES.md)
//! cargo xtask docs specs                          # spec↔test citation gate
//! cargo xtask docs map-check                      # workspace crates named in Codebase Map
//! cargo xtask research scaffold <slug> --group <domain> # scaffold a research dossier
//! cargo xtask research check                      # validate research meta.json
//! cargo xtask roadmap audit                       # validate roadmap meta.json
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use crate::report::{self, FormatArgs};

mod brand;
pub(crate) mod contract;
mod lychee_cache;
mod site_links;
mod specs;

const DOCS_ROOT: &str = "docs/content";
const DOCS_MARKDOWN_ROOT: &str = "docs";
const ROADMAP_REL: &str = "roadmap";
const RESEARCH_REL: &str = "research";
const REPO_FILE_PREFIXES: &[&str] = &[
    "crates/", "src/", "docs/", "docker/", ".github/", "scripts/",
];
const REPO_LINK_ROOT_DOCS: &[&str] = &[
    "AGENTS.md",
    "ENGINEERING.md",
    "PROJECT_STRUCTURE.md",
    "PULL_REQUESTS.md",
    "README.md",
    "RULES.md",
    "TESTING.md",
    "TODO.md",
];
const REPO_TOP_LEVEL_FILES: &[&str] = &[
    "AGENTS.md",
    "BRANCHING.md",
    "Cargo.lock",
    "Cargo.toml",
    "COMMITS.md",
    "ENGINEERING.md",
    "PROJECT_STRUCTURE.md",
    "PULL_REQUESTS.md",
    "README.md",
    "TESTING.md",
    "TODO.md",
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
    RepoLinks(DocsGateArgs),
    /// Reject forbidden brand spellings (`jackin'`, `Jackin`, `Jackin'`) in prose.
    Brand(DocsGateArgs),
    /// Verify every behavioral-spec INV row cites an existing test (or MISSING).
    Specs(DocsGateArgs),
    /// Every workspace member crate name appears in the Codebase Map MDX.
    MapCheck(DocsGateArgs),
    /// Print a stable CI cache contract for a documentation surface.
    Contract(contract::DocsContractArgs),
    /// Resolve and publish the reusable docs-link result for GitHub Actions.
    #[command(hide = true)]
    CiLinkResult,
    /// Restore the durable lychee response cache for GitHub Actions.
    #[command(hide = true)]
    CiLycheeCache,
    /// Validate links in the generated documentation site.
    SiteLinks,
}

#[derive(Args, Clone, Copy)]
pub(crate) struct DocsGateArgs {
    #[command(flatten)]
    output: FormatArgs,
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
    /// Validate research sidebars, frontmatter, state labels, and page-title structure.
    Check(DocsGateArgs),
}

#[derive(Args)]
pub(crate) struct ResearchScaffoldArgs {
    /// Kebab-case slug; becomes the dossier directory name under `--group`.
    slug: String,
    /// Top-level research domain: `agents`, `platform`, `product`, `engineering`, or `context`.
    #[arg(long)]
    group: String,
    /// Dossier title. Defaults to a title-cased form of the slug.
    #[arg(long)]
    title: Option<String>,
}

#[derive(Subcommand)]
pub(crate) enum RoadmapCommand {
    /// Validate that every roadmap `meta.json` page resolves and no item `.mdx`
    /// is orphaned.
    Audit(DocsGateArgs),
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
        DocsCommand::RepoLinks(args) => run_docs_gate(
            args,
            "repo-links",
            "docs/",
            "replace unverifiable repository paths with checked link components",
            "cargo xtask docs repo-links",
            check_repo_links,
        ),
        DocsCommand::Brand(args) => run_docs_gate(
            args,
            "brand",
            ".",
            "replace forbidden brand spellings with jackin❯ in rich text",
            "cargo xtask docs brand",
            brand::check_brand,
        ),
        DocsCommand::Specs(args) => run_docs_gate(
            args,
            "specs",
            "docs/content/contributing/behavioral-specs.mdx",
            "cite an existing test for every behavioral invariant",
            "cargo xtask docs specs",
            specs::check_specs,
        ),
        DocsCommand::MapCheck(args) => run_docs_gate(
            args,
            "map-check",
            "docs/content/reference/getting-oriented/codebase-map.mdx",
            "synchronize workspace crate names with the codebase map",
            "cargo xtask docs map-check",
            check_codebase_map,
        ),
        DocsCommand::Contract(args) => contract::run(args),
        DocsCommand::CiLinkResult => contract::run_ci_link_result(),
        DocsCommand::CiLycheeCache => lychee_cache::run(),
        DocsCommand::SiteLinks => site_links::run(),
    }
}

fn run_docs_gate(
    args: DocsGateArgs,
    gate: &'static str,
    file: &'static str,
    fix: &'static str,
    rerun: &'static str,
    check: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    report::run_gate(args.output.resolved(), gate, file, fix, rerun, || {
        check(&repo_root()?)
    })
}

/// Recurring map↔workspace gate (R-map-metadata-gate): every `cargo metadata`
/// workspace member package name must appear as a token in the Codebase Map.
fn check_codebase_map(root: &Path) -> Result<()> {
    let map_rel = "docs/content/reference/getting-oriented/codebase-map.mdx";
    let map_path = root.join(map_rel);
    let map = fs::read_to_string(&map_path)
        .with_context(|| format!("reading codebase map at {}", map_path.display()))?;

    let members = workspace_package_names(root)?;
    let tiers: BTreeMap<&str, u8> = crate::arch::TIERS.iter().copied().collect();

    check_codebase_map_text(&map, &members, &tiers, map_rel)
}

fn check_codebase_map_text(
    map: &str,
    members: &[String],
    tiers: &BTreeMap<&str, u8>,
    map_rel: &str,
) -> Result<()> {
    let member_set: BTreeSet<&str> = members.iter().map(String::as_str).collect();

    let mut missing: Vec<String> = Vec::new();
    for name in members {
        let needle = name.as_str();
        let present = map
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .any(|tok| tok == needle);
        if !present {
            missing.push(name.clone());
        }
    }

    // Two-way: jackin-* tokens in the map that are not workspace members.
    let mut stale: Vec<String> = Vec::new();
    for tok in map.split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_') {
        if !tok.starts_with("jackin") {
            continue;
        }
        if tok == "jackin" || tok == "jackin❯" {
            continue;
        }
        // crate-shaped: jackin-foo or jackin_foo
        if !(tok.contains('-') || tok.contains('_')) {
            continue;
        }
        if !member_set.contains(tok) {
            stale.push(tok.to_owned());
        }
    }
    stale.sort();
    stale.dedup();

    let mut problems = Vec::new();
    if !missing.is_empty() {
        missing.sort();
        problems.push(format!(
            "{} workspace crate(s) missing from {map_rel}:\n  {}",
            missing.len(),
            missing.join("\n  ")
        ));
    }
    if !stale.is_empty() {
        problems.push(format!(
            "{} map token(s) are not workspace members (stale after rename/delete):\n  {}",
            stale.len(),
            stale.join("\n  ")
        ));
    }
    for name in members {
        let row = map.lines().find(|line| {
            line.starts_with('|')
                && line
                    .split('|')
                    .nth(1)
                    .is_some_and(|cell| cell.contains(&format!("](/reference/crates/{name}/)")))
        });
        let Some(row) = row else {
            problems.push(format!(
                "{map_rel}: missing inventory row and crate-page link for {name}"
            ));
            continue;
        };
        let expected_tier = tiers.get(name.as_str()).copied();
        match expected_tier {
            Some(tier)
                if row
                    .split('|')
                    .nth(2)
                    .is_some_and(|cell| cell.trim() == tier.to_string()) => {}
            Some(tier) => problems.push(format!(
                "{map_rel}: {name} inventory row is missing architecture tier {tier}"
            )),
            None => problems.push(format!(
                "{map_rel}: {name} has no tier in the executable architecture inventory"
            )),
        }
        let readme_path = format!("path=\"crates/{name}/README.md\"");
        if !row.contains(&readme_path) {
            problems.push(format!(
                "{map_rel}: {name} inventory row is missing README link {readme_path}"
            ));
        }
    }
    if problems.is_empty() {
        emit(&format!(
            "docs map-check OK — {} workspace crate(s); two-way map tokens clean in {map_rel}",
            members.len()
        ));
        return Ok(());
    }
    bail!(
        "{} map-check problem(s):\n{}\n\nre-run: cargo xtask docs map-check",
        problems.len(),
        problems.join("\n")
    );
}

fn workspace_package_names(root: &Path) -> Result<Vec<String>> {
    let mut meta = crate::cmd::command("cargo");
    meta.args([
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
        "--manifest-path",
    ])
    .arg(root.join("Cargo.toml"));
    let output =
        crate::cmd::output_raw(&mut meta).context("running cargo metadata for docs map-check")?;
    if !output.success {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let v: Value = serde_json::from_slice(&output.stdout).context("parsing cargo metadata JSON")?;
    let workspace_root = v
        .get("workspace_root")
        .and_then(|x| x.as_str())
        .unwrap_or_default();
    let packages = v
        .get("packages")
        .and_then(|x| x.as_array())
        .context("cargo metadata missing packages")?;
    let mut names = Vec::new();
    for pkg in packages {
        let manifest = pkg
            .get("manifest_path")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        // Workspace members only (manifest under workspace root).
        if !manifest.starts_with(workspace_root) {
            continue;
        }
        if let Some(name) = pkg.get("name").and_then(|x| x.as_str()) {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

pub(crate) fn run_research(command: ResearchCommand) -> Result<()> {
    match command {
        ResearchCommand::Scaffold(args) => research_scaffold(args),
        ResearchCommand::Check(args) => report::run_gate(
            args.output.resolved(),
            "research",
            "docs/content/research/",
            "repair research sidebars or normalize page metadata and structure",
            "cargo xtask research check",
            || validate_tree(&research_dir()?, "research"),
        ),
    }
}

pub(crate) fn run_roadmap(command: RoadmapCommand) -> Result<()> {
    match command {
        RoadmapCommand::Audit(args) => report::run_gate(
            args.output.resolved(),
            "roadmap",
            "docs/content/roadmap/",
            "repair meta.json page entries and orphaned roadmap pages",
            "cargo xtask roadmap audit",
            || validate_tree(&roadmap_dir()?, "roadmap"),
        ),
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
/// contains `docs/content`).
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
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("meta.json");
    let staged = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    fs::write(&staged, text).with_context(|| format!("writing {}", staged.display()))?;
    fs::rename(&staged, path).with_context(|| format!("replacing {}", path.display()))
}

/// Append `entry` to a `meta.json`'s `pages` array if not already present.
#[expect(
    clippy::disallowed_methods,
    reason = "synchronous CLI locks the research tree before atomically updating sidebar metadata"
)]
fn append_page(meta_path: &Path, tree_lock_path: &Path, entry: &str) -> Result<()> {
    let tree_lock = fs::File::open(tree_lock_path)
        .with_context(|| format!("opening {}", tree_lock_path.display()))?;
    fs4::FileExt::lock(&tree_lock)
        .with_context(|| format!("locking {}", tree_lock_path.display()))?;

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
        "---\ntitle: \"{title}\"\ndescription: \"<!-- One-sentence unfinished outcome. -->\"\n---\n\n\
         **Status**: Open\n\n\
         **Outcome**: <!-- What must become true? -->\n\n\
         ## Current state\n\n<!-- What exists today? Keep research evidence in /research. -->\n\n\
         ## Remaining work\n\n<!-- Concrete implementation scope. -->\n\n\
         ## Completion gate\n\n<!-- Observable evidence required before retirement. -->\n\n\
         ## Related research\n\n<!-- Site links under /research, when applicable. -->\n"
    );
    fs::write(&item_path, body).with_context(|| format!("writing {}", item_path.display()))?;

    // Group meta references siblings in the parent roadmap dir as `../<slug>`.
    append_page(
        &group_meta,
        &roadmap.join("meta.json"),
        &format!("../{}", args.slug),
    )?;

    report_created(&[item_path.as_path(), group_meta.as_path()]);
    Ok(())
}

fn research_scaffold(args: ResearchScaffoldArgs) -> Result<()> {
    let root = repo_root()?;
    research_scaffold_in(
        &root.join(DOCS_ROOT).join(RESEARCH_REL),
        &root.join("prompts/research"),
        args,
    )
}

struct ResearchScaffoldRollback {
    dossier: PathBuf,
    prompt: PathBuf,
    prompt_owned: bool,
    committed: bool,
}

impl Drop for ResearchScaffoldRollback {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if self.prompt_owned {
            drop(fs::remove_file(&self.prompt));
        }
        drop(fs::remove_dir_all(&self.dossier));
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "jackin-xtask is a synchronous CLI; exclusive creation prevents scaffold races"
)]
fn research_scaffold_in(research: &Path, prompts: &Path, args: ResearchScaffoldArgs) -> Result<()> {
    validate_slug(&args.slug)?;
    let title = args.title.unwrap_or_else(|| title_from_slug(&args.slug));
    if title.contains(['\n', '\r']) {
        bail!("research title must be a single line");
    }
    let title_yaml = serde_json::to_string(&title).context("serializing research title")?;

    let group = args.group.trim_matches(['(', ')'].as_ref());
    if !matches!(
        group,
        "agents" | "platform" | "product" | "engineering" | "context"
    ) {
        bail!(
            "research group `{group}` is invalid — choose agents, platform, product, engineering, or context"
        );
    }
    let group_dir = research.join(group);
    let group_meta = group_dir.join("meta.json");
    if !group_meta.is_file() {
        bail!(
            "research group `{group}` not found at {} — choose agents, platform, product, engineering, or context",
            group_dir.display()
        );
    }
    let parent = read_meta(&group_meta)?;
    parent
        .get("pages")
        .and_then(Value::as_array)
        .with_context(|| format!("`pages` is not an array in {}", group_meta.display()))?;

    let dossier = group_dir.join(&args.slug);
    let prompt_dir = prompts.join(group);
    let prompt = prompt_dir.join(format!("{}.md", args.slug));
    fs::create_dir_all(&prompt_dir)
        .with_context(|| format!("creating {}", prompt_dir.display()))?;
    match fs::create_dir(&dossier) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            bail!("research dossier already exists: {}", dossier.display());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("creating {}", dossier.display()));
        }
    }
    let mut rollback = ResearchScaffoldRollback {
        dossier: dossier.clone(),
        prompt: prompt.clone(),
        prompt_owned: false,
        committed: false,
    };

    let index = dossier.join("index.mdx");
    fs::write(
        &index,
        format!(
            "---\ntitle: {title_yaml}\ndescription: \"<!-- One-sentence research question. -->\"\n---\n\n\
             **Research state:** Incomplete\n\n\
             **Research brief:** <RepoFile path=\"prompts/research/{}/{}.md\" />\n\n\
             ## Research question\n\n<!-- What must this dossier establish? -->\n\n\
             ## Headline findings\n\n<!-- Key findings, each with a source. -->\n\n\
             ## Method and evidence\n\n<!-- Sources, measurements, dates, and confidence limits. -->\n\n\
             ## Limitations and open questions\n\n<!-- What remains uncertain? -->\n\n\
             ## How to read\n\n<!-- Chapter map and recommended order. -->\n\n\
             ## Related work\n\n<!-- Link research, reference, and roadmap pages without scheduling work here. -->\n",
            group,
            args.slug
        ),
    )
    .with_context(|| format!("writing {}", index.display()))?;

    let prompt_result = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&prompt);
    let mut prompt_file = match prompt_result {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            bail!("research brief already exists: {}", prompt.display());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("creating {}", prompt.display()));
        }
    };
    rollback.prompt_owned = true;
    prompt_file
        .write_all(
            format!(
            "# {title} research brief\n\n## Mission\n\n<!-- What to establish and why. -->\n\n\
             ## Scope\n\n<!-- In scope, out of scope, and evidence cutoff. -->\n\n\
             ## Evidence requirements\n\n<!-- Primary sources, local measurements, confidence labels, and freshness rules. -->\n\n\
             ## Required output\n\n<!-- Chapter list and completion criteria. -->\n\n\
             ## Run\n\n`/goal Follow prompts/research/{}/{}.md`\n",
            group,
            args.slug
        )
            .as_bytes(),
        )
        .with_context(|| format!("writing {}", prompt.display()))?;

    let meta = dossier.join("meta.json");
    write_meta(
        &meta,
        &json!({ "title": title, "defaultOpen": false, "pages": ["index"] }),
    )?;

    // Register the dossier in its domain sidebar.
    append_page(&group_meta, &research.join("meta.json"), &args.slug)?;
    rollback.committed = true;

    report_created(&[
        index.as_path(),
        prompt.as_path(),
        meta.as_path(),
        group_meta.as_path(),
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

    let files = collect_repo_link_files(root, content_root)?;

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

fn collect_repo_link_files(root: &Path, content_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_mdx_files(content_root, &mut files)?;
    collect_markdown_files(&root.join(DOCS_MARKDOWN_ROOT), &mut files)?;
    for doc in REPO_LINK_ROOT_DOCS {
        let path = root.join(doc);
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn check_repo_links_file(root: &Path, file: &Path, failures: &mut Vec<String>) -> Result<()> {
    let text = fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let display_path = relative(root, file);
    let is_fumadocs_content = file.starts_with(root.join(DOCS_ROOT));
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
        check_repo_file_components(
            root,
            &display_path,
            is_fumadocs_content,
            line_no,
            line,
            failures,
        );
        check_github_repo_urls(&display_path, line_no, line, failures);
        check_inline_repo_paths(
            root,
            &display_path,
            is_fumadocs_content,
            line_no,
            line,
            failures,
        );
    }
    Ok(())
}

fn check_repo_file_components(
    root: &Path,
    display_path: &str,
    is_fumadocs_content: bool,
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
        if !is_fumadocs_content {
            failures.push(format!(
                "{display_path}:{line_no}: <RepoFile> is only allowed under {DOCS_ROOT}; use a Markdown link in this file"
            ));
            rest = &rest[end + 1..];
            continue;
        }
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
    is_fumadocs_content: bool,
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
            let guidance = if is_fumadocs_content {
                format!("link existing repo file `{path}` with <RepoFile path=\"{path}\" />")
            } else {
                format!("link existing repo file `{path}` with a Markdown link")
            };
            failures.push(format!("{display_path}:{line_no}: {guidance}"));
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
        if report::human_output() {
            println!("repo links OK - repository file references are verifiable.");
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
    if report::human_output() {
        println!("{line}");
    }
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

/// Find the `meta.json` entry that registers a roadmap item.
fn find_group_registration(
    roadmap: &Path,
    item: &Path,
    slug: &str,
) -> Result<Option<(PathBuf, String)>> {
    let mut metas = Vec::new();
    collect_meta_files(roadmap, &mut metas)?;
    for meta in metas {
        let colocated = meta.parent() == item.parent();
        let entries = if colocated {
            [slug.to_owned(), format!("../{slug}")]
        } else {
            [format!("../{slug}"), slug.to_owned()]
        };
        let value = read_meta(&meta)?;
        if let Some(entry) = entries.into_iter().find(|entry| {
            value
                .get("pages")
                .and_then(Value::as_array)
                .is_some_and(|pages| pages.iter().any(|p| p.as_str() == Some(entry.as_str())))
        }) {
            return Ok(Some((meta, entry)));
        }
    }
    Ok(None)
}

fn find_roadmap_item(roadmap: &Path, slug: &str) -> Result<PathBuf> {
    let filename = format!("{slug}.mdx");
    let mut files = Vec::new();
    collect_text_files(roadmap, &mut files)?;
    let matches = files
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name == filename.as_str())
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [item] => Ok(item.clone()),
        [] => bail!(
            "no roadmap item named `{filename}` under {}",
            roadmap.display()
        ),
        _ => bail!(
            "multiple roadmap items named `{filename}` under {}; slugs must be unique",
            roadmap.display()
        ),
    }
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
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
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
    let item = find_roadmap_item(&roadmap, &args.slug)?;

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
    let group = find_group_registration(roadmap, item, slug)?;
    let links = inbound_links(docs_root, slug, item)?;

    emit(&format!("Retirement plan for `{slug}` (read-only)\n"));
    emit("1. Move the page content below into canonical docs (operator detail →");
    emit("   guides/commands, design rationale → reference or research); remove it");
    emit("   from the active roadmap status view; repoint the inbound links below.");
    emit("2. Then run: cargo xtask roadmap retire <slug> --apply\n");
    match group {
        Some((meta, entry)) => emit(&format!(
            "Sidebar entry to drop: `{entry}` in {}",
            meta.display()
        )),
        None => emit(&format!(
            "WARNING: `{slug}` is not registered in any roadmap group sidebar"
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
    let (meta, entry) = find_group_registration(roadmap, item, slug)?
        .with_context(|| format!("`{slug}` is not registered in any roadmap group sidebar"))?;

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

    remove_page(&meta, &entry)?;
    fs::remove_file(item).with_context(|| format!("deleting {}", item.display()))?;
    validate_tree(roadmap, "roadmap")?;
    emit(&format!(
        "Retired `{slug}`: removed `{entry}` from {}, deleted the page, sidebar audit clean, no dangling links.",
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
        if label == "research" && pages.first().and_then(Value::as_str) != Some("index") {
            problems.push(format!(
                "{}: research navigation must list `index` first",
                meta_path.display()
            ));
        }
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
    for mdx in &mdx_files {
        let canonical = fs::canonicalize(mdx).unwrap_or(mdx.clone());
        if !referenced.contains(&canonical) {
            problems.push(format!(
                "{}: not referenced by any meta.json (orphaned sidebar page)",
                mdx.display()
            ));
        }
    }
    if label == "research" {
        validate_research_pages(root, &mdx_files, &mut problems)?;
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

/// Enforce the reader-facing contract that makes the research corpus scan as
/// one collection instead of a pile of unrelated Markdown shapes.
fn validate_research_pages(
    research_root: &Path,
    files: &[PathBuf],
    problems: &mut Vec<String>,
) -> Result<()> {
    for path in files {
        let stem = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(stem.as_str(), "prompt" | "brief")
            || stem.ends_with("-prompt")
            || stem.ends_with("-brief")
        {
            problems.push(format!(
                "{}: research briefs belong under prompts/research/, not published docs",
                path.display()
            ));
        }

        let text = fs::read_to_string(path)
            .with_context(|| format!("reading research page {}", path.display()))?;
        let Some(rest) = text.strip_prefix("---\n") else {
            problems.push(format!("{}: missing YAML frontmatter", path.display()));
            continue;
        };
        let Some((frontmatter, body)) = rest.split_once("\n---\n") else {
            problems.push(format!("{}: unterminated YAML frontmatter", path.display()));
            continue;
        };

        for field in ["title", "description"] {
            let prefix = format!("{field}:");
            let value = frontmatter
                .lines()
                .find_map(|line| line.strip_prefix(&prefix))
                .map(str::trim)
                .unwrap_or_default();
            if value.is_empty() || value.contains("<!--") {
                problems.push(format!(
                    "{}: frontmatter `{field}` must be present and non-placeholder",
                    path.display()
                ));
            }
            if field == "description" && !value.is_empty() {
                let description = value.trim_matches(['"', '\'']);
                if description.chars().count() > 160
                    || description.contains('…')
                    || description.contains("...")
                    || description.starts_with("Evidence and analysis for ")
                    || !description.ends_with(['.', '?', '!'])
                {
                    problems.push(format!(
                        "{}: frontmatter `description` must be an informative sentence of at most 160 characters",
                        path.display()
                    ));
                }
            }
        }

        if body.contains("**Research state**:") {
            problems.push(format!(
                "{}: use canonical `**Research state:**` punctuation",
                path.display()
            ));
        }
        for line in body
            .lines()
            .filter(|line| line.starts_with("**Research state:**"))
        {
            let value = line.trim_start_matches("**Research state:**").trim();
            if !matches!(
                value,
                "Current" | "Needs refresh" | "Incomplete" | "Reference"
            ) {
                problems.push(format!(
                    "{}: research state must be Current, Needs refresh, Incomplete, or Reference",
                    path.display()
                ));
            }
        }

        let mut in_fence = false;
        for (line_index, line) in body.lines().enumerate() {
            if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                in_fence = !in_fence;
                continue;
            }
            if !in_fence && line.starts_with("# ") {
                problems.push(format!(
                    "{}:{}: remove the explicit H1; frontmatter renders the page title",
                    path.display(),
                    line_index + frontmatter.lines().count() + 3
                ));
            }
            if in_fence {
                continue;
            }
            let source_line = line_index + frontmatter.lines().count() + 3;
            for href in quoted_attribute_values(line, "href") {
                validate_research_href(
                    research_root,
                    path,
                    source_line,
                    href,
                    "Card target",
                    problems,
                );
            }
            for href in markdown_link_targets(line) {
                validate_research_href(
                    research_root,
                    path,
                    source_line,
                    href,
                    "Markdown link",
                    problems,
                );
            }
        }
        if body.lines().count() > 400 {
            problems.push(format!(
                "{}: research page has more than 400 body lines; split independent reader questions into chapters",
                path.display()
            ));
        }
    }
    Ok(())
}

fn quoted_attribute_values<'a>(line: &'a str, attribute: &str) -> Vec<&'a str> {
    let marker = format!("{attribute}=\"");
    let mut values = Vec::new();
    let mut rest = line;
    while let Some((_, after)) = rest.split_once(&marker) {
        let Some((value, tail)) = after.split_once('"') else {
            break;
        };
        values.push(value);
        rest = tail;
    }
    values
}

fn markdown_link_targets(line: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut rest = line;
    while let Some((_, after)) = rest.split_once("](") {
        let Some((target, tail)) = after.split_once(')') else {
            break;
        };
        targets.push(target.split_whitespace().next().unwrap_or(target));
        rest = tail;
    }
    targets
}

fn validate_research_href(
    research_root: &Path,
    source: &Path,
    line: usize,
    href: &str,
    kind: &str,
    problems: &mut Vec<String>,
) {
    if href.starts_with("http://")
        || href.starts_with("https://")
        || href.starts_with("mailto:")
        || href.starts_with('#')
    {
        return;
    }
    if let Some(route) = href.strip_prefix("/research/") {
        let route = route
            .split('#')
            .next()
            .unwrap_or(route)
            .trim_end_matches('/');
        let page = research_root.join(format!("{route}.mdx"));
        let index = research_root.join(route).join("index.mdx");
        if !page.is_file() && !index.is_file() {
            problems.push(format!(
                "{}:{line}: research {kind} `{href}` does not resolve",
                source.display()
            ));
        }
        return;
    }
    if href.starts_with('/') {
        return;
    }

    let path_part = href.split('#').next().unwrap_or(href);
    let is_asset = matches!(
        Path::new(path_part)
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg")
    );
    if is_asset {
        let asset = source.parent().unwrap_or(research_root).join(path_part);
        if !asset.is_file() {
            problems.push(format!(
                "{}:{line}: research asset `{href}` does not resolve",
                source.display()
            ));
        }
    } else {
        problems.push(format!(
            "{}:{line}: published documentation links must be site-absolute; found `{href}`",
            source.display()
        ));
    }
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
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        if path.is_dir() {
            collect_meta_files(&path, out)?;
        }
    }
    Ok(())
}

/// Recursively collect every `.mdx` file under `root`.
fn collect_mdx_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        if path.is_dir() {
            collect_mdx_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "mdx") {
            out.push(path);
        }
    }
    Ok(())
}

/// Recursively collect every Markdown source file under `root`.
fn collect_markdown_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(skip_docs_vendor_dir) {
                continue;
            }
            collect_markdown_files(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext == "md" || ext == "mdx")
        {
            out.push(path);
        }
    }
    Ok(())
}

fn skip_docs_vendor_dir(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some("node_modules" | ".output" | ".tanstack" | ".astro")
    )
}

fn report_clean(label: &str, meta_count: usize) {
    #[expect(
        clippy::print_stdout,
        reason = "jackin-xtask is a CLI; the audit result is its output"
    )]
    {
        if report::human_output() {
            println!("{label} sidebar OK — {meta_count} meta.json file(s), all pages resolve.");
        }
    }
}

#[cfg(test)]
mod tests;
