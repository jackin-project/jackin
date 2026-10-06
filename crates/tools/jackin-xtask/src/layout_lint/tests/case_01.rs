// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
fn ungated_helper_modules_are_not_suites_but_test_gated_ones_are() {
    // Shared fixtures use ungated decls with the gate inside the file
    // (`#![cfg(test)]`); only test-gated or suite-named mods must be canonical.
    assert!(non_tests_rs_violation("pub mod fixtures;\n").is_none());
    assert!(non_tests_rs_violation("mod support;\n").is_none());
    assert!(non_tests_rs_violation("#[cfg(test)]\nmod helpers;\n").is_some());
    assert!(non_tests_rs_violation("#[cfg(test)]\nmod support {}\n").is_some());
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
fn tests_rs_accepts_canonical_cases_and_rejects_the_rest() {
    assert!(tests_rs_violation("use super::*;\nmod case_01;\nmod support;\n").is_none());
    assert!(tests_rs_violation("#[cfg(unix)]\nmod linux_cases;\n").is_none());
    for source in [
        "mod helpers { fn value() {} }\n",
        "#[path = \"tests/case.rs\"]\nmod case;\n",
        "pub mod case_01;\n",
        "#[cfg(test)]\nmod extra;\n",
        "#[test]\nfn works() { mod helpers { pub fn value() {} } }\n",
    ] {
        assert!(
            tests_rs_violation(source).is_some(),
            "child spelling should be rejected: {source:?}"
        );
    }
    assert!(
        tests_rs_violation(
            "// mod helpers;\nconst EXAMPLE: &str = r#\"mod helpers {}\"#;\n#[test]\nfn works() {}\n"
        )
        .is_none()
    );
}

#[test]
fn filesystem_measurement_accepts_declared_cases_and_flags_strays() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("crates/group/example/src");
    fs::create_dir_all(src.join("foo/tests")).unwrap();
    fs::write(src.join("foo.rs"), "#[cfg(test)]\nmod tests;\n").unwrap();
    fs::write(
        src.join("foo/tests.rs"),
        "use super::*;\nmod case_01;\nmod support;\n",
    )
    .unwrap();
    fs::write(src.join("foo/tests/case_01.rs"), "#[test] fn works() {}\n").unwrap();
    fs::write(src.join("foo/tests/support.rs"), "fn helper() {}\n").unwrap();
    fs::write(src.join("foo/tests/stray.rs"), "#[test] fn stray() {}\n").unwrap();

    let violations = measure_violations(temp.path()).unwrap();
    assert!(!violations.contains_key("crates/group/example/src/foo/tests.rs"));
    assert!(!violations.contains_key("crates/group/example/src/foo/tests/case_01.rs"));
    assert!(!violations.contains_key("crates/group/example/src/foo/tests/support.rs"));
    assert!(violations.contains_key("crates/group/example/src/foo/tests/stray.rs"));
}

#[test]
fn filesystem_measurement_still_finds_source_violations() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("crates/group/example/src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("lib.rs"), "#[cfg(test)] mod legacy_tests;\n").unwrap();

    let violations = measure_violations(temp.path()).unwrap();
    assert!(violations.contains_key("crates/group/example/src/lib.rs"));
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
