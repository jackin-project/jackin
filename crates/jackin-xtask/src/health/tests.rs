use super::*;
use std::fs;

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

#[test]
fn parses_multiline_multi_lint_expect_with_reason() {
    let src = r#"
#[expect(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    reason = "legacy body awaiting extraction"
)]
fn big() {}
"#;
    let attrs = parse_suppression_attrs(src);
    assert_eq!(attrs.len(), 1);
    let (is_allow, lints, has_reason) = &attrs[0];
    assert!(!*is_allow);
    assert!(*has_reason);
    assert!(lints.iter().any(|l| l.contains("too_many_lines")));
    assert!(lints.iter().any(|l| l.contains("cognitive_complexity")));
}

#[test]
fn parses_bare_allow() {
    let src = "#[allow(dead_code, reason = "documented residual allow; prefer expect when site is lint-true")]\nfn x() {}\n";
    let attrs = parse_suppression_attrs(src);
    assert_eq!(attrs.len(), 1);
    assert!(attrs[0].0);
    assert!(!attrs[0].2);
    assert!(attrs[0].1.iter().any(|l| l == "dead_code"));
}

#[test]
fn untested_large_classifier_finds_module_without_sibling_tests() {
    let dir = tempfile::tempdir().unwrap();
    let crates = dir.path().join("crates/demo/src");
    write(
        &crates.join("big.rs"),
        &"// padding line\n".repeat(LARGE_MODULE_LINES + 5),
    );
    write(&crates.join("small.rs"), "fn tiny() {}\n");
    // With sibling tests, should not appear
    write(
        &crates.join("covered.rs"),
        &"// padding line\n".repeat(LARGE_MODULE_LINES + 5),
    );
    write(&crates.join("covered/tests.rs"), "#[test] fn t() {}\n");

    let counts = measure_rs_files(dir.path()).unwrap();
    let untested = untested_large(dir.path(), &counts);
    let paths: Vec<_> = untested.iter().map(|f| f.path.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.ends_with("big.rs")),
        "expected big.rs in {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("covered.rs")),
        "covered.rs should have sibling tests: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("small.rs")),
        "small.rs under threshold: {paths:?}"
    );
}

#[test]
fn json_report_contains_required_keys() {
    let root = repo_root().unwrap();
    let report = collect(&root, 3).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    for key in [
        "largest_production_files",
        "largest_test_files",
        "untested_large_modules",
        "suppressions",
        "pub_surface",
        "agent_docs",
        "duplicate_helpers",
        "advisory",
        "verification_map",
    ] {
        assert!(json.get(key).is_some(), "missing key {key}");
    }
}

#[test]
fn verification_map_covers_every_workspace_member() {
    let root = repo_root().unwrap();
    let report = collect(&root, 3).unwrap();
    assert!(
        report.verification_map.len() >= 10,
        "expected workspace members, got {}",
        report.verification_map.len()
    );
    assert!(
        report.verification_map.contains_key("jackin-xtask"),
        "missing jackin-xtask in {:?}",
        report.verification_map.keys().collect::<Vec<_>>()
    );
}
