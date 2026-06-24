use super::*;

#[test]
fn paths_stay_inside_pr_bundle() {
    let root = PathBuf::from("/Users/example/Projects/jackin-project/test/pr-580");
    let paths = PrPaths::from_root(root.clone());

    assert_eq!(paths.repo, root.join("jackin"));
    assert_eq!(paths.env_file, root.join("env.sh"));
    assert_eq!(paths.config, root.join("state/config"));
    assert_eq!(paths.home, root.join("state/home"));
}

#[test]
fn env_points_at_bundle_state() {
    let root = PathBuf::from("/Users/example/Projects/jackin-project/test/pr-580");
    let paths = PrPaths::from_root(root);
    let env = env_lines(&paths).join("\n");

    // Assert against the derived paths, not re-typed literals, so a layout
    // rename propagates here instead of silently passing.
    assert!(env.contains(&paths.config.display().to_string()));
    assert!(env.contains(&paths.home.display().to_string()));
    assert!(!env.contains(".config/jackin-pr-"));
    assert!(!env.contains(".jackin-pr-"));
}

#[test]
fn auto_prep_detects_capsule_and_construct_inputs() {
    let repo = test_repo_root();
    let auto = auto_prep(
        &repo,
        &[
            "crates/jackin-capsule/src/lib.rs".to_owned(),
            "docker/construct/Dockerfile".to_owned(),
        ],
    )
    .unwrap();

    assert!(auto.capsule.required);
    assert!(auto.construct.required);
    assert_eq!(
        auto.construct.reasons,
        vec!["docker/construct/Dockerfile: construct image source changed"]
    );
}

#[test]
fn auto_prep_ignores_docs_only_changes() {
    let repo = test_repo_root();
    let auto = auto_prep(
        &repo,
        &["docs/content/docs/reference/roadmap/pr-verification.mdx".to_owned()],
    )
    .unwrap();

    assert!(!auto.capsule.required);
    assert!(!auto.construct.required);
    assert!(auto.capsule.reasons.is_empty());
    assert!(auto.construct.reasons.is_empty());
}

#[test]
fn auto_prep_treats_protocol_change_as_capsule() {
    let repo = test_repo_root();
    let auto = auto_prep(&repo, &["crates/jackin-protocol/src/wire.rs".to_owned()]).unwrap();

    assert!(auto.capsule.required);
    assert!(!auto.construct.required);
    assert_eq!(
        auto.capsule.reasons,
        vec!["crates/jackin-protocol/src/wire.rs: jackin-protocol is used by jackin-capsule"]
    );
}

#[test]
fn auto_prep_treats_tui_dependency_change_as_capsule() {
    let repo = test_repo_root();
    let auto = auto_prep(&repo, &["crates/jackin-tui/src/lib.rs".to_owned()]).unwrap();

    assert!(auto.capsule.required);
    assert!(!auto.construct.required);
}

#[test]
fn auto_prep_ignores_unrelated_workspace_package_change() {
    let repo = test_repo_root();
    let auto = auto_prep(&repo, &["crates/jackin-dev/src/main.rs".to_owned()]).unwrap();

    assert!(!auto.capsule.required);
    assert!(!auto.construct.required);
}

#[test]
fn auto_prep_construct_triggers() {
    for file in [
        "docker-bake.hcl",
        "mise.toml",
        "crates/jackin-xtask/src/construct.rs",
        "crates/jackin-xtask/src/construct/image.rs",
    ] {
        assert!(
            construct_build_decision(&[file.to_owned()]).required,
            "{file} should trigger a construct build"
        );
    }
}

#[test]
fn construct_decision_explains_triggering_files() {
    let decision = construct_build_decision(&[
        "docs/content/docs/getting-started/concepts.mdx".to_owned(),
        "docker-bake.hcl".to_owned(),
        "crates/jackin-xtask/src/construct.rs".to_owned(),
    ]);

    assert!(decision.required);
    assert_eq!(
        decision.reasons,
        vec![
            "docker-bake.hcl: construct image bake graph changed",
            "crates/jackin-xtask/src/construct.rs: construct build orchestration changed",
        ]
    );
}

#[test]
fn capsule_decision_explains_broad_workspace_inputs() {
    let repo = test_repo_root();
    let decision = capsule_build_decision(&repo, &["Cargo.lock".to_owned()]).unwrap();

    assert!(decision.required);
    assert_eq!(
        decision.reasons,
        vec!["Cargo.lock: workspace build inputs changed"]
    );
}

#[test]
fn path_only_prep_explains_capsule_dependency_without_checkout() {
    let auto = auto_prep_from_paths(&["crates/jackin-tui/src/lib.rs".to_owned()]);

    assert!(auto.capsule.required);
    assert!(!auto.construct.required);
    assert_eq!(
        auto.capsule.reasons,
        vec!["crates/jackin-tui/src/lib.rs: jackin-tui is used by jackin-capsule"]
    );
}

fn test_repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/jackin-dev should live two levels below repo root")
        .to_owned()
}

#[test]
fn shell_quote_quotes_spaces() {
    assert_eq!(
        shell_quote(OsStr::new("/tmp/with space/env.sh")),
        "'/tmp/with space/env.sh'"
    );
}

#[test]
fn shell_quote_leaves_path_safe_chars_unquoted() {
    assert_eq!(shell_quote(OsStr::new("a:b+c/d-e_f.g")), "a:b+c/d-e_f.g");
}

#[test]
fn shell_quote_escapes_single_quotes() {
    // Closes the quote, emits a literal ', reopens — the POSIX idiom.
    assert_eq!(shell_quote(OsStr::new("it's")), r#"'it'"'"'s'"#);
}

#[test]
fn shell_quote_quotes_shell_metachars() {
    assert_eq!(shell_quote(OsStr::new("$HOME")), "'$HOME'");
}

#[test]
fn parse_pr_info_filters_empty_and_non_string_paths() {
    let json = serde_json::json!({
        "headRefName": "fix/example",
        "headRefOid": "abc123",
        "files": [{"path": "a.rs"}, {"path": ""}, {"additions": 1}, {"path": "b.rs"}],
    });
    let info = parse_pr_info(&json).unwrap();

    assert_eq!(info.head_ref_name, "fix/example");
    assert_eq!(info.head_oid, "abc123");
    assert_eq!(info.changed_files, vec!["a.rs", "b.rs"]);
}

#[test]
fn parse_pr_info_rejects_missing_files() {
    let json = serde_json::json!({ "headRefName": "fix/example", "headRefOid": "abc123" });

    assert!(parse_pr_info(&json).is_err());
}

#[test]
fn json_string_rejects_missing_and_empty() {
    let json = serde_json::json!({ "headRefOid": "" });

    assert!(json_string(&json, "headRefOid").is_err());
    assert!(json_string(&json, "absent").is_err());
    assert_eq!(
        json_string(&serde_json::json!({ "k": "v" }), "k").unwrap(),
        "v"
    );
}
