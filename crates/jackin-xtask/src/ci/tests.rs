use std::fs;

use super::{CiArgs, build_steps, parse_capsule_export};

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
fn msrv_step_uses_an_isolated_target_directory() {
    let root = crate::docs::repo_root().expect("repo root");
    let args = CiArgs {
        fast: false,
        e2e: false,
        base: "origin/main".to_owned(),
        only: vec!["msrv".to_owned()],
    };
    let steps = build_steps(&root, &args).expect("CI steps");
    let msrv = steps
        .iter()
        .find(|step| step.name == "cargo msrv")
        .expect("MSRV step");

    assert_eq!(
        msrv.env.get("CARGO_TARGET_DIR"),
        Some(&root.join("target/msrv").into_os_string())
    );
}
