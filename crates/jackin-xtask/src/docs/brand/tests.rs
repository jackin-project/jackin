use super::*;

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
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("NOTE.md"), "The jackin' product is great.\n").unwrap();
    let err = check_brand(root).unwrap_err().to_string();
    assert!(err.contains("jackin'"), "{err}");
    assert!(err.contains("NOTE.md"), "{err}");
}

#[test]
fn clean_file_passes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("NOTE.md"), "The jackin❯ product is great.\n").unwrap();
    check_brand(root).unwrap();
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
