use std::fs;

use super::parse_capsule_export;

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
