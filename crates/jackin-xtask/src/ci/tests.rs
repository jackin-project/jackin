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

/// Post-#1110 CI surface: velnor-actions 0.1.0 `ci.yml` only. The legacy
/// per-lane workflows were deleted by main commit 6c389d38e, and the two
/// advisory evidence-observer stubs were dropped by the all-branches
/// consolidation because the pinned renderer rejects non-generated files.
const GENERATED_WORKFLOWS: [&str; 1] = [".github/workflows/ci.yml"];

#[test]
fn sccache_is_absent_from_generated_workflow_environment() {
    for workflow in GENERATED_WORKFLOWS {
        let text = workspace_file(workflow);
        assert_no_sccache_env(&text, workflow);
    }
}

#[test]
fn sccache_is_absent_from_installer_environment() {
    for workflow in GENERATED_WORKFLOWS {
        let text = workspace_file(workflow);
        // velnor-actions emits "Setup Mise" (kept tolerant of "Set up Mise").
        let setup = ["- name: Set up Mise", "- name: Setup Mise"]
            .iter()
            .filter_map(|marker| text.find(marker))
            .min()
            .unwrap_or_else(|| panic!("{workflow} has no Mise installer"));
        assert_no_sccache_env(&text[..setup], &format!("{workflow} installer prefix"));
    }
}

// NOTE: the desktop merge/scheduled cadence contract test was removed with the
// post-#1110 CI surface (main commit 6c389d38e deleted desktop-merge.yml,
// desktop-scheduled.yml, ci-main.yml, ci-pr.yml, and ci-unit-swift.yml; no
// surviving workflow invokes desktop tasks, runs macOS, or mentions
// plan_digest / SELECTION_PLAN_DIGEST / product_transport_ready). The surviving
// mise task-graph assertions live in
// desktop::tests::cadence_tasks_define_the_canonical_graph.
