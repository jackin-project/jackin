// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `migrations`.
use super::*;
use tempfile::tempdir;

#[test]
fn migrates_missing_manifest_version() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("jackin.role.toml");
    std::fs::write(&path, "# keep me\ndockerfile = \"Dockerfile\"\n").unwrap();

    let (old, new) = migrate_manifest_file(&path).unwrap().unwrap();
    let out = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&out).unwrap();

    assert_eq!(old, "legacy");
    assert_eq!(new, "v1alpha6");
    assert_eq!(parsed["version"].as_str().unwrap(), "v1alpha6");
    assert!(out.starts_with("version = \"v1alpha6\""), "{out}");
    assert!(out.contains("# keep me"), "{out}");
}

#[test]
fn migrates_v1alpha1_manifest_to_current() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("jackin.role.toml");
    std::fs::write(
        &path,
        "version = \"v1alpha1\"\ndockerfile = \"Dockerfile\"\n",
    )
    .unwrap();

    let (old, new) = migrate_manifest_file(&path).unwrap().unwrap();
    let out = std::fs::read_to_string(&path).unwrap();

    assert_eq!(old, "v1alpha1");
    assert_eq!(new, "v1alpha6");
    assert!(out.starts_with("version = \"v1alpha6\""), "{out}");
}

#[test]
fn current_manifest_migration_is_noop() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("jackin.role.toml");
    let manifest = "version = \"v1alpha6\"\ndockerfile = \"Dockerfile\"\n";
    std::fs::write(&path, manifest).unwrap();

    assert!(migrate_manifest_file(&path).unwrap().is_none());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), manifest);
}

#[test]
fn rejects_newer_manifest_version() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("jackin.role.toml");
    std::fs::write(
        &path,
        "version = \"v2alpha1\"\ndockerfile = \"Dockerfile\"\n",
    )
    .unwrap();

    let err = migrate_manifest_file(&path).unwrap_err();
    assert!(
        err.to_string().contains("only understands up to v1alpha6"),
        "{err}"
    );
}

#[test]
fn validate_manifest_version_accepts_current() {
    let doc: DocumentMut = "version = \"v1alpha6\"\n".parse().unwrap();
    validate_manifest_version(&doc).unwrap();
}

#[test]
fn validate_manifest_version_accepts_legacy() {
    let doc: DocumentMut = "dockerfile = \"Dockerfile\"\n".parse().unwrap();
    let version = validate_manifest_version(&doc).unwrap();
    assert_eq!(version.to_string(), "legacy");
}

#[test]
fn validate_manifest_version_rejects_newer() {
    let doc: DocumentMut = "version = \"v2alpha1\"\n".parse().unwrap();
    let err = validate_manifest_version(&doc).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("only understands up to v1alpha6"), "{msg}");
}

#[test]
fn manifest_migrations_chain_reaches_current() {
    // Production registry must form a contiguous chain from `legacy` to
    // CURRENT_MANIFEST_VERSION. The shared helper catches typos,
    // missing middle steps, backward steps, cycles, and duplicate
    // `from` forks on every CI run.
    jackin_config::migrations::assert_registry_chain(MANIFEST_MIGRATIONS, CURRENT_MANIFEST_VERSION);
}
