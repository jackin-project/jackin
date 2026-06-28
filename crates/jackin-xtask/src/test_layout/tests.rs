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
    let stacked = "#[cfg(test)]\n#[allow(clippy::all)]\nmod tests {\n}\n";
    assert!(inline_test_module_violation(stacked).is_some());
}

#[test]
fn tests_rs_child_module_is_flagged() {
    assert!(tests_rs_violation("use super::*;\nmod helpers;\n").is_some());
    assert!(tests_rs_violation("use super::*;\n#[test]\nfn t() {}\n").is_none());
}
