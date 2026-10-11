// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn gate_fallback_eval_matches_without_finalize() {
    // A pack that was validated but NOT finalized evaluates the gate through the
    // raw `Gate::eval` fallback (the `(None, Some(gate))` arm). This is the only
    // coverage of that hand-written second copy of the eval logic — it must agree
    // with the compiled path, so the same three outcomes hold.
    let toml = r#"
schema_version = 1
agent = "test"
validated_versions = ">=1.0.0, <2"

[[rule]]
id = "bash-permission"
state = "blocked"
priority = 100
region = "bottom:8"

[rule.gate]
all = [
  { contains = "do you want to proceed?" },
  { any = [ { contains = "bash command" }, { contains = "run shell" } ] },
  { not = { contains = "cancelled" } },
]
"#;
    // Validate-only (no finalize) -> compiled_gate stays None -> fallback path.
    let pack: RulePack = toml::from_str(toml).unwrap();
    pack.validate().unwrap();

    let hit = vec![
        "Bash command: ls -la".to_owned(),
        "Do you want to proceed?".to_owned(),
    ];
    assert_eq!(
        pack.evaluate(&hit).unwrap().state,
        Some(RawAgentState::Blocked),
        "fallback eval must match like the compiled path"
    );
    let no_or = vec![
        "Edit file foo.rs".to_owned(),
        "Do you want to proceed?".to_owned(),
    ];
    assert!(pack.evaluate(&no_or).is_none());
    let cancelled = vec![
        "Bash command: ls".to_owned(),
        "Do you want to proceed?".to_owned(),
        "(cancelled)".to_owned(),
    ];
    assert!(pack.evaluate(&cancelled).is_none());
}

#[test]
fn gate_regex_and_line_regex_leaves_match() {
    // `regex` anchors to the joined blob; `line_regex` is existential per line.
    let pack = toml::from_str::<RulePack>(
        r#"
schema_version = 1
agent = "test"
validated_versions = ">=1.0.0, <2"

[[rule]]
id = "regex-leaves"
state = "working"
priority = 100
region = "bottom:5"

[rule.gate]
all = [ { regex = "esc to interrupt" }, { line_regex = '^\s*[*]\s' } ]
"#,
    )
    .unwrap()
    .finalize()
    .unwrap();

    // Joined text contains "esc to interrupt" AND a line starts with "* ".
    let hit = vec!["* Thinking…".to_owned(), "(esc to interrupt)".to_owned()];
    assert_eq!(
        pack.evaluate(&hit).unwrap().state,
        Some(RawAgentState::Working)
    );
    // "* " only mid-line (not line-start) -> line_regex fails -> no match.
    let no_line = vec!["see * here".to_owned(), "(esc to interrupt)".to_owned()];
    assert!(pack.evaluate(&no_line).is_none());
    // line-start "* " present but the regex needle absent -> no match.
    let no_regex = vec!["* Thinking…".to_owned(), "all done".to_owned()];
    assert!(pack.evaluate(&no_regex).is_none());
}

#[test]
fn gate_rejects_empty_all_any() {
    for empty in ["{ any = [] }", "{ all = [] }"] {
        validate_gate(empty)
            .expect_err(&format!("vacuous gate `{empty}` must be rejected at load"));
    }
}

#[test]
fn gate_rejects_overlong_leaf() {
    let long = "x".repeat(513);
    validate_gate(&format!("{{ contains = \"{long}\" }}"))
        .expect_err("a >512-char gate leaf must be rejected");
}

#[test]
fn gate_rejects_excessive_nesting_depth() {
    // Build `not = { not = { … contains } }` nested past MAX_GATE_DEPTH.
    let mut gate = "{ contains = \"x\" }".to_owned();
    for _ in 0..20 {
        gate = format!("{{ not = {gate} }}");
    }
    validate_gate(&gate).expect_err("over-nested gate must fail at load");
}
