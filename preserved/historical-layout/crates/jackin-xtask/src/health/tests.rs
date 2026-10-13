use super::*;
use std::fs;

#[test]
fn rust_source_walk_skips_nested_build_and_dependency_trees() {
    let dir = tempfile::tempdir().expect("tempdir");
    for rel in ["src/kept.rs", "target/generated.rs", "node_modules/pkg.rs"] {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, "fn marker() {}\n").expect("write fixture");
    }
    let paths = walk_rs_paths(dir.path()).expect("walk sources");
    assert_eq!(paths, vec![dir.path().join("src/kept.rs")]);
}

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
    let src = "#[allow(dead_code)]\nfn x() {}\n";
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
        "trend",
    ] {
        assert!(json.get(key).is_some(), "missing key {key}");
    }
}

#[test]
fn trend_reports_delta_and_requires_sustained_dated_headroom() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("ratchet.toml"),
        r#"
[[family]]
id = "agent-doc-bytes"
[[family.entry]]
key = "AGENTS.md"
bound = 100
"#,
    );
    let mut history = String::new();
    for (observed_at_unix, bytes) in [
        (1_700_000_000_u64, 75_usize),
        (1_700_900_000, 76),
        (1_701_800_000, 74),
        (1_702_500_000, 72),
    ] {
        let snapshot = serde_json::json!({
            "observed_at_unix": observed_at_unix,
            "report": {"agent_docs": [{"path": "AGENTS.md", "bytes": bytes}]}
        });
        history.push_str(&snapshot.to_string());
        history.push('\n');
    }
    write(&dir.path().join("health-history.jsonl"), &history);

    let trend = build_trend_section(
        dir.path(),
        &[DocBytes {
            path: "AGENTS.md".to_owned(),
            bytes: 70,
            token_approx: 17,
        }],
    )
    .unwrap();

    assert_eq!(trend.history_snapshots, 4);
    assert_eq!(trend.deltas.len(), 1);
    assert_eq!(trend.deltas[0].baseline, 75);
    assert_eq!(trend.deltas[0].delta, -5);
    assert_eq!(trend.proposals.len(), 1);
}

#[test]
fn trend_accepts_legacy_raw_reports_but_does_not_treat_them_as_dated() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("health-history.jsonl"),
        r#"{"agent_docs":[{"path":"AGENTS.md","bytes":80}]}"#,
    );
    let trend = build_trend_section(
        dir.path(),
        &[DocBytes {
            path: "AGENTS.md".to_owned(),
            bytes: 70,
            token_approx: 17,
        }],
    )
    .unwrap();

    assert_eq!(trend.deltas[0].delta, -10);
    assert!(trend.proposals.is_empty());
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

#[test]
fn ignores_attribute_shaped_text_in_comments_and_strings() {
    let src = concat!(
        "// #[expect(dead_code)]\n",
        "/* #[expect(unused)] */\n",
        "fn f() {\n",
        "    let _s = \"#[expect(dead_code)]\";\n",
        "    let _r = r#\"#[expect(dead_code)]\"#;\n",
        "    let _c = 'x';\n",
        "}\n",
    );
    let attrs = parse_suppression_attrs(src);
    assert!(attrs.is_empty(), "expected no suppressions, got {attrs:?}");
}

#[test]
fn comma_inside_reason_does_not_invent_lints() {
    let src = r#"
#[expect(clippy::disallowed_methods, reason = "shells out to git, gh, cargo, and mise")]
fn run() {}
"#;
    let attrs = parse_suppression_attrs(src);
    assert_eq!(attrs.len(), 1);
    let (_allow, lints, has_reason) = &attrs[0];
    assert!(*has_reason);
    assert_eq!(lints.as_slice(), &["clippy::disallowed_methods".to_owned()]);
    assert!(
        !lints
            .iter()
            .any(|l| l == "and" || l == "cargo" || l == "gh")
    );
}

#[test]
fn cfg_attr_allow_is_collected() {
    let src = "#[cfg_attr(test, allow(dead_code))]\nfn x() {}\n";
    let attrs = parse_suppression_attrs(src);
    assert_eq!(attrs.len(), 1);
    assert!(attrs[0].0);
    assert!(attrs[0].1.iter().any(|l| l == "dead_code"));
}

#[test]
fn bare_allow_vs_expect_with_reason_policy() {
    let bare = parse_suppression_attrs("#[allow(dead_code)]\nfn x() {}\n");
    assert_eq!(bare.len(), 1);
    assert!(
        bare[0].0 && !bare[0].2,
        "bare allow must report has_reason=false"
    );
    let with = parse_suppression_attrs(
        "#[expect(dead_code, reason = \"documented residual allow; prefer expect when site is lint-true\")]\nfn y() {}\n",
    );
    assert_eq!(with.len(), 1);
    assert!(
        !with[0].0 && with[0].2,
        "expect with reason is not bare allow"
    );
}
