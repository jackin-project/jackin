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
    validate_slug("agent-codenames").unwrap();
    validate_slug("a1-b2").unwrap();
    validate_slug("Bad-Slug").unwrap_err();
    validate_slug("-leading").unwrap_err();
    validate_slug("double--hyphen").unwrap_err();
    validate_slug("").unwrap_err();
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

fn repo_link_fixture(page_body: &str) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    write(
        &root.join("crates/jackin-host/src/host_desktop.rs"),
        "pub fn open() {}\n",
    );
    write(&root.join("Cargo.toml"), "[workspace]\n");
    write(&root.join("docs/content/docs/guide.mdx"), page_body);
    repo
}

#[test]
fn repo_links_scan_root_and_docs_markdown_files() {
    let repo = repo_link_fixture("---\ntitle: Guide\n---\n");
    let root = repo.path();
    write(
        &root.join("docs/AGENTS.md"),
        "See `crates/jackin-host/src/host_desktop.rs`.\n",
    );
    write(
        &root.join("PROJECT_STRUCTURE.md"),
        "See `crates/jackin-host/src/host_desktop.rs`.\n",
    );
    write(
        &root.join("TODO.md"),
        "See `crates/jackin-host/src/host_desktop.rs`.\n",
    );

    let err = check_repo_links_in(root, &root.join(DOCS_ROOT))
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("docs/AGENTS.md")
            && err.contains("PROJECT_STRUCTURE.md")
            && err.contains("TODO.md"),
        "should include docs markdown outside content/docs: {err}"
    );
}

#[test]
fn repo_links_flag_restored_top_level_governance_files() {
    let repo = repo_link_fixture("---\ntitle: Guide\n---\n");
    let root = repo.path();
    write(&root.join("BRANCHING.md"), "# Branching\n");
    write(&root.join("COMMITS.md"), "# Commits\n");
    write(
        &root.join("README.md"),
        "See `BRANCHING.md` and `COMMITS.md`.\n",
    );

    let err = check_repo_links_in(root, &root.join(DOCS_ROOT))
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("BRANCHING.md") && err.contains("COMMITS.md"),
        "should flag inline top-level governance paths: {err}"
    );
}

#[test]
fn repo_links_reject_repo_file_component_outside_fumadocs_content() {
    let repo = repo_link_fixture("---\ntitle: Guide\n---\n");
    let root = repo.path();
    write(
        &root.join("docs/AGENTS.md"),
        "See <RepoFile path=\"crates/jackin-host/src/host_desktop.rs\" />.\n",
    );

    let err = check_repo_links_in(root, &root.join(DOCS_ROOT))
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("<RepoFile> is only allowed under docs/content/docs"),
        "should reject Fumadocs-only component outside Fumadocs content: {err}"
    );
}

#[test]
fn repo_links_ignore_docs_local_paths_that_are_not_repo_files() {
    let repo = repo_link_fixture(
        "---\ntitle: Guide\n---\n\nSee `crates/jackin-host/src/host_desktop.rs`.\n",
    );
    write(
        &repo.path().join("docs/AGENTS.md"),
        "Docs source lives in `src/lib/source.ts`.\n",
    );

    let err = check_repo_links_in(repo.path(), &repo.path().join(DOCS_ROOT))
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("docs/content/docs/guide.mdx") && !err.contains("docs/AGENTS.md"),
        "should not treat docs-local paths as repo-root files: {err}"
    );
}

#[test]
fn repo_links_reject_inline_code_repo_paths() {
    let repo = repo_link_fixture(
        "---\ntitle: Guide\n---\n\nSee `crates/jackin-host/src/host_desktop.rs`.\n",
    );

    let err = check_repo_links_in(repo.path(), &repo.path().join(DOCS_ROOT))
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("link existing repo file")
            && err.contains("crates/jackin-host/src/host_desktop.rs"),
        "should flag raw inline repo path: {err}"
    );
}

#[test]
fn repo_links_accept_repo_file_component_and_markdown_link_text() {
    let repo = repo_link_fixture(
        "---\ntitle: Guide\n---\n\n\
         See <RepoFile path=\"crates/jackin-host/src/host_desktop.rs\">crates/jackin-host/src/host_desktop.rs</RepoFile>.\n\
         [`Cargo.toml`](../../Cargo.toml) is regular Markdown.\n",
    );

    check_repo_links_in(repo.path(), &repo.path().join(DOCS_ROOT)).unwrap();
}

#[test]
fn repo_links_reject_missing_repo_file_component_path() {
    let repo = repo_link_fixture(
        "---\ntitle: Guide\n---\n\n\
         See <RepoFile path=\"crates/jackin-host/src/missing.rs\" />.\n",
    );

    let err = check_repo_links_in(repo.path(), &repo.path().join(DOCS_ROOT))
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("RepoFile path does not exist")
            && err.contains("crates/jackin-host/src/missing.rs"),
        "should flag missing RepoFile path: {err}"
    );
}

#[test]
fn repo_links_reject_repo_file_component_traversal() {
    let repo = repo_link_fixture(
        "---\ntitle: Guide\n---\n\n\
         See <RepoFile path=\"docs/content/docs/../../Cargo.toml\" />.\n",
    );

    let err = check_repo_links_in(repo.path(), &repo.path().join(DOCS_ROOT))
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("RepoFile path does not exist")
            && err.contains("docs/content/docs/../../Cargo.toml"),
        "should reject non-normal repo paths: {err}"
    );
}

#[test]
fn repo_links_ignore_code_fences() {
    let repo = repo_link_fixture(
        "---\ntitle: Guide\n---\n\n\
         ```text\n\
         crates/jackin-host/src/host_desktop.rs\n\
         ```\n",
    );

    check_repo_links_in(repo.path(), &repo.path().join(DOCS_ROOT)).unwrap();
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
    assert!(line_references_slug("see /roadmap/auth/ for", "auth"));
    assert!(line_references_slug("    \"../auth\"", "auth"));
    assert!(!line_references_slug("/roadmap/auth-health/", "auth"));
    assert!(!line_references_slug("nothing here", "auth"));
}

/// Build a `docs/content/docs` shape with one roadmap item registered in a
/// group, plus optional extra files. Returns the docs-root temp dir.
fn roadmap_fixture(extra: &[(&str, &str)]) -> tempfile::TempDir {
    let docs = tempfile::tempdir().unwrap();
    let d = docs.path();
    write_meta_mk(
        &d.join("roadmap/(grp)/meta.json"),
        &json!({ "pages": ["../shipme"] }),
    );
    write(
        &d.join("roadmap/shipme.mdx"),
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
    assert!(!d.join("roadmap/shipme.mdx").exists(), "page deleted");
    let meta = read_meta(&d.join("roadmap/(grp)/meta.json")).unwrap();
    assert!(
        meta["pages"].as_array().unwrap().is_empty(),
        "sidebar entry dropped"
    );
}

#[test]
fn retire_apply_fails_on_dangling_inbound_link() {
    let docs = roadmap_fixture(&[(
        "guides/foo.mdx",
        "---\ntitle: F\n---\n\nSee [the work](/roadmap/shipme/).\n",
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
    assert!(d.join("roadmap/shipme.mdx").exists(), "page must survive");
    let meta = read_meta(&d.join("roadmap/(grp)/meta.json")).unwrap();
    assert_eq!(meta["pages"][0], "../shipme", "sidebar entry must survive");
}

#[test]
fn retire_partial_sets_status_and_keeps_page() {
    let docs = roadmap_fixture(&[]);
    let item = docs.path().join("roadmap/shipme.mdx");
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
