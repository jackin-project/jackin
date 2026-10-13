// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn runtime_pack_directory_overrides_embedded_pack() {
    let runtime = tempfile::tempdir().unwrap();
    write_test_pack(
        runtime.path(),
        "claude",
        "runtime-pack",
        "idle",
        "runtime marker",
    );

    let registry = RulePackRegistry::from_pack_dirs(Some(runtime.path()), None).unwrap();

    let matched = registry
        .evaluate(Some("claude"), &["runtime marker".to_owned()])
        .unwrap();
    assert_eq!(matched.rule_id, "runtime-pack");
    assert_eq!(matched.state, Some(RawAgentState::Idle));
}

#[test]
fn bundle_entry_parser_applies_entries_over_embedded_floor() {
    let entries = bundle_entries(vec![(
        "claude-remote",
        test_pack_toml("claude", "remote-pack", "blocked", "remote marker"),
    )]);

    let registry = registry_with_entries_for_parser_test(&entries);

    assert!(
        registry
            .evaluate(Some("claude"), &["remote marker".to_owned()])
            .is_some_and(|matched| matched.rule_id == "remote-pack"),
        "parsed entry should replace the embedded pack for the same agent"
    );
    assert!(
        registry
            .evaluate(Some("codex"), &["›".to_owned()])
            .is_some(),
        "entry parser must not remove unrelated embedded floor packs"
    );
    assert!(
        registry
            .notes()
            .iter()
            .any(|note| note.contains("remote pack claude-remote applied")),
        "applied remote packs must be reported: {:?}",
        registry.notes()
    );
}

#[test]
fn pack_sources_reject_unverified_bundle_and_keep_floor() {
    let bundle = SignedPackBundle {
        signer_identity: "jackin-project/agent-status-packs".to_owned(),
        signature: "jackin-agent-status-pack-bundle:v1:jackin-project/agent-status-packs"
            .to_owned(),
        packs: bundle_entries(vec![(
            "claude-remote",
            test_pack_toml("claude", "remote-pack", "blocked", "remote marker"),
        )]),
    };
    let registry = RulePackRegistry::from_sources([
        PackSource::Embedded,
        PackSource::SignedRemoteBundle(bundle),
    ])
    .unwrap();

    assert!(
        registry
            .evaluate(Some("claude"), &["remote marker".to_owned()])
            .is_none(),
        "unverified bundle content must not be parsed or applied"
    );
    assert!(
        registry
            .evaluate(Some("claude"), &["esc to interrupt".to_owned()])
            .is_some(),
        "embedded floor must survive rejected remote bundle"
    );
    assert!(
        registry.notes().iter().any(|note| note.contains(
            "remote pack bundle failed verification - using baked packs"
        )),
        "rejected remote bundle must be reported: {:?}",
        registry.notes()
    );
}

#[test]
fn bundle_entry_parser_skips_oversized_and_bad_packs_without_dropping_floor() {
    let entries = bundle_entries(vec![
        (
            "too-large",
            test_pack_toml(
                "claude",
                "too-large",
                "blocked",
                &"x".repeat(MAX_SIGNED_PACK_BYTES),
            ),
        ),
        (
            "bad-regex",
            r#"
schema_version = 1
agent = "codex"
validated_versions = ">=1.0.0, <2.0.0"

[[rule]]
id = "bad-regex"
state = "working"
priority = 100
region = "bottom:12"
regex = ["(unclosed"]
"#
            .to_owned(),
        ),
    ]);

    let registry = registry_with_entries_for_parser_test(&entries);

    assert!(
        registry
            .evaluate(Some("claude"), &["esc to interrupt".to_owned()])
            .is_some(),
        "oversized remote pack must not drop the embedded floor"
    );
    assert!(
        registry
            .evaluate(Some("codex"), &["›".to_owned()])
            .is_some(),
        "bad remote regex must not abort the registry"
    );
    assert!(
        registry
            .notes()
            .iter()
            .any(|note| note.contains("remote pack too-large skipped")),
        "oversized remote packs must be reported: {:?}",
        registry.notes()
    );
    assert!(
        registry
            .notes()
            .iter()
            .any(|note| note.contains("remote pack bad-regex")),
        "invalid remote packs must be reported: {:?}",
        registry.notes()
    );
}

#[test]
fn override_pack_directory_overrides_runtime_pack() {
    let runtime = tempfile::tempdir().unwrap();
    let override_dir = tempfile::tempdir().unwrap();
    write_test_pack(
        runtime.path(),
        "claude",
        "runtime-pack",
        "idle",
        "runtime marker",
    );
    write_test_pack(
        override_dir.path(),
        "claude",
        "override-pack",
        "blocked",
        "override marker",
    );

    let registry =
        RulePackRegistry::from_pack_dirs(Some(runtime.path()), Some(override_dir.path())).unwrap();

    assert!(
        registry
            .evaluate(Some("claude"), &["runtime marker".to_owned()])
            .is_none(),
        "override pack should replace the runtime pack for the same agent"
    );
    let matched = registry
        .evaluate(Some("claude"), &["override marker".to_owned()])
        .unwrap();
    assert_eq!(matched.rule_id, "override-pack");
    assert_eq!(matched.state, Some(RawAgentState::Blocked));
}

#[test]
fn loaded_pack_directory_replaces_existing_pack_for_same_agent() {
    let mut packs = HashMap::new();
    let bundled: RulePack = toml::from_str(
        r#"
schema_version = 1
agent = "test"
validated_versions = ">=1.0.0, <2.0.0"

[[rule]]
id = "bundled"
state = "working"
priority = 1
region = "bottom:12"
strength = "strong"
requires_all = ["bundled"]
"#,
    )
    .unwrap();
    packs.insert(bundled.agent.clone(), bundled);

    let tmp = tempfile::tempdir().unwrap();
    write_test_pack(tmp.path(), "test", "override", "blocked", "override");

    load_packs_from_dir(&mut packs, tmp.path()).unwrap();

    let matched = packs
        .get("test")
        .unwrap()
        .evaluate(&["override".to_owned()])
        .unwrap();
    assert_eq!(matched.rule_id, "override");
    assert_eq!(matched.state, Some(RawAgentState::Blocked));
}

#[test]
fn finalize_compiles_regexes_used_on_the_production_path() {
    let pack: RulePack = toml::from_str(
        "schema_version = 1\n\
         agent = \"test\"\n\
         validated_versions = \">=1.0.0, <2\"\n\
         [[rule]]\n\
         id = \"numbered-choice\"\n\
         state = \"blocked\"\n\
         priority = 100\n\
         region = \"bottom:5\"\n\
         line_regex = ['^\\s*\\d+\\.\\s']\n",
    )
    .unwrap();
    // finalize() is the production load path: it compiles every regex once into
    // the rule, so evaluate() uses the compiled regexes (not the per-call
    // fallback the validate-only tests above exercise).
    let pack = pack.finalize().unwrap();
    assert_eq!(pack.rule[0].compiled_line_regex.len(), 1);

    let rows = vec!["Choose one:".to_owned(), "  1. yes".to_owned()];
    assert_eq!(
        pack.evaluate(&rows).unwrap().state,
        Some(RawAgentState::Blocked),
        "compiled-regex path must match identically to the fallback path",
    );
    assert!(pack.evaluate(&["item 1. done".to_owned()]).is_none());
}

#[test]
fn finalize_sorts_rules_by_descending_priority() {
    // Lower-priority rule declared first; both match the same row. After
    // finalize the higher-priority rule must win, proving the sort happens at
    // load (not per evaluation).
    let pack: RulePack = toml::from_str(
        "schema_version = 1\n\
         agent = \"test\"\n\
         validated_versions = \">=1.0.0, <2\"\n\
         [[rule]]\n\
         id = \"low\"\n\
         state = \"idle\"\n\
         priority = 1\n\
         region = \"bottom:1\"\n\
         requires_all = [\"ready\"]\n\
         [[rule]]\n\
         id = \"high\"\n\
         state = \"blocked\"\n\
         priority = 100\n\
         region = \"bottom:1\"\n\
         requires_all = [\"ready\"]\n",
    )
    .unwrap();
    let pack = pack.finalize().unwrap();
    assert_eq!(pack.rule[0].id, "high");
    let matched = pack.evaluate(&["ready".to_owned()]).unwrap();
    assert_eq!(matched.rule_id, "high");
    assert_eq!(matched.state, Some(RawAgentState::Blocked));
}

#[test]
fn gate_nested_all_any_not_matches() {
    // Claude bash-permission shape: a shared positive prefix ("do you want to
    // proceed?") with its OWN sub-OR (bash markers) plus a numbered-choice line,
    // and a negative guard. Flattening this into one `requires_any` would let the
    // branches leak and over-match — the nested gate keeps them scoped.
    let pack = toml::from_str::<RulePack>(
        r#"
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
"#,
    )
    .unwrap()
    .finalize()
    .unwrap();

    // prefix + one of the OR branch + no negative -> blocked.
    let hit = vec![
        "Bash command: ls -la".to_owned(),
        "Do you want to proceed?".to_owned(),
    ];
    assert_eq!(
        pack.evaluate(&hit).unwrap().state,
        Some(RawAgentState::Blocked)
    );

    // prefix present but NEITHER OR branch -> no match (sub-OR is scoped).
    let no_or = vec![
        "Edit file foo.rs".to_owned(),
        "Do you want to proceed?".to_owned(),
    ];
    assert!(pack.evaluate(&no_or).is_none());

    // all positives but the `not` guard fires -> no match.
    let cancelled = vec![
        "Bash command: ls".to_owned(),
        "Do you want to proceed?".to_owned(),
        "(cancelled)".to_owned(),
    ];
    assert!(pack.evaluate(&cancelled).is_none());
}

#[test]
fn gate_invalid_regex_fails_validation() {
    let pack: RulePack = toml::from_str(
        r#"
schema_version = 1
agent = "test"
validated_versions = ">=1.0.0, <2"

[[rule]]
id = "bad-gate-regex"
state = "blocked"
priority = 100
region = "bottom:5"

[rule.gate]
any = [ { regex = "(unclosed" } ]
"#,
    )
    .unwrap();
    // The broken regex inside the gate must fail loudly at load, not silently
    // never match at runtime.
    pack.validate().unwrap_err();
}

#[test]
fn gate_leaf_count_counts_toward_matcher_cap() {
    let leaves = (0..33)
        .map(|i| format!("{{ contains = \"m{i}\" }}"))
        .collect::<Vec<_>>()
        .join(", ");
    let pack: RulePack = toml::from_str(&format!(
        "schema_version = 1\n\
         agent = \"test\"\n\
         validated_versions = \">=1.0.0, <2\"\n\
         [[rule]]\n\
         id = \"too-many\"\n\
         state = \"blocked\"\n\
         priority = 100\n\
         region = \"bottom:5\"\n\
         gate = {{ all = [{leaves}] }}\n"
    ))
    .unwrap();
    // 33 gate leaves exceed the 32-matcher pathological-pack cap.
    pack.validate().unwrap_err();
}
