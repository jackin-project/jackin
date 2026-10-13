use super::*;
use crate::cmd::shell_quote;
use std::ffi::OsStr;

#[test]
fn shell_quote_leaves_plain_paths_bare() {
    assert_eq!(
        shell_quote(OsStr::new("/tmp/jackin-pr-550")),
        "/tmp/jackin-pr-550"
    );
}

#[test]
fn shell_quote_wraps_spaces_and_quotes() {
    assert_eq!(
        shell_quote(OsStr::new("/tmp/PR user's checkout")),
        "'/tmp/PR user'\"'\"'s checkout'"
    );
}

#[test]
fn classify_detects_categories_from_paths() {
    let cats = classify(&[
        "crates/x/src/a.rs".to_owned(),
        "docs/content/x.mdx".to_owned(),
        "crates/core/jackin-config/src/versions.rs".to_owned(),
    ]);
    assert!(cats.rust && cats.docs && cats.schema);
    assert!(!cats.capsule);
}

#[test]
fn filter_template_keeps_checkout_and_gated_blocks() {
    let tpl = "## Summary\n\nprose\n\n## Verify locally\n\nintro\n\n\
               ### Checkout\n\nco\n\n### Rust tests\n\nrt\n\n\
               ### Docs checks\n\ndc\n\n## Migration notes\n\nnone\n";
    let cats = Categories {
        rust: true,
        ..Categories::default()
    };
    let out = filter_template(tpl, &cats);
    assert!(out.contains("### Checkout"), "checkout always kept");
    assert!(out.contains("### Rust tests"), "rust block kept");
    assert!(
        !out.contains("### Docs checks"),
        "docs block dropped: {out}"
    );
    assert!(out.contains("## Summary") && out.contains("## Migration notes"));
}

#[test]
fn pr_body_reads_the_canonical_docs_template() {
    let root = tempfile::tempdir().expect("temporary repo root");
    let docs = root.path().join("docs");
    let github = root.path().join(".github");
    fs::create_dir_all(&docs).expect("create docs directory");
    fs::create_dir_all(&github).expect("create generated directory");
    fs::write(
        docs.join("PULL_REQUEST_TEMPLATE.md"),
        "canonical docs template",
    )
    .expect("write docs template");
    fs::write(
        github.join("PULL_REQUEST_TEMPLATE.md"),
        "stale generated template",
    )
    .expect("write stale template");

    assert_eq!(
        read_template(root.path()).expect("read canonical template"),
        "canonical docs template"
    );
}
