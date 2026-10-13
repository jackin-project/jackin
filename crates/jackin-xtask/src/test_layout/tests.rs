use std::fs;

use super::*;

#[test]
fn canonical_suite_is_the_only_test_suite_declaration_allowed() {
    assert!(non_tests_rs_violation("#[cfg(test)]\nmod tests;\n").is_none());

    for source in [
        "#[cfg(test)] mod tests;\n",
        "#[cfg( test )]\nmod tests;\n",
        "#[cfg(all(test, feature = \"otlp\"))]\nmod tests;\n",
        "#[cfg_attr(unix, cfg(test))]\nmod tests;\n",
        "#[cfg(test)]\nmod checks;\n",
        "#[cfg(test)]\nmod export_category_tests;\n",
        "#[cfg(test)]\npub(crate) mod tests;\n",
        "#[path = \"foo/tests.rs\"]\n#[cfg(test)]\nmod tests;\n",
        "#[cfg(test)]\nmod tests { #[test] fn works() {} }\n",
        "#[cfg(test)]\nmod\ntests;\n",
    ] {
        assert!(
            non_tests_rs_violation(source).is_some(),
            "suite spelling should be rejected: {source:?}"
        );
    }
}

#[test]
fn direct_test_attributes_are_found_by_syntax() {
    for attr in [
        "#[test]",
        "#[tokio::test]",
        "#[tokio::test(flavor = \"multi_thread\")]",
        "#[rstest]",
        "#[rstest(case::empty(\"\"))]",
        "#[cfg_attr(unix, test)]",
        "#[cfg_attr(unix, tokio::test)]",
        "#[cfg_attr(unix, cfg_attr(feature = \"x\", test))]",
        "#[async_std::test]",
        "#[test_case::test_case]",
    ] {
        let source = format!("{attr}\nfn works() {{}}\n");
        assert!(
            non_tests_rs_violation(&source).is_some(),
            "attribute should be rejected: {attr}"
        );
    }
}

#[test]
fn syntax_scan_ignores_comments_strings_and_test_only_helpers() {
    let source = r##"
/// Production registries call this from a `#[test]`.
const EXAMPLE: &str = r#"#[cfg(test)] mod hidden_tests;"#;
#[cfg(test)]
fn helper() -> bool { true }
#[cfg_attr(test, allow(dead_code, reason = "test helper"))]
fn another_helper() {}
"##;
    assert!(non_tests_rs_violation(source).is_none());
}

#[test]
fn external_test_support_is_allowed_but_inline_support_is_not() {
    assert!(
        non_tests_rs_violation_at("crates/jackin-launch/src/lib.rs", "mod test_support;\n")
            .is_none()
    );
    assert!(non_tests_rs_violation("mod test_support;\n").is_some());
    assert!(non_tests_rs_violation("#[cfg(test)]\nmod test_support {}\n").is_some());
}

#[test]
fn nested_inline_test_suite_is_found() {
    let source = "mod outer { #[cfg(test)] mod tests { #[test] fn works() {} } }";
    assert!(non_tests_rs_violation(source).is_some());
}

#[test]
fn malformed_rust_is_a_violation_instead_of_an_audit_bypass() {
    assert!(non_tests_rs_violation("mod tests {").is_some());
    assert!(tests_rs_violation("fn broken(").is_some());
}

#[test]
fn tests_rs_rejects_child_modules_but_ignores_text_that_looks_like_one() {
    assert!(tests_rs_violation("use super::*;\nmod helpers;\n").is_some());
    assert!(tests_rs_violation("mod helpers { fn value() {} }\n").is_some());
    assert!(
        tests_rs_violation("#[test]\nfn works() { mod helpers { pub fn value() {} } }\n").is_some()
    );
    assert!(
        tests_rs_violation(
            "// mod helpers;\nconst EXAMPLE: &str = r#\"mod helpers {}\"#;\n#[test]\nfn works() {}\n"
        )
        .is_none()
    );
}

#[test]
fn filesystem_measurement_finds_source_and_split_directory_violations() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("crates/example/src");
    fs::create_dir_all(src.join("tests")).unwrap();
    fs::write(src.join("lib.rs"), "#[cfg(test)] mod legacy_tests;\n").unwrap();
    fs::write(src.join("tests/case.rs"), "#[test] fn works() {}\n").unwrap();

    let violations = measure_violations(temp.path()).unwrap();
    assert!(violations.contains_key("crates/example/src/lib.rs"));
    assert!(violations.contains_key("crates/example/src/tests/case.rs"));
}

fn violation(path: &str) -> (String, String) {
    (path.to_owned(), "inline test module".to_owned())
}

#[test]
fn check_passes_when_allowlist_exactly_matches_violations() {
    let violations = BTreeMap::from([violation("crates/a/src/foo.rs")]);
    let allowed = BTreeSet::from(["crates/a/src/foo.rs".to_owned()]);
    check(&violations, &allowed).unwrap();
}

#[test]
fn check_rejects_new_violation_not_in_allowlist() {
    let violations = BTreeMap::from([violation("crates/a/src/foo.rs")]);
    let error = check(&violations, &BTreeSet::new())
        .unwrap_err()
        .to_string();
    assert!(error.contains("crates/a/src/foo.rs"), "{error}");
    assert!(error.contains("test-layout violation"), "{error}");
}

#[test]
fn check_rejects_stale_allowlist_row() {
    let allowed = BTreeSet::from(["crates/a/src/fixed.rs".to_owned()]);
    let error = check(&BTreeMap::new(), &allowed).unwrap_err().to_string();
    assert!(error.contains("crates/a/src/fixed.rs"), "{error}");
    assert!(
        error.contains("remove the stale allowlist entry"),
        "{error}"
    );
}
