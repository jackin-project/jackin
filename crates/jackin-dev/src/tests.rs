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
fn mise_exec_command_runs_inside_project_tool_env() {
    let cmd = mise_exec_command("cargo", ["run", "--bin", "build-jackin-capsule"]);

    assert_eq!(
        display_command(&cmd),
        "mise exec -- cargo run --bin build-jackin-capsule"
    );
}

#[test]
fn local_construct_image_ref_is_commit_pinned() {
    let repo = test_repo_root();
    let image = local_construct_image_ref(&repo).unwrap();
    let prefix = "jackin-local/construct:trixie-";
    assert!(
        image.starts_with(prefix),
        "expected commit-pinned local construct ref, got {image}"
    );
    let sha = &image[prefix.len()..];
    assert_eq!(sha.len(), 12, "short SHA must be 12 chars: {image}");
    assert!(
        sha.chars().all(|c| c.is_ascii_hexdigit()),
        "SHA must be hex: {image}"
    );
}

#[test]
fn parse_pr_refs_reads_head_name_and_oid() {
    let json = serde_json::json!({ "headRefName": "fix/example", "headRefOid": "abc123" });
    let (name, oid) = parse_pr_refs(&json).unwrap();
    assert_eq!(name, "fix/example");
    assert_eq!(oid, "abc123");
}

#[test]
fn parse_changed_files_trims_and_drops_blank_lines() {
    // Mimics `gh pr diff --name-only`, including a path past the old 100-file cap.
    let out = "a.rs\n  docker/construct/Dockerfile  \n\n\nb.rs\n";
    let files = parse_changed_files(out).unwrap();
    assert_eq!(files, vec!["a.rs", "docker/construct/Dockerfile", "b.rs"]);
}

#[test]
fn parse_changed_files_rejects_empty() {
    assert!(parse_changed_files("\n  \n").is_err());
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
