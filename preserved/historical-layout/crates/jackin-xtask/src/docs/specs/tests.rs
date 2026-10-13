use super::*;

#[test]
fn parse_requires_tests_column() {
    let text = r"
| INV | Description | Verify by |
|---|---|---|
| INV-1 | Trust first | `foo` |
";
    let rows = parse_inv_rows(text);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].tests.is_none());
}

#[test]
fn parse_reads_tests_cell() {
    let text = r"
| INV | Description | Verify by | Tests |
|---|---|---|---|
| INV-1 | Trust first | `foo` | `jackin_runtime::runtime::launch::tests::load_namespaced_agent_registers_source_and_trusts_on_accept` |
| INV-2 | Missing | `bar` | MISSING |
";
    let rows = parse_inv_rows(text);
    assert_eq!(rows.len(), 2);
    assert!(rows[0].tests.as_ref().unwrap().contains("load_namespaced"));
    assert_eq!(rows[1].tests.as_deref(), Some("MISSING"));
}

#[test]
fn verify_missing_fn_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let tests = root.join("crates/jackin-runtime/src/runtime/launch/tests.rs");
    fs::create_dir_all(tests.parent().unwrap()).unwrap();
    fs::write(&tests, "#[test]\nfn real_test() {}\n").unwrap();
    let err = verify_citation(
        root,
        "jackin_runtime::runtime::launch::tests::not_a_real_test",
    )
    .unwrap_err();
    assert!(err.contains("not found"), "{err}");
}

#[test]
fn verify_existing_test_fn_ok() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let tests = root.join("crates/jackin-runtime/src/runtime/launch/tests.rs");
    fs::create_dir_all(tests.parent().unwrap()).unwrap();
    fs::write(&tests, "#[test]\nasync fn real_test() {}\n").unwrap();
    verify_citation(root, "jackin_runtime::runtime::launch::tests::real_test").unwrap();
}

#[test]
fn verify_helper_fn_without_test_attr_fails() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let tests = root.join("crates/jackin-runtime/src/runtime/launch/tests.rs");
    fs::create_dir_all(tests.parent().unwrap()).unwrap();
    fs::write(&tests, "fn helper_only() {}\n").unwrap();
    let err =
        verify_citation(root, "jackin_runtime::runtime::launch::tests::helper_only").unwrap_err();
    assert!(
        err.contains("lacks a test attribute"),
        "helper must be rejected: {err}"
    );
}

#[test]
fn verify_commented_out_test_is_not_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let tests = root.join("crates/jackin-runtime/src/runtime/launch/tests.rs");
    fs::create_dir_all(tests.parent().unwrap()).unwrap();
    // Line-text matching would see `fn ghost_test` in a comment; syn must not.
    fs::write(
        &tests,
        "// #[test]\n// fn ghost_test() {}\n#[test]\nfn other() {}\n",
    )
    .unwrap();
    let err =
        verify_citation(root, "jackin_runtime::runtime::launch::tests::ghost_test").unwrap_err();
    assert!(err.contains("not found"), "{err}");
}

#[test]
fn verify_tokio_test_attr_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let tests = root.join("crates/jackin-runtime/src/runtime/launch/tests.rs");
    fs::create_dir_all(tests.parent().unwrap()).unwrap();
    fs::write(&tests, "#[tokio::test]\nasync fn async_case() {}\n").unwrap();
    verify_citation(root, "jackin_runtime::runtime::launch::tests::async_case").unwrap();
}

#[test]
fn missing_cell_fails_check_specs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let specs = root.join(SPECS_REL);
    fs::create_dir_all(&specs).unwrap();
    fs::write(
        specs.join("sample.mdx"),
        r"
| INV | Description | Verify by | Tests |
|---|---|---|---|
| INV-1 | Still open | unit | MISSING |
",
    )
    .unwrap();
    let err = check_specs(root).unwrap_err().to_string();
    assert!(err.contains("MISSING"), "{err}");
}
