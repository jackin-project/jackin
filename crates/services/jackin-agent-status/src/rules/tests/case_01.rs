// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn line_regex_matches_per_line_not_joined_blob() {
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
    pack.validate().unwrap();
    // A line that *starts* with "N. " -> match.
    let rows = vec!["Choose one:".to_owned(), "  1. yes".to_owned()];
    assert_eq!(
        pack.evaluate(&rows).unwrap().state,
        Some(RawAgentState::Blocked)
    );
    // The same token mid-line -> no match. A whole-region regex anchored at
    // ^ could not distinguish this; line_regex can.
    let rows2 = vec!["see item 1. here".to_owned()];
    assert!(pack.evaluate(&rows2).is_none());
}

#[test]
fn forbids_regex_blocks_anchored_pattern() {
    let pack: RulePack = toml::from_str(
        "schema_version = 1\n\
             agent = \"test\"\n\
             validated_versions = \">=1.0.0, <2\"\n\
             [[rule]]\n\
             id = \"blocked-unless-bare-caret\"\n\
             state = \"blocked\"\n\
             priority = 100\n\
             region = \"bottom:5\"\n\
             requires_any = [\"do you want to proceed\"]\n\
             forbids_regex = ['^\\s*>\\s*$']\n",
    )
    .unwrap();
    pack.validate().unwrap();
    let blocked = vec!["Do you want to proceed?".to_owned(), "  1. yes".to_owned()];
    assert_eq!(
        pack.evaluate(&blocked).unwrap().state,
        Some(RawAgentState::Blocked)
    );
    // A bare caret line means it is actually an idle prompt, not a dialog ->
    // the anchored forbid suppresses the blocked match.
    let idle = vec!["Do you want to proceed?".to_owned(), ">".to_owned()];
    assert!(pack.evaluate(&idle).is_none());
}

#[test]
fn validated_versions_must_be_bounded() {
    // Bounded ranges are accepted.
    pack_with_versions(">=2.1.0, <2.3.0").unwrap();
    pack_with_versions("=0.14.0").unwrap();
    // Wildcard and lower-only ranges are rejected — they could never gate a
    // future CLI whose TUI changed under the pack.
    pack_with_versions("*").unwrap_err();
    pack_with_versions(">=2.1.0").unwrap_err();
}

#[test]
fn min_engine_version_defaults_and_gates_future_engines() {
    // Absent field defaults to 1 and validates.
    let pack = pack_with_versions(">=1.0.0, <2").unwrap();
    assert_eq!(pack.min_engine_version, 1);

    // A pack needing a future engine is rejected (the load path logs + skips it).
    let future: RulePack = toml::from_str(&format!(
        "schema_version = 1\nagent = \"test\"\nvalidated_versions = \">=1.0.0, <2\"\nmin_engine_version = {}\n",
        RULE_ENGINE_VERSION + 1
    ))
    .unwrap();
    assert_eq!(future.min_engine_version, RULE_ENGINE_VERSION + 1);
    future
        .validate()
        .expect_err("a pack requiring a newer engine must be rejected");

    // The current engine version is accepted.
    let current: RulePack = toml::from_str(&format!(
        "schema_version = 1\nagent = \"test\"\nvalidated_versions = \">=1.0.0, <2\"\nmin_engine_version = {RULE_ENGINE_VERSION}\n"
    ))
    .unwrap();
    current.validate().unwrap();
}

#[test]
fn embedded_pack_loader_keeps_good_pack_when_peer_is_bad() {
    let good = r#"
schema_version = 1
agent = "test"
validated_versions = ">=1.0.0, <2"

[[rule]]
id = "ok"
state = "working"
priority = 1
region = "bottom:1"
requires_all = ["ok"]
"#;
    let bad = "schema_version = 1\nagent = \"broken\"\nvalidated_versions = \"*\"\n";
    let mut packs = HashMap::new();

    let failures = load_pack_sources(&mut packs, [("good", good), ("bad", bad)]);

    assert!(
        packs.contains_key("test"),
        "a malformed embedded pack must not drop valid peers"
    );
    assert_eq!(failures.len(), 1);
    assert!(
        failures[0].contains("bad"),
        "failure should name the bad embedded source: {failures:?}"
    );
}

#[test]
fn agent_screen_detector_coverage_is_exhaustive_or_reviewed() {
    // Reviewed S1 opt-outs: screen-detector packs need live per-agent TUI
    // capture and belong to the agent-status lane, not catalog expansion.
    const NO_SCREEN_DETECTOR: &[&str] =
        &["antigravity", "gemini", "cursor", "muse", "omp", "hermes"];
    let registry = RulePackRegistry::bundled().unwrap();

    for agent in jackin_core::Agent::ALL {
        let slug = agent.slug();
        if NO_SCREEN_DETECTOR.contains(&slug) {
            assert!(
                !registry.packs.contains_key(slug),
                "{slug} has a detector now; remove it from NO_SCREEN_DETECTOR"
            );
        } else {
            assert!(
                registry.packs.contains_key(slug),
                "{slug} must have a screen detector or reviewed opt-out"
            );
        }
    }
}

#[test]
fn prompt_caret_regions_isolate_live_prompt() {
    let pack: RulePack = toml::from_str(
        "schema_version = 1\n\
             agent = \"test\"\n\
             validated_versions = \">=1.0.0, <2\"\n\
             [[rule]]\n\
             id = \"q\"\n\
             state = \"blocked\"\n\
             priority = 100\n\
             region = \"after_last_prompt_marker\"\n\
             requires_any = [\"approve?\"]\n",
    )
    .unwrap();
    pack.validate().unwrap();
    // The question scrolled ABOVE the live caret -> not matched.
    let stale = vec![
        "Approve?".to_owned(),
        "› ".to_owned(),
        "ok thanks".to_owned(),
    ];
    assert!(pack.evaluate(&stale).is_none());
    // The question is below the caret (the live prompt) -> matched.
    let live = vec!["›".to_owned(), "Approve?".to_owned()];
    assert_eq!(
        pack.evaluate(&live).unwrap().state,
        Some(RawAgentState::Blocked)
    );
}

#[test]
fn whole_recent_without_caret_self_disables() {
    let pack: RulePack = toml::from_str(
        "schema_version = 1\n\
             agent = \"test\"\n\
             validated_versions = \">=1.0.0, <2\"\n\
             [[rule]]\n\
             id = \"w\"\n\
             state = \"working\"\n\
             priority = 100\n\
             region = \"whole_recent_without_current_prompt_marker\"\n\
             requires_any = [\"running\"]\n",
    )
    .unwrap();
    pack.validate().unwrap();
    // No caret -> whole screen -> matches.
    assert_eq!(
        pack.evaluate(&[String::from("task running")])
            .unwrap()
            .state,
        Some(RawAgentState::Working)
    );
    // Live caret present -> region self-disables -> no match (idle at prompt).
    assert!(
        pack.evaluate(&[String::from("task running"), String::from("› ")])
            .is_none()
    );
}

#[test]
fn packs_load_and_match_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    for agent in ["claude", "codex", "amp", "kimi", "opencode", "grok"] {
        let pack = RulePack::load(
            &root
                .join("crates/services/jackin-agent-status/packs")
                .join(format!("{agent}.toml")),
        )
        .unwrap();
        let fixture_dir = root
            .join("crates/services/jackin-agent-status/src/screen/fixtures")
            .join(agent);
        for entry in fs::read_dir(fixture_dir).unwrap() {
            let path = entry.unwrap().path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let (forbidden, rows) = fixture_for_detection(&path);
            let matched = pack.evaluate(&rows).and_then(|matched| matched.state);
            if name.starts_with("working") {
                assert_eq!(matched, Some(RawAgentState::Working), "{path:?}");
            } else if name.starts_with("blocked") {
                assert_eq!(matched, Some(RawAgentState::Blocked), "{path:?}");
            } else if name.starts_with("idle") {
                assert_eq!(matched, Some(RawAgentState::Idle), "{path:?}");
            } else if name.starts_with("false_positive") {
                assert_ne!(
                    matched,
                    Some(forbidden.unwrap_or(RawAgentState::Working)),
                    "{path:?}"
                );
            }
        }
    }
}

#[test]
fn regex_matchers_participate_in_rules() {
    let pack: RulePack = toml::from_str(
        r#"
schema_version = 1
agent = "test"
validated_versions = ">=1.0.0, <2.0.0"

[[rule]]
id = "anchored-spinner"
state = "working"
priority = 1
region = "bottom:12"
strength = "strong"
regex = ["^\\* thinking"]
"#,
    )
    .unwrap();
    pack.validate().unwrap();
    let rows = vec!["* Thinking".to_owned()];
    assert_eq!(
        pack.evaluate(&rows).and_then(|matched| matched.state),
        Some(RawAgentState::Working)
    );
}

#[test]
fn structural_regions_extract_prompt_and_rule_areas() {
    let rows = vec![
        "before".to_owned(),
        "────────────────────".to_owned(),
        "after rule".to_owned(),
        "╭────────────╮".to_owned(),
        "│ > hello    │".to_owned(),
        "╰────────────╯".to_owned(),
    ];

    assert_eq!(
        parse_region("prompt_box_body")
            .unwrap()
            .extract(&rows, VirtualRegions::default()),
        vec!["> hello".to_owned()]
    );
    assert_eq!(
        parse_region("above_prompt_box")
            .unwrap()
            .extract(&rows, VirtualRegions::default()),
        vec![
            "before".to_owned(),
            "────────────────────".to_owned(),
            "after rule".to_owned(),
        ]
    );
    assert_eq!(
        parse_region("after_last_rule")
            .unwrap()
            .extract(&rows, VirtualRegions::default()),
        vec![
            "after rule".to_owned(),
            "╭────────────╮".to_owned(),
            "│ > hello    │".to_owned(),
            "╰────────────╯".to_owned(),
        ]
    );

    let codex_rows = vec![
        "› older prompt".to_owned(),
        "• Working (old)".to_owned(),
        "› current prompt".to_owned(),
        "  gpt-5.3-codex-spark low · /repo".to_owned(),
    ];
    assert_eq!(
        parse_region("last_prompt_marker")
            .unwrap()
            .extract(&codex_rows, VirtualRegions::default()),
        vec!["› current prompt".to_owned()]
    );
    assert_eq!(
        parse_region("after_last_prompt_marker")
            .unwrap()
            .extract(&codex_rows, VirtualRegions::default()),
        vec!["  gpt-5.3-codex-spark low · /repo".to_owned()]
    );
}

#[test]
fn virtual_osc_regions_participate_in_matching_and_explain() {
    let pack: RulePack = toml::from_str(
        r#"
schema_version = 1
agent = "codex"
validated_versions = ">=1.0.0, <2.0.0"

[[rule]]
id = "title-spinner"
state = "working"
priority = 10
region = "osc_title"
strength = "strong"
requires_all = ["codex", "working"]

[[rule]]
id = "progress-cleared"
state = "idle"
priority = 9
region = "osc_progress"
strength = "strong"
requires_all = ["cleared"]
"#,
    )
    .unwrap();
    pack.validate().unwrap();

    let title_virtuals = VirtualRegions {
        osc_title: Some("Codex - working"),
        osc_progress: Some("inactive"),
    };
    let matched = pack
        .evaluate_with_virtuals(&[], title_virtuals)
        .expect("title rule should match");
    assert_eq!(matched.rule_id, "title-spinner");
    assert_eq!(matched.state, Some(RawAgentState::Working));

    let progress_virtuals = VirtualRegions {
        osc_title: None,
        osc_progress: Some("cleared"),
    };
    let explain = pack.explain_with_virtuals(&[], progress_virtuals);
    assert!(explain.iter().any(|rule| {
        rule.id == "progress-cleared" && rule.matched && rule.preview == "cleared"
    }));
}
