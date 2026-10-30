use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{CiArgs, e2e_selected, parse_capsule_export, step_names, validate_capsule_path};

#[test]
fn e2e_partition_selects_the_complete_docker_suite() {
    let args = CiArgs {
        fast: false,
        e2e: false,
        e2e_capsule: None,
        e2e_filter: None,
        base: "origin/main".to_owned(),
        only: vec!["e2e".to_owned()],
    };

    assert!(e2e_selected(&args));
}

#[test]
fn tests_partition_runs_the_pre_commit_snapshot_fixture() {
    let args = CiArgs {
        fast: true,
        e2e: false,
        e2e_capsule: None,
        e2e_filter: None,
        base: "origin/main".to_owned(),
        only: vec!["tests".to_owned()],
    };

    assert_eq!(
        step_names(&args).unwrap().first().map(String::as_str),
        Some("pre-commit snapshot fixture")
    );
}

#[test]
fn lint_partition_uses_native_jactionlint() {
    let args = CiArgs {
        fast: true,
        e2e: false,
        e2e_capsule: None,
        e2e_filter: None,
        base: "origin/main".to_owned(),
        only: vec!["lint".to_owned()],
    };

    assert_eq!(
        step_names(&args).unwrap().first().map(String::as_str),
        Some("jactionlint")
    );
}

#[test]
fn parse_capsule_export_accepts_single_quoted_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let capsule = temp.path().join("jackin-capsule");
    fs::write(&capsule, "").expect("capsule");

    let output = format!("export JACKIN_CAPSULE_BIN='{}'\n", capsule.display());

    assert_eq!(parse_capsule_export(&output).unwrap(), capsule);
}

#[test]
fn parse_capsule_export_rejects_missing_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let capsule = temp.path().join("missing-capsule");
    let output = format!("export JACKIN_CAPSULE_BIN='{}'\n", capsule.display());

    let err = parse_capsule_export(&output).unwrap_err().to_string();

    assert!(err.contains("capsule export path does not exist"));
}

#[test]
fn existing_relative_capsule_path_is_resolved_from_the_repository() {
    let temp = tempfile::tempdir().expect("tempdir");
    let capsule = temp.path().join("target/debug/jackin-capsule");
    fs::create_dir_all(capsule.parent().expect("parent")).expect("target directory");
    fs::write(&capsule, "").expect("capsule");

    assert_eq!(
        validate_capsule_path(temp.path(), Path::new("target/debug/jackin-capsule")).unwrap(),
        capsule
    );
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn workspace_file(path: &str) -> String {
    fs::read_to_string(workspace_root().join(path)).expect("workspace file")
}

fn assert_no_sccache_env(text: &str, context: &str) {
    for line in [
        "CARGO_INCREMENTAL: \"0\"",
        "RUSTC_WRAPPER: sccache",
        "SCCACHE_GHA_ENABLED: \"true\"",
        "CARGO_INCREMENTAL=0",
        "RUSTC_WRAPPER=sccache",
        "SCCACHE_GHA_ENABLED=true",
    ] {
        assert!(!text.contains(line), "{context} exports `{line}`");
    }
}

const GENERATED_WORKFLOWS: [&str; 1] = [".github/workflows/ci.yml"];

#[test]
fn sccache_is_absent_from_generated_workflow_environment() {
    for workflow in GENERATED_WORKFLOWS {
        let text = workspace_file(workflow);
        assert_no_sccache_env(&text, workflow);
    }
}

fn assert_no_sccache_value(value: &serde_json::Value, context: &str) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                match key.as_str() {
                    "CARGO_INCREMENTAL" => assert!(
                        value.as_str() != Some("0") && value.as_u64() != Some(0),
                        "{context} disables incremental compilation"
                    ),
                    "RUSTC_WRAPPER" => assert!(
                        !value.as_str().is_some_and(
                            |wrapper| wrapper == "sccache" || wrapper.ends_with("/sccache")
                        ),
                        "{context} selects sccache"
                    ),
                    "SCCACHE_GHA_ENABLED" => assert!(
                        value.as_str() != Some("true") && value.as_bool() != Some(true),
                        "{context} enables sccache"
                    ),
                    _ => {}
                }
                assert_no_sccache_value(value, context);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                assert_no_sccache_value(value, context);
            }
        }
        serde_json::Value::String(text) => assert_no_sccache_env(text, context),
        _ => {}
    }
}

fn assert_installer_environments(workflow: &serde_json::Value, context: &str) -> usize {
    let jobs = workflow["jobs"].as_object().expect("workflow jobs");
    let mut installers = 0;
    for (id, job) in jobs {
        let Some(steps) = job["steps"].as_array() else {
            continue;
        };
        for (index, step) in steps.iter().enumerate() {
            if !step["uses"]
                .as_str()
                .is_some_and(|uses| uses.starts_with("jdx/mise-action@"))
            {
                continue;
            }
            installers += 1;
            let context = format!("{context} job {id} installer {index}");
            assert_no_sccache_value(&workflow["env"], &context);
            assert_no_sccache_value(&job["env"], &context);
            for prior in &steps[..=index] {
                assert_no_sccache_value(prior, &context);
            }
        }
    }
    assert!(installers > 0, "{context} has no Mise installer");
    installers
}

#[test]
fn sccache_is_absent_from_installer_environment() {
    for workflow in GENERATED_WORKFLOWS {
        let text = workspace_file(workflow);
        let parsed = jackin_xtask::workflow_yaml::parse(text.as_bytes()).expect("workflow YAML");
        assert_installer_environments(&parsed, workflow);
    }
}

#[test]
fn installer_environment_checks_expand_step_and_environment_aliases() {
    let good = jackin_xtask::workflow_yaml::parse(
        br"
jobs:
  first:
    steps:
      - &installer
        name: Setup Mise
        uses: jdx/mise-action@fixed
  second:
    steps:
      - *installer
",
    )
    .expect("aliased installer");
    assert_eq!(assert_installer_environments(&good, "fixture"), 2);
    let bad = jackin_xtask::workflow_yaml::parse(
        br"
jobs:
  definition:
    steps:
      - name: Define an environment without installing Mise
        env: &forbidden
          RUSTC_WRAPPER: sccache
  installer:
    env: *forbidden
    steps:
      - uses: jdx/mise-action@fixed
",
    )
    .expect("aliased environment");
    std::panic::catch_unwind(|| assert_installer_environments(&bad, "fixture"))
        .expect_err("aliased forbidden environment must be rejected");
}

#[test]
fn jactionlint_uses_native_config_strict_flags_and_sorted_complete_inventory() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workflows = temp.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(
        temp.path().join(".github/jactionlint.yaml"),
        "profile: correctness\n",
    )
    .unwrap();
    let action = temp.path().join(".github/actions/build/action.yml");
    fs::create_dir_all(action.parent().unwrap()).unwrap();
    fs::create_dir_all(temp.path().join(".github/actions/other")).unwrap();
    for name in ["z.yml", "a.yml", "a.yaml", "x.y.extra.ml", "README.md"] {
        fs::write(workflows.join(name), "").unwrap();
    }
    fs::write(&action, "").unwrap();
    fs::write(
        temp.path().join(".github/actions/build/action.extra.yml"),
        "",
    )
    .unwrap();
    fs::write(
        temp.path().join(".github/actions/build/action.y.extra.ml"),
        "",
    )
    .unwrap();
    fs::write(temp.path().join(".github/actions/other/README.md"), "").unwrap();
    fs::write(temp.path().join(".github/dependabot.yaml"), "").unwrap();
    fs::write(temp.path().join(".github/dependabot.y.extra.ml"), "").unwrap();
    fs::write(temp.path().join(".github/dependabot.extra.yml"), "").unwrap();
    fs::write(temp.path().join(".github/dependabot.md"), "").unwrap();
    let actual = super::workflow_lint::jactionlint_args(temp.path()).unwrap();
    let expected = [
        "--config-file",
        ".github/jactionlint.yaml",
        "--format",
        "oneline",
        "--strict-exit",
        "--no-online",
        "--shellcheck=shellcheck",
        "--pyflakes=",
        "--no-color",
        ".github/actions/build/action.y.extra.ml",
        ".github/actions/build/action.yml",
        ".github/dependabot.y.extra.ml",
        ".github/dependabot.yaml",
        ".github/workflows/a.yaml",
        ".github/workflows/a.yml",
        ".github/workflows/x.y.extra.ml",
        ".github/workflows/z.yml",
    ];
    assert_eq!(actual, expected.map(std::ffi::OsString::from));
}

#[test]
fn jactionlint_refuses_missing_or_empty_complete_inventory() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join(".github")).unwrap();
    fs::write(
        temp.path().join(".github/jactionlint.yaml"),
        "profile: correctness\n",
    )
    .unwrap();
    let missing = super::workflow_lint::jactionlint_args(temp.path())
        .expect_err("missing workflows must reject");
    assert!(missing.to_string().contains(".github/workflows"));
    fs::create_dir_all(temp.path().join(".github/workflows")).unwrap();
    let empty = super::workflow_lint::jactionlint_args(temp.path())
        .expect_err("empty workflows must reject");
    assert!(
        empty
            .to_string()
            .contains("no workflow, composite-action, or Dependabot")
    );
}

#[test]
fn jactionlint_requires_its_generated_config_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join(".github/workflows")).unwrap();

    let error = super::workflow_lint::jactionlint_args(temp.path())
        .expect_err("missing config must reject");
    assert!(error.to_string().contains("jactionlint config"));
}

#[test]
fn jactionlint_rejects_a_non_directory_inventory_root() {
    let temp = tempfile::tempdir().expect("tempdir");
    let github = temp.path().join(".github");
    fs::create_dir_all(github.join("workflows")).unwrap();
    fs::write(github.join("jactionlint.yaml"), "profile: correctness\n").unwrap();
    fs::write(github.join("actions"), "not a directory").unwrap();
    fs::write(github.join("workflows/ci.yml"), "").unwrap();

    let error = super::workflow_lint::jactionlint_args(temp.path())
        .expect_err("existing non-directory class root must reject");
    assert!(
        error
            .to_string()
            .contains(".github/actions is not a real directory")
    );
}

#[test]
fn jactionlint_rejects_symlinked_inventory_roots() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let external = temp.path().join("external");
    fs::create_dir_all(external.join("workflows")).unwrap();
    fs::write(external.join("jactionlint.yaml"), "profile: correctness\n").unwrap();
    fs::write(external.join("workflows/ci.yml"), "").unwrap();
    symlink(&external, temp.path().join(".github")).unwrap();
    assert!(
        super::workflow_lint::jactionlint_args(temp.path())
            .unwrap_err()
            .to_string()
            .contains(".github is not a real directory")
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let github = temp.path().join(".github");
    let external = temp.path().join("external");
    fs::create_dir_all(&github).unwrap();
    fs::create_dir_all(&external).unwrap();
    fs::write(github.join("jactionlint.yaml"), "profile: correctness\n").unwrap();
    fs::write(external.join("ci.yml"), "").unwrap();
    symlink(&external, github.join("workflows")).unwrap();
    assert!(
        super::workflow_lint::jactionlint_args(temp.path())
            .unwrap_err()
            .to_string()
            .contains(".github/workflows is not a real directory")
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let github = temp.path().join(".github");
    let external = temp.path().join("external");
    fs::create_dir_all(github.join("workflows")).unwrap();
    fs::create_dir_all(&external).unwrap();
    fs::write(github.join("jactionlint.yaml"), "profile: correctness\n").unwrap();
    fs::write(github.join("workflows/ci.yml"), "").unwrap();
    symlink(&external, github.join("actions")).unwrap();
    assert!(
        super::workflow_lint::jactionlint_args(temp.path())
            .unwrap_err()
            .to_string()
            .contains(".github/actions is not a real directory")
    );
}

#[test]
fn jactionlint_rejects_symlinks_inside_workflow_action_and_dependabot_inventory() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let github = temp.path().join(".github");
    let workflows = github.join("workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(github.join("jactionlint.yaml"), "profile: correctness\n").unwrap();
    fs::write(temp.path().join("external.yml"), "").unwrap();
    symlink(
        temp.path().join("external.yml"),
        workflows.join("unrelated.md"),
    )
    .unwrap();
    assert!(
        super::workflow_lint::jactionlint_args(temp.path())
            .unwrap_err()
            .to_string()
            .contains("symlink is not allowed in lint inventory")
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let github = temp.path().join(".github");
    let actions = github.join("actions/build");
    let workflows = github.join("workflows");
    fs::create_dir_all(&actions).unwrap();
    fs::create_dir_all(&workflows).unwrap();
    fs::write(github.join("jactionlint.yaml"), "profile: correctness\n").unwrap();
    fs::write(workflows.join("ci.yml"), "").unwrap();
    fs::write(temp.path().join("external.yml"), "").unwrap();
    symlink(
        temp.path().join("external.yml"),
        actions.join("unrelated.md"),
    )
    .unwrap();
    assert!(
        super::workflow_lint::jactionlint_args(temp.path())
            .unwrap_err()
            .to_string()
            .contains("symlink is not allowed in lint inventory")
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let github = temp.path().join(".github");
    let workflows = github.join("workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(github.join("jactionlint.yaml"), "profile: correctness\n").unwrap();
    fs::write(workflows.join("ci.yml"), "").unwrap();
    fs::write(temp.path().join("external.yml"), "").unwrap();
    symlink(
        temp.path().join("external.yml"),
        github.join("dependabot.yml"),
    )
    .unwrap();
    assert!(
        super::workflow_lint::jactionlint_args(temp.path())
            .unwrap_err()
            .to_string()
            .contains("symlink is not allowed in lint inventory")
    );
}
