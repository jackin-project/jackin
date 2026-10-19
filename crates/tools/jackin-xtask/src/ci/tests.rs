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
        br#"
jobs:
  first:
    steps:
      - &installer
        name: Setup Mise
        uses: jdx/mise-action@fixed
  second:
    steps:
      - *installer
"#,
    )
    .expect("aliased installer");
    assert_eq!(assert_installer_environments(&good, "fixture"), 2);
    let bad = jackin_xtask::workflow_yaml::parse(
        br#"
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
"#,
    )
    .expect("aliased environment");
    assert!(std::panic::catch_unwind(|| assert_installer_environments(&bad, "fixture")).is_err());
}
