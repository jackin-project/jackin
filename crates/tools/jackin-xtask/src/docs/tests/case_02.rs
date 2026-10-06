// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
