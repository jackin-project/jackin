// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use tempfile::tempdir;

use super::{
    Artifact, excluded, has_exact_local_target, has_reusable_local_target, key_for_package,
    reusable_paths,
};

#[test]
fn resolves_exact_key_without_workflow_expression_parsing() {
    assert_eq!(
        key_for_package(r#"{"jackin-xtask":"source-key"}"#, "jackin-xtask").unwrap(),
        "source-key"
    );
    key_for_package("{}", "jackin-xtask").unwrap_err();
}

#[test]
fn reusable_paths_keep_outputs_and_drop_transport_state() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir_all(target.join("debug/deps")).unwrap();
    fs::create_dir_all(target.join("debug/incremental/state")).unwrap();
    fs::create_dir_all(target.join("nextest/default")).unwrap();
    fs::write(target.join("debug/deps/libexample.rlib"), b"output").unwrap();
    fs::write(target.join("debug/deps/example-abc123"), b"test binary").unwrap();
    fs::write(target.join("debug/jackin"), b"application binary").unwrap();
    fs::write(target.join("debug/incremental/state/data"), b"state").unwrap();
    fs::write(target.join("nextest/default/junit.xml"), b"report").unwrap();

    let mut paths = reusable_paths(&target).unwrap();
    paths.sort_unstable();

    assert_eq!(
        paths,
        vec![
            target.join("debug/deps/example-abc123"),
            target.join("debug/deps/libexample.rlib"),
            target.join("debug/jackin"),
        ]
    );
}

#[test]
fn local_target_requires_rustc_metadata_and_an_rlib() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir_all(target.join("debug/deps")).unwrap();
    assert!(!has_reusable_local_target(&target, "").unwrap());

    fs::write(target.join(".rustc_info.json"), b"{}").unwrap();
    assert!(!has_reusable_local_target(&target, "").unwrap());

    fs::write(target.join("debug/deps/libexample.rlib"), b"output").unwrap();
    assert!(has_reusable_local_target(&target, "").unwrap());
}

#[test]
fn local_target_requires_the_exact_source_key() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir_all(target.join("debug/deps")).unwrap();
    fs::write(target.join(".rustc_info.json"), b"{}").unwrap();
    fs::write(target.join("debug/deps/libexample.rlib"), b"output").unwrap();

    assert!(!has_exact_local_target(&target, "", "current").unwrap());
    fs::write(target.join(".ci-source-key"), b"stale\n").unwrap();
    assert!(!has_exact_local_target(&target, "", "current").unwrap());
    fs::write(target.join(".ci-source-key"), b"current\n").unwrap();
    assert!(has_exact_local_target(&target, "", "current").unwrap());
}

#[test]
fn fuzzing_crate_local_target_requires_release_fuzz_binary() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir_all(target.join("debug/deps")).unwrap();
    fs::write(target.join(".rustc_info.json"), b"{}").unwrap();
    fs::write(target.join("debug/deps/libexample.rlib"), b"output").unwrap();
    assert!(!has_reusable_local_target(&target, "jackin-env").unwrap());

    let release = target.join("x86_64-unknown-linux-gnu/release");
    fs::create_dir_all(&release).unwrap();
    fs::write(release.join("env_resolve"), b"fuzzer").unwrap();
    assert!(has_reusable_local_target(&target, "jackin-env").unwrap());
}

#[test]
fn rejects_empty_or_expired_target_artifacts() {
    let artifact = |size_in_bytes, expired| Artifact {
        id: 1,
        expired,
        created_at: "2026-07-17T00:00:00Z".to_owned(),
        size_in_bytes,
    };

    assert!(!artifact(285, false).reusable());
    assert!(!artifact(2 * 1024 * 1024, true).reusable());
    assert!(artifact(2 * 1024 * 1024, false).reusable());
}

#[test]
fn transport_exclusions_are_semantic_directories() {
    assert!(excluded("debug/incremental/object".as_ref()));
    assert!(excluded("nextest/ci/junit.xml".as_ref()));
    assert!(excluded("telemetry-volume-ratchet.json".as_ref()));
    assert!(!excluded("debug/deps/libexample.rlib".as_ref()));
}
