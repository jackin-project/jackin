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
    }
}

// ---------------------------------------------------------------------------
// Locating the docs tree
// ---------------------------------------------------------------------------

/// Walk up from the current directory to the repo root (the directory that
/// contains `docs/content/docs`).
fn repo_root() -> Result<PathBuf> {
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
    validate_slug(&args.slug)?;
    let roadmap = roadmap_dir()?;
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
    validate_slug(&args.slug)?;
    let research = research_dir()?;
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
}
