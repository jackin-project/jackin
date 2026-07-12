use super::*;

#[test]
fn mod_has_body_distinguishes_declaration_from_inline() {
    assert_eq!(mod_has_body("mod tests;"), Some(false));
    assert_eq!(mod_has_body("mod tests {"), Some(true));
    assert_eq!(mod_has_body("pub mod helpers {"), Some(true));
    assert_eq!(mod_has_body("pub(crate) mod thing;"), Some(false));
    assert_eq!(mod_has_body("pub(super) mod thing {"), Some(true));
    assert_eq!(mod_has_body("let x = 1;"), None);
    assert_eq!(mod_has_body("fn model() {"), None);
    // `mod_foo` is an identifier, not a `mod` declaration.
    assert_eq!(mod_has_body("mod_foo();"), None);
}

#[test]
fn inline_test_module_is_flagged_but_declaration_is_not() {
    let inline = "fn a() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n";
    assert!(inline_test_module_violation(inline).is_some());

    let declaration = "fn a() {}\n#[cfg(test)]\nmod tests;\n";
    assert!(inline_test_module_violation(declaration).is_none());

    // A `#[cfg(test)]` on a helper fn (not a module) is not a layout violation.
    let cfg_fn = "#[cfg(test)]\nfn helper() -> bool { true }\n";
    assert!(inline_test_module_violation(cfg_fn).is_none());

    // Stacked attributes between `#[cfg(test)]` and the `mod` are tolerated.
    // Fixture string uses escaped quotes so the bare-allow scanner still sees
    // a reason= and does not inflate jackin-xtask bare-allow debt.
    let stacked =
        "#[cfg(test)]\n#[allow(clippy::all, reason = \"fixture\")]\nmod tests {\n}\n";
    assert!(inline_test_module_violation(stacked).is_some());
}

#[test]
fn direct_test_attributes_are_flagged_outside_tests_rs() {
    for attr in [
        "#[test]",
        "#[tokio::test]",
        "#[tokio::test(flavor = \"multi_thread\")]",
        "#[rstest]",
        "#[rstest(case::empty(\"\"))]",
    ] {
        let text = format!("{attr}\nfn t() {{}}\n");
        assert!(
            direct_test_attr_violation(&text).is_some(),
            "{attr} should be rejected outside tests.rs"
        );
        assert!(
            non_tests_rs_violation(&text).is_some(),
            "{attr} should make a non-tests file violate layout"
        );
    }
}

#[test]
fn direct_test_attribute_scan_ignores_comments_and_helpers() {
    let comment = "/// Production registries call this from a `#[test]` so mistakes fail.\n";
    assert!(direct_test_attr_violation(comment).is_none());

    let helper = "#[cfg(test)]\nfn helper() -> bool { true }\n";
    assert!(direct_test_attr_violation(helper).is_none());
    assert!(non_tests_rs_violation(helper).is_none());
}

#[test]
fn tests_rs_child_module_is_flagged() {
    assert!(tests_rs_violation("use super::*;\nmod helpers;\n").is_some());
    assert!(tests_rs_violation("use super::*;\n#[test]\nfn t() {}\n").is_none());
}

fn violation(path: &str) -> (String, String) {
    (path.to_owned(), "inline test module".to_owned())
}

#[test]
fn check_passes_when_allowlist_exactly_matches_violations() {
    let v = violation("crates/a/src/foo.rs");
    let violations = BTreeMap::from([v]);
    let allowed = BTreeSet::from(["crates/a/src/foo.rs".to_owned()]);
    check(&violations, &allowed).unwrap();
}

#[test]
fn check_rejects_new_violation_not_in_allowlist() {
    let v = violation("crates/a/src/foo.rs");
    let violations = BTreeMap::from([v]);
    let allowed = BTreeSet::new();
    let err = check(&violations, &allowed).unwrap_err().to_string();
    assert!(err.contains("crates/a/src/foo.rs"), "{err}");
    assert!(err.contains("test-layout violation"), "{err}");
}

#[test]
fn check_rejects_stale_allowlist_row() {
    // The allowlist still names a file that no longer violates -> stale row.
    let violations = BTreeMap::new();
    let allowed = BTreeSet::from(["crates/a/src/fixed.rs".to_owned()]);
    let err = check(&violations, &allowed).unwrap_err().to_string();
    assert!(err.contains("crates/a/src/fixed.rs"), "{err}");
    assert!(
        err.contains("no longer violates") && err.contains("remove the stale allowlist entry"),
        "{err}"
    );
}

#[test]
fn check_rejects_missing_allowlist_path_as_stale() {
    // An allowlist path that was never a real violation is indistinguishable
    // from a stale row (the parser stores paths, not provenance), so the gate
    // treats a missing path as stale and fails.
    let violations = BTreeMap::new();
    let allowed = BTreeSet::from(["crates/ghost/src/nope.rs".to_owned()]);
    let err = check(&violations, &allowed).unwrap_err().to_string();
    assert!(err.contains("crates/ghost/src/nope.rs"), "{err}");
    assert!(err.contains("remove the stale allowlist entry"), "{err}");
}
