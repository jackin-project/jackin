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
    let tree_lock = dir.path().join("tree-meta.json");
    write_meta(&meta, &json!({ "title": "X", "pages": ["index"] })).unwrap();
    write_meta(&tree_lock, &json!({ "pages": [] })).unwrap();
    append_page(&meta, &tree_lock, "alpha").unwrap();
    append_page(&meta, &tree_lock, "alpha").unwrap();
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
    write(&root.join("docs/content/guide.mdx"), page_body);
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
        "should include docs markdown outside content: {err}"
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
        err.contains("<RepoFile> is only allowed under docs/content"),
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
        err.contains("docs/content/guide.mdx") && !err.contains("docs/AGENTS.md"),
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
fn codebase_map_inventory_requires_members_tiers_and_links() {
    let members = vec!["jackin-core".to_owned()];
    let tiers = BTreeMap::from([("jackin-core", 0)]);
    let valid = "| Crate | Tier | README |\n|---|---:|---|\n\
        | [jackin-core](/reference/crates/jackin-core/) | 0 | <RepoFile path=\"crates/jackin-core/README.md\">README</RepoFile> |\n";
    check_codebase_map_text(valid, &members, &tiers, "map.mdx").expect("valid inventory");

    let invalid = [
        ("jackin-core".to_owned(), "missing inventory row"),
        (
            valid.replace("| 0 |", "| 1 |"),
            "missing architecture tier 0",
        ),
        (
            valid.replace("crates/jackin-core/README.md", "missing.md"),
            "missing README link",
        ),
    ];
    for (body, expected) in invalid {
        let err = check_codebase_map_text(&body, &members, &tiers, "map.mdx")
            .expect_err("invalid inventory must fail")
            .to_string();
        assert!(err.contains(expected), "{err}");
    }
}

#[test]
fn codebase_map_inventory_rejects_non_members() {
    let members = vec!["jackin-core".to_owned()];
    let tiers = BTreeMap::from([("jackin-core", 0)]);
    let map = "| [jackin-core](/reference/crates/jackin-core/) | 0 | <RepoFile path=\"crates/jackin-core/README.md\">README</RepoFile> |\n\
        stale crate jackin-deleted\n";
    let err = check_codebase_map_text(map, &members, &tiers, "map.mdx")
        .expect_err("stale member must fail")
        .to_string();
    assert!(err.contains("jackin-deleted"), "{err}");
}

#[test]
fn repo_links_reject_repo_file_component_traversal() {
    let repo = repo_link_fixture(
        "---\ntitle: Guide\n---\n\n\
         See <RepoFile path=\"docs/content/../../Cargo.toml\" />.\n",
    );

    let err = check_repo_links_in(repo.path(), &repo.path().join(DOCS_ROOT))
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("RepoFile path does not exist")
            && err.contains("docs/content/../../Cargo.toml"),
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
    write_meta(&r.join("meta.json"), &json!({ "pages": [] })).unwrap();
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
    assert!(body.contains("## Current state") && body.contains("## Completion gate"));
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
    let prompts = tempfile::tempdir().unwrap();
    let r = research.path();
    write_meta(&r.join("meta.json"), &json!({ "pages": [] })).unwrap();
    fs::create_dir_all(r.join("agents")).unwrap();
    write_meta(&r.join("agents/meta.json"), &json!({ "pages": [] })).unwrap();

    research_scaffold_in(
        r,
        prompts.path(),
        ResearchScaffoldArgs {
            slug: "my-study".to_owned(),
            group: "agents".to_owned(),
            title: None,
        },
    )
    .unwrap();

    assert!(r.join("agents/my-study/index.mdx").is_file());
    assert!(prompts.path().join("agents/my-study.md").is_file());
    let dossier_meta = read_meta(&r.join("agents/my-study/meta.json")).unwrap();
    assert_eq!(dossier_meta["pages"], json!(["index"]));
    let index = fs::read_to_string(r.join("agents/my-study/index.mdx")).unwrap();
    assert!(index.contains("<RepoFile path=\"prompts/research/agents/my-study.md\" />"));
    assert!(index.contains("**Research state:** Incomplete"));
    assert!(index.find("## Headline findings") < index.find("## Method and evidence"));
    assert!(index.find("## Method and evidence") < index.find("## Limitations and open questions"));
    assert!(index.find("## Limitations and open questions") < index.find("## How to read"));
    let parent = read_meta(&r.join("agents/meta.json")).unwrap();
    assert_eq!(parent["pages"].as_array().unwrap()[0], "my-study");
}

#[test]
fn research_validation_enforces_shared_page_contract() {
    let research = tempfile::tempdir().unwrap();
    let r = research.path();
    write_meta(&r.join("meta.json"), &json!({ "pages": ["bad"] })).unwrap();
    write(
        &r.join("bad.mdx"),
        "---\ntitle: Bad\n---\n\n# Bad\n\n**Research state**: Working\n",
    );

    let err = validate_tree(r, "research").unwrap_err().to_string();
    assert!(err.contains("frontmatter `description`"), "{err}");
    assert!(err.contains("canonical `**Research state:**`"), "{err}");
    assert!(err.contains("remove the explicit H1"), "{err}");
}

#[test]
fn research_validation_rejects_published_prompt() {
    let research = tempfile::tempdir().unwrap();
    let r = research.path();
    write_meta(&r.join("meta.json"), &json!({ "pages": ["prompt"] })).unwrap();
    write(
        &r.join("prompt.mdx"),
        "---\ntitle: Brief\ndescription: A published brief.\n---\n",
    );

    let err = validate_tree(r, "research").unwrap_err().to_string();
    assert!(
        err.contains("briefs belong under prompts/research"),
        "{err}"
    );
}

#[test]
fn research_validation_rejects_broken_card_target() {
    let research = tempfile::tempdir().unwrap();
    let r = research.path();
    write_meta(&r.join("meta.json"), &json!({ "pages": ["index"] })).unwrap();
    write(
        &r.join("index.mdx"),
        "---\ntitle: Research\ndescription: A valid research landing page.\n---\n\n<Card title=\"Missing\" href=\"/research/missing/\">Missing.</Card>\n",
    );

    let err = validate_tree(r, "research").unwrap_err().to_string();
    assert!(err.contains("Card target `/research/missing/`"), "{err}");
}

#[test]
fn research_validation_checks_every_card_and_markdown_link() {
    let research = tempfile::tempdir().unwrap();
    let r = research.path();
    write_meta(&r.join("meta.json"), &json!({ "pages": ["index"] })).unwrap();
    write(
        &r.join("index.mdx"),
        "---\ntitle: Research\ndescription: A valid research landing page.\n---\n\n<Card href=\"/research/missing-one/\" /><Card href=\"/research/missing-two/\" />\n\n[Relative](chapter/)\n\n[Missing](/research/missing-three/)\n",
    );

    let err = validate_tree(r, "research").unwrap_err().to_string();
    assert!(err.contains("missing-one"), "{err}");
    assert!(err.contains("missing-two"), "{err}");
    assert!(err.contains("missing-three"), "{err}");
    assert!(err.contains("must be site-absolute"), "{err}");
}

#[test]
fn research_validation_rejects_literal_ellipsis_and_oversized_page() {
    let research = tempfile::tempdir().unwrap();
    let r = research.path();
    write_meta(&r.join("meta.json"), &json!({ "pages": ["large"] })).unwrap();
    let body = "line\n".repeat(401);
    write(
        &r.join("large.mdx"),
        &format!("---\ntitle: Large\ndescription: An incomplete description...\n---\n\n{body}"),
    );

    let err = validate_tree(r, "research").unwrap_err().to_string();
    assert!(err.contains("informative sentence"), "{err}");
    assert!(err.contains("more than 400 body lines"), "{err}");
}

#[test]
fn research_scaffold_does_not_overwrite_existing_brief() {
    let research = tempfile::tempdir().unwrap();
    let prompts = tempfile::tempdir().unwrap();
    let r = research.path();
    fs::create_dir_all(r.join("agents")).unwrap();
    fs::create_dir_all(prompts.path().join("agents")).unwrap();
    write_meta(
        &r.join("agents/meta.json"),
        &json!({ "pages": ["other-study"] }),
    )
    .unwrap();
    let parent_before = fs::read(r.join("agents/meta.json")).unwrap();
    write(&prompts.path().join("agents/my-study.md"), "keep me\n");

    let err = research_scaffold_in(
        r,
        prompts.path(),
        ResearchScaffoldArgs {
            slug: "my-study".to_owned(),
            group: "agents".to_owned(),
            title: None,
        },
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("research brief already exists"), "{err}");
    assert_eq!(
        fs::read_to_string(prompts.path().join("agents/my-study.md")).unwrap(),
        "keep me\n"
    );
    assert!(!r.join("agents/my-study").exists());
    assert_eq!(fs::read(r.join("agents/meta.json")).unwrap(), parent_before);
}

#[test]
fn research_scaffold_rolls_back_partial_writes_when_parent_meta_is_invalid() {
    let research = tempfile::tempdir().unwrap();
    let prompts = tempfile::tempdir().unwrap();
    let r = research.path();
    fs::create_dir_all(r.join("agents")).unwrap();
    let invalid_meta = b"{ invalid json }\n";
    fs::write(r.join("agents/meta.json"), invalid_meta).unwrap();

    let err = research_scaffold_in(
        r,
        prompts.path(),
        ResearchScaffoldArgs {
            slug: "my-study".to_owned(),
            group: "agents".to_owned(),
            title: None,
        },
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("meta.json"), "{err}");
    assert!(!r.join("agents/my-study").exists());
    assert!(!prompts.path().join("agents/my-study.md").exists());
    assert_eq!(fs::read(r.join("agents/meta.json")).unwrap(), invalid_meta);
}

#[test]
fn research_scaffold_rejects_group_traversal_and_multiline_title() {
    let research = tempfile::tempdir().unwrap();
    let prompts = tempfile::tempdir().unwrap();
    let r = research.path();
    fs::create_dir_all(r.join("agents")).unwrap();
    write_meta(&r.join("agents/meta.json"), &json!({ "pages": [] })).unwrap();

    for (group, title) in [("../roadmap", None), ("agents", Some("Bad\ntitle"))] {
        let err = research_scaffold_in(
            r,
            prompts.path(),
            ResearchScaffoldArgs {
                slug: "my-study".to_owned(),
                group: group.to_owned(),
                title: title.map(str::to_owned),
            },
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("invalid") || err.contains("single line"),
            "{err}"
        );
    }
    assert!(!r.join("my-study").exists());
}

#[test]
fn research_scaffold_escapes_quoted_title_in_frontmatter() {
    let research = tempfile::tempdir().unwrap();
    let prompts = tempfile::tempdir().unwrap();
    let r = research.path();
    write_meta(&r.join("meta.json"), &json!({ "pages": [] })).unwrap();
    fs::create_dir_all(r.join("agents")).unwrap();
    write_meta(&r.join("agents/meta.json"), &json!({ "pages": [] })).unwrap();

    research_scaffold_in(
        r,
        prompts.path(),
        ResearchScaffoldArgs {
            slug: "quoted-study".to_owned(),
            group: "agents".to_owned(),
            title: Some("A \"quoted\" study".to_owned()),
        },
    )
    .unwrap();

    let index = fs::read_to_string(r.join("agents/quoted-study/index.mdx")).unwrap();
    assert!(
        index.contains("title: \"A \\\"quoted\\\" study\""),
        "{index}"
    );
}

#[test]
fn research_scaffold_never_removes_preexisting_dossier() {
    let research = tempfile::tempdir().unwrap();
    let prompts = tempfile::tempdir().unwrap();
    let r = research.path();
    fs::create_dir_all(r.join("agents/my-study")).unwrap();
    write(&r.join("agents/my-study/keep.md"), "keep me\n");
    write_meta(&r.join("agents/meta.json"), &json!({ "pages": [] })).unwrap();

    let err = research_scaffold_in(
        r,
        prompts.path(),
        ResearchScaffoldArgs {
            slug: "my-study".to_owned(),
            group: "agents".to_owned(),
            title: None,
        },
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("research dossier already exists"), "{err}");
    assert_eq!(
        fs::read_to_string(r.join("agents/my-study/keep.md")).unwrap(),
        "keep me\n"
    );
    assert!(!prompts.path().join("agents/my-study.md").exists());
}

#[test]
fn concurrent_research_scaffolds_keep_both_sidebar_entries() {
    let research = tempfile::tempdir().unwrap();
    let prompts = tempfile::tempdir().unwrap();
    let r = research.path();
    write_meta(&r.join("meta.json"), &json!({ "pages": [] })).unwrap();
    fs::create_dir_all(r.join("agents")).unwrap();
    write_meta(&r.join("agents/meta.json"), &json!({ "pages": [] })).unwrap();
    let prompts_path = prompts.path();

    std::thread::scope(|scope| {
        for slug in ["first-study", "second-study"] {
            scope.spawn(move || {
                research_scaffold_in(
                    r,
                    prompts_path,
                    ResearchScaffoldArgs {
                        slug: slug.to_owned(),
                        group: "agents".to_owned(),
                        title: None,
                    },
                )
                .unwrap();
            });
        }
    });

    let meta = read_meta(&r.join("agents/meta.json")).unwrap();
    let pages = meta["pages"].as_array().unwrap();
    assert!(pages.iter().any(|page| page == "first-study"));
    assert!(pages.iter().any(|page| page == "second-study"));
}

#[test]
fn line_references_slug_is_boundary_safe() {
    assert!(line_references_slug("see /roadmap/auth/ for", "auth"));
    assert!(line_references_slug("    \"../auth\"", "auth"));
    assert!(!line_references_slug("/roadmap/auth-health/", "auth"));
    assert!(!line_references_slug("nothing here", "auth"));
}

/// Build a `docs/content` shape with one roadmap item colocated with its
/// group metadata, plus optional extra files. Returns the docs-root temp dir.
fn roadmap_fixture(extra: &[(&str, &str)]) -> tempfile::TempDir {
    let docs = tempfile::tempdir().unwrap();
    let d = docs.path();
    write_meta_mk(
        &d.join("roadmap/(grp)/meta.json"),
        &json!({ "pages": ["shipme"] }),
    );
    write(
        &d.join("roadmap/(grp)/shipme.mdx"),
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
    assert!(!d.join("roadmap/(grp)/shipme.mdx").exists(), "page deleted");
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
    assert!(
        d.join("roadmap/(grp)/shipme.mdx").exists(),
        "page must survive"
    );
    let meta = read_meta(&d.join("roadmap/(grp)/meta.json")).unwrap();
    assert_eq!(meta["pages"][0], "shipme", "sidebar entry must survive");
}

#[test]
fn retire_partial_sets_status_and_keeps_page() {
    let docs = roadmap_fixture(&[]);
    let item = docs.path().join("roadmap/(grp)/shipme.mdx");
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
