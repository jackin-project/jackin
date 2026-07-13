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
    fs::write(&tests, "fn real_test() {}\n").unwrap();
    let err = verify_citation(
        root,
        "jackin_runtime::runtime::launch::tests::not_a_real_test",
    )
    .unwrap_err();
    assert!(err.contains("not found"), "{err}");
}

#[test]
fn verify_existing_fn_ok() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let tests = root.join("crates/jackin-runtime/src/runtime/launch/tests.rs");
    fs::create_dir_all(tests.parent().unwrap()).unwrap();
    fs::write(&tests, "async fn real_test() {}\n").unwrap();
    verify_citation(root, "jackin_runtime::runtime::launch::tests::real_test").unwrap();
}
