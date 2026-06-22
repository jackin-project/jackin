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
    let auto = auto_prep(&[
        "crates/jackin-capsule/src/lib.rs".to_owned(),
        "docker/construct/Dockerfile".to_owned(),
    ]);

    assert!(auto.capsule);
    assert!(auto.construct);
}

#[test]
fn auto_prep_ignores_docs_only_changes() {
    let auto = auto_prep(&["docs/content/docs/reference/roadmap/pr-verification.mdx".to_owned()]);

    assert!(!auto.capsule);
    assert!(!auto.construct);
}

#[test]
fn auto_prep_treats_protocol_change_as_capsule() {
    let auto = auto_prep(&["crates/jackin-protocol/src/wire.rs".to_owned()]);

    assert!(auto.capsule);
    assert!(!auto.construct);
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
            auto_prep(&[file.to_owned()]).construct,
            "{file} should trigger a construct build"
        );
    }
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
        "headRefOid": "abc123",
        "files": [{"path": "a.rs"}, {"path": ""}, {"additions": 1}, {"path": "b.rs"}],
    });
    let info = parse_pr_info(&json).unwrap();

    assert_eq!(info.head_oid, "abc123");
    assert_eq!(info.changed_files, vec!["a.rs", "b.rs"]);
}

#[test]
fn parse_pr_info_rejects_missing_files() {
    let json = serde_json::json!({ "headRefOid": "abc123" });

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
