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

#[test]
fn sccache_is_absent_from_generated_workflow_environment() {
    for workflow in [
        ".github/workflows/ci-unit-swift.yml",
        ".github/workflows/desktop-merge.yml",
        ".github/workflows/desktop-scheduled.yml",
        ".github/workflows/ci-unit-rust.yml",
        ".github/workflows/release.yml",
    ] {
        let text = workspace_file(workflow);
        assert_no_sccache_env(&text, workflow);
    }
}

#[test]
fn sccache_is_absent_from_installer_environment() {
    for workflow in [
        ".github/workflows/ci-unit-swift.yml",
        ".github/workflows/desktop-merge.yml",
        ".github/workflows/desktop-scheduled.yml",
    ] {
        let text = workspace_file(workflow);
        let setup = text
            .find("- name: Set up Mise")
            .unwrap_or_else(|| panic!("{workflow} has no Mise installer"));
        assert_no_sccache_env(&text[..setup], &format!("{workflow} installer prefix"));
    }
}

#[test]
fn desktop_merge_and_scheduled_contracts_preserve_cadence_concurrency_and_gates() {
    let merge = workspace_file(".github/workflows/desktop-merge.yml");
    assert!(merge.contains("on:\n  push:\n    branches: [main]\n  workflow_dispatch:"));
    assert!(!merge.contains("  schedule:"));
    assert!(merge.contains("group: desktop-merge-${{ github.repository }}-${{ github.ref }}"));
    assert!(merge.contains("cancel-in-progress: true"));
    assert_eq!(
        merge.matches("run: mise run desktop-merge").count(),
        1,
        "desktop merge must have one generated task caller"
    );
    let generated_workflow_dir = workspace_root().join(".github/workflows");
    let direct_callers = fs::read_dir(generated_workflow_dir)
        .expect("generated workflow directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("yml"))
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter(|workflow| workflow.contains("run: mise run desktop-merge"))
        .count();
    assert_eq!(
        direct_callers, 1,
        "desktop merge must have one generated workflow caller"
    );

    let scheduled = workspace_file(".github/workflows/desktop-scheduled.yml");
    assert!(scheduled.contains("  schedule:\n    - cron: \"41 4 * * 1\""));
    assert!(
        scheduled.contains("group: desktop-scheduled-${{ github.repository }}-${{ github.ref }}")
    );
    assert!(scheduled.contains("cancel-in-progress: true"));
    assert!(!scheduled.contains("  pull_request:"));

    for workflow in [
        ".github/workflows/ci-main.yml",
        ".github/workflows/ci-pr.yml",
    ] {
        let text = workspace_file(workflow);
        assert!(
            text.contains("plan_digest"),
            "{workflow} lost plan digest wiring"
        );
    }

    let swift = workspace_file(".github/workflows/ci-unit-swift.yml");
    assert!(swift.contains("product_transport_ready:"));
    assert!(swift.contains("SELECTION_PLAN_DIGEST:"));

    let mise = workspace_file("mise.toml");
    for required in [
        "[tasks.desktop-ci]",
        "mise run desktop-bindings-check",
        "mise run desktop-generate",
        "mise run desktop-format-check",
        "mise run desktop-lint",
        "mise run desktop-test",
        "mise run desktop-build",
        "cargo xtask desktop test-swift",
        "mise run desktop-verify",
        "[tasks.desktop-merge]",
        "mise run desktop-ci",
        "mise run desktop-test-ui",
    ] {
        assert!(mise.contains(required), "desktop graph lost `{required}`");
    }
}
