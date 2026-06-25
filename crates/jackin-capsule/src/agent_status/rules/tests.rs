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

fn pack_with_versions(versions: &str) -> anyhow::Result<RulePack> {
    let pack: RulePack = toml::from_str(&format!(
        "schema_version = 1\nagent = \"test\"\nvalidated_versions = \"{versions}\"\n"
    ))
    .unwrap();
    pack.validate().map(|()| pack)
}

#[test]
fn validated_versions_must_be_bounded() {
    // Bounded ranges are accepted.
    assert!(pack_with_versions(">=2.1.0, <2.3.0").is_ok());
    assert!(pack_with_versions("=0.14.0").is_ok());
    // Wildcard and lower-only ranges are rejected — they could never gate a
    // future CLI whose TUI changed under the pack.
    assert!(pack_with_versions("*").is_err());
    assert!(pack_with_versions(">=2.1.0").is_err());
}

#[test]
fn accepts_cli_version_gates_the_pinned_window() {
    let pack = pack_with_versions(">=2.1.173, <2.2.0").unwrap();
    assert!(pack.accepts_cli_version("2.1.180").unwrap());
    assert!(!pack.accepts_cli_version("2.2.0").unwrap());
    assert!(!pack.accepts_cli_version("2.0.9").unwrap());
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

fn fixture(path: &str) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn fixture_for_detection(path: &Path) -> (Option<RawAgentState>, Vec<String>) {
    let mut rows = fixture(path.to_str().unwrap());
    let forbidden = rows
        .first()
        .and_then(|line| line.trim().strip_prefix("# not:"))
        .map(str::trim)
        .map(|state| match state {
            "working" => RawAgentState::Working,
            "blocked" => RawAgentState::Blocked,
            "idle" => RawAgentState::Idle,
            other => panic!("unknown forbidden state {other:?} in {path:?}"),
        });
    if forbidden.is_some() {
        rows.remove(0);
    }
    (forbidden, rows)
}

fn write_test_pack(dir: &Path, agent: &str, id: &str, state: &str, needle: &str) {
    fs::write(
        dir.join(format!("{agent}.toml")),
        format!(
            r#"
schema_version = 1
agent = "{agent}"
validated_versions = ">=1.0.0, <2.0.0"

[[rule]]
id = "{id}"
state = "{state}"
priority = 1
region = "bottom:12"
strength = "strong"
requires_all = ["{needle}"]
"#
        ),
    )
    .unwrap();
}

#[test]
fn packs_load_and_match_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    for agent in ["claude", "codex", "amp", "kimi", "opencode"] {
        let pack = RulePack::load(
            &root
                .join("docker/runtime/agent-status/packs")
                .join(format!("{agent}.toml")),
        )
        .unwrap();
        let fixture_dir = root
            .join("crates/jackin-capsule/src/agent_status/screen/fixtures")
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
