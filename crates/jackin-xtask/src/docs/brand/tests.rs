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
