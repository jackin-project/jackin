use super::*;

fn published_root_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for doc in super::super::ROOT_PROSE_DOCS {
        fs::write(dir.path().join(doc), "Published jackin❯ document.\n").unwrap();
    }
    dir
}

#[test]
fn strips_fenced_blocks() {
    let text = "prose jackin'\n```\njackin'\n```\nmore";
    let stripped = strip_code_regions(text);
    assert!(!stripped.contains("```"));
    assert_eq!(stripped.matches("jackin'").count(), 1);
}

#[test]
fn strips_inline_code() {
    let text = "see `jackin'` and Jackin in prose";
    let stripped = strip_code_regions(text);
    assert!(!stripped.contains("`jackin'`"));
    assert!(stripped.contains("Jackin"));
}

#[test]
fn strips_urls() {
    let text = "link http://example.com/jackin' end";
    let stripped = strip_code_regions(text);
    assert!(!stripped.contains("jackin'"));
    assert!(stripped.contains("end"));
}

#[test]
fn detects_real_violation() {
    let dir = published_root_fixture();
    let root = dir.path();
    fs::write(
        root.join("CONTRIBUTING.md"),
        "The jackin' product is great.\n",
    )
    .unwrap();
    let err = check_brand(root).unwrap_err().to_string();
    assert!(err.contains("jackin'"), "{err}");
    assert!(err.contains("CONTRIBUTING.md"), "{err}");
}

#[test]
fn clean_file_passes() {
    let dir = published_root_fixture();
    check_brand(dir.path()).unwrap();
}

#[test]
fn operational_root_inputs_are_outside_published_prose() {
    let dir = published_root_fixture();
    fs::write(dir.path().join("operator-task.md"), "Consolidate Jackin.\n").unwrap();
    check_brand(dir.path()).unwrap();
}

#[test]
fn missing_owned_root_document_is_an_error() {
    let dir = published_root_fixture();
    fs::remove_file(dir.path().join("CONTRIBUTING.md")).unwrap();
    let error = check_brand(dir.path()).unwrap_err().to_string();
    assert!(
        error.contains("owned root prose document missing"),
        "{error}"
    );
    assert!(error.contains("CONTRIBUTING.md"), "{error}");
}

#[test]
fn new_docs_pages_are_discovered_without_root_registration() {
    let dir = published_root_fixture();
    let content = dir.path().join("docs/content/new-domain");
    fs::create_dir_all(&content).unwrap();
    fs::write(content.join("new-page.mdx"), "Jackin product.\n").unwrap();
    let error = check_brand(dir.path()).unwrap_err().to_string();
    assert!(
        error.contains("docs/content/new-domain/new-page.mdx"),
        "{error}"
    );
}

#[test]
fn scans_roadmap_tree() {
    let dir = published_root_fixture();
    let root = dir.path();
    fs::create_dir_all(root.join("roadmap/topic")).unwrap();
    fs::write(
        root.join("roadmap/topic/README.md"),
        "The jackin' roadmap item.\n",
    )
    .unwrap();
    let err = check_brand(root).unwrap_err().to_string();
    assert!(err.contains("roadmap/topic/README.md"), "{err}");
}

#[test]
fn bare_prose_jackin_is_violation() {
    assert!(contains_bare_brand_prose("install jackin for agents"));
    assert!(contains_bare_brand_prose("the jackin product"));
}

#[test]
fn bare_identifier_shapes_are_not_violations() {
    assert!(!contains_bare_brand_prose("run `jackin load`"));
    assert!(!contains_bare_brand_prose("see jackin-capsule README"));
    assert!(!contains_bare_brand_prose("export JACKIN_DEBUG=1"));
    assert!(!contains_bare_brand_prose("path ~/.jackin/config.toml"));
    assert!(!contains_bare_brand_prose("brand is jackin❯ always"));
    assert!(!contains_bare_brand_prose("plaintext jackin> fallback"));
}

#[test]
fn strip_then_bare_detects_prose_only() {
    let text = "Use jackin-capsule binary.\n\nThe product is jackin for operators.\n";
    let stripped = strip_code_regions(text);
    let lines: Vec<_> = stripped.lines().collect();
    assert!(!contains_bare_brand_prose(lines[0]));
    assert!(contains_bare_brand_prose(lines[2]));
}
