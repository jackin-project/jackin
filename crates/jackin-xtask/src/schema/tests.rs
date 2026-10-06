use super::*;

#[test]
fn parse_version_reads_the_declaration() {
    let src = r#"
        //! comment
        pub const CURRENT_CONFIG_VERSION: &str = "v1alpha6";
        pub const CURRENT_WORKSPACE_VERSION: &str = "v1alpha6";
    "#;
    assert_eq!(
        parse_version(src, "CURRENT_CONFIG_VERSION").as_deref(),
        Some("v1alpha6")
    );
    assert_eq!(
        parse_version(src, "CURRENT_WORKSPACE_VERSION").as_deref(),
        Some("v1alpha6")
    );
    assert_eq!(parse_version(src, "CURRENT_MANIFEST_VERSION"), None);
}

#[test]
fn parse_version_ignores_non_declaration_usages() {
    // A usage line (no `const NAME`) must not be mistaken for the declaration.
    let src = "fn default() -> String { CURRENT_MANIFEST_VERSION.to_owned() }";
    assert_eq!(parse_version(src, "CURRENT_MANIFEST_VERSION"), None);
}

/// Build a fake repo root with the doc + a fixtures tree for one kind.
fn fixture_root(
    kind_dir: &str,
    from_ver: &str,
    target_ver: &str,
    doc_has: &str,
) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    let from = r
        .join("crates/jackin/tests/fixtures/migrations")
        .join(kind_dir)
        .join(format!("from-{from_ver}"));
    fs::create_dir_all(&from).unwrap();
    fs::write(
        from.join("meta.toml"),
        format!("target_version = \"{target_ver}\"\n"),
    )
    .unwrap();
    fs::write(from.join("before.toml"), "x = 1\n").unwrap();
    fs::write(from.join("after.toml"), "x = 1\n").unwrap();
    let doc = r.join(SCHEMA_VERSIONS_DOC);
    fs::create_dir_all(doc.parent().unwrap()).unwrap();
    fs::write(
        &doc,
        format!("# Schema versions\n\n## Timeline\n\n### `{doc_has}` — 2026-01-01\n"),
    )
    .unwrap();
    root
}

fn config_kind() -> &'static SchemaKind {
    KINDS.iter().find(|k| k.name == "config").unwrap()
}

#[test]
fn check_bump_passes_when_artifacts_present() {
    let root = fixture_root("config", "v1alpha5", "v1alpha6", "v1alpha6");
    let mut problems = Vec::new();
    check_bump(
        root.path(),
        config_kind(),
        "v1alpha5",
        "v1alpha6",
        &mut problems,
    )
    .unwrap();
    assert!(problems.is_empty(), "expected clean, got: {problems:?}");
}

#[test]
fn check_bump_flags_missing_fixture_and_doc_entry() {
    // Fixture dir is from-v1alpha4, but the bump is v1alpha5 → v1alpha6, and
    // the doc only mentions v1alpha5.
    let root = fixture_root("config", "v1alpha4", "v1alpha5", "v1alpha5");
    let mut problems = Vec::new();
    check_bump(
        root.path(),
        config_kind(),
        "v1alpha5",
        "v1alpha6",
        &mut problems,
    )
    .unwrap();
    let joined = problems.join("\n");
    assert!(
        joined.contains("missing fixture file"),
        "should flag fixture: {joined}"
    );
    assert!(
        joined.contains("Timeline entry for `v1alpha6`"),
        "should flag doc entry: {joined}"
    );
}

#[test]
fn doc_timeline_entry_is_backtick_bounded() {
    let doc = "## Timeline\n\n### `v1alpha10` — d\n\n### Manifest `v1alpha5` — d\n";
    assert!(doc_has_timeline_entry(doc, "v1alpha10"));
    assert!(doc_has_timeline_entry(doc, "v1alpha5"));
    // `v1alpha1` must NOT be satisfied by the `v1alpha10` heading.
    assert!(!doc_has_timeline_entry(doc, "v1alpha1"));
    assert!(!doc_has_timeline_entry(doc, "v1alpha6"));
    // A bare prose mention is not a Timeline entry.
    assert!(!doc_has_timeline_entry(
        "see v1alpha6 in passing",
        "v1alpha6"
    ));
}
