use super::*;

#[test]
fn json_round_trip_shape() {
    let report = Report::new(
        "file-size",
        vec![Violation {
            rule: "file-size",
            file: "crates/foo.rs".into(),
            line: Some(12),
            message: "over budget by 10 lines".into(),
            fix: "split the module".into(),
            rerun: "cargo xtask lint files".into(),
        }],
    );
    assert!(!report.ok);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["gate"], "file-size");
    assert_eq!(json["ok"], false);
    assert_eq!(json["violations"][0]["file"], "crates/foo.rs");
    assert_eq!(json["violations"][0]["line"], 12);
    assert_eq!(json["violations"][0]["rule"], "file-size");
    assert!(
        !json["violations"][0]["message"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert!(!json["violations"][0]["fix"].as_str().unwrap().is_empty());
    assert!(!json["violations"][0]["rerun"].as_str().unwrap().is_empty());
}

#[test]
fn ok_report_has_empty_violations() {
    let report = Report::new("agents", vec![]);
    assert!(report.ok);
    assert!(report.violations.is_empty());
}

#[test]
fn github_escaping_percent_and_newline() {
    assert_eq!(escape_workflow_prop("a%b"), "a%25b");
    assert_eq!(escape_workflow_prop("a\nb"), "a%0Ab");
    assert_eq!(escape_workflow_prop("a:b,c"), "a%3Ab%2Cc");
    assert_eq!(escape_workflow_data("why\nline"), "why%0Aline");
    assert_eq!(escape_workflow_data("100%"), "100%25");
}

#[test]
fn detect_explicit_flag_wins() {
    assert_eq!(Format::detect(Some(Format::Json)), Format::Json);
    assert_eq!(Format::detect(Some(Format::Human)), Format::Human);
    assert_eq!(Format::detect(Some(Format::Github)), Format::Github);
}

#[test]
fn detect_defaults_to_human_when_actions_unset() {
    // Env mutation is forbidden (`unsafe_code = forbid`); cover the
    // non-Actions path only when the runner has not set GITHUB_ACTIONS.
    if env::var_os("GITHUB_ACTIONS").is_none() {
        assert_eq!(Format::detect(None), Format::Human);
    }
}

#[test]
fn violation_fields_are_non_empty() {
    let v = Violation {
        rule: "agents",
        file: "crates/x/AGENTS.md".into(),
        line: None,
        message: "missing AGENTS.md".into(),
        fix: "create AGENTS.md".into(),
        rerun: "cargo xtask lint agents".into(),
    };
    assert!(!v.rule.is_empty());
    assert!(!v.file.is_empty());
    assert!(!v.message.is_empty());
    assert!(!v.fix.is_empty());
    assert!(!v.rerun.is_empty());
}

#[test]
fn prose_gate_failure_maps_to_complete_structured_diagnostic() {
    let report = report_from_result(
        "headers",
        "crates/",
        "restore headers",
        "cargo xtask lint headers",
        Err(anyhow::anyhow!(
            "crates/example/src/lib.rs: missing invariant"
        )),
    );
    let value = serde_json::to_value(report).expect("serialize report");
    let violation = &value["violations"][0];
    for key in ["file", "line", "message", "fix", "rerun"] {
        assert!(violation.get(key).is_some(), "missing {key}");
    }
}
