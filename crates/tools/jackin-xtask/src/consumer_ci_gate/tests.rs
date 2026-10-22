// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use anyhow::Result;
use std::fs;

fn target_fixture() -> Result<(tempfile::TempDir, TargetDirectory)> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("repo");
    let managed = temp.path().join("cache/targets");
    let checkout = managed.join("v1/checkout");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&checkout)?;
    std::os::unix::fs::symlink("../cache/targets/v1/checkout", root.join("target"))?;
    let repository = evidence::RepositoryDirectory::open(&root)?;
    let target = TargetDirectory::open_with_target_root(repository, &managed.canonicalize()?)?;
    Ok((temp, target))
}

#[test]
fn cli_lane_spellings_are_frozen() {
    let variants = Lane::value_variants()
        .iter()
        .map(|lane| {
            lane.to_possible_value()
                .expect("lane name")
                .get_name()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        variants,
        [
            "broker-fixtures",
            "orb-stack-broker-fixtures",
            "native",
            "native-ui",
            "production-capsule",
        ]
    );
}

#[test]
fn shell_export_is_parsed_without_evaluating_shell_code() -> Result<()> {
    assert_eq!(
        lanes::parse_capsule_export("export JACKIN_CAPSULE_BIN='/tmp/a'\\''b'\n")?,
        "/tmp/a'b"
    );
    for input in [
        "export JACKIN_CAPSULE_BIN=/tmp/a",
        "export JACKIN_CAPSULE_BIN='relative'",
        "export JACKIN_CAPSULE_BIN='/tmp/a'\necho poison",
        "",
    ] {
        lanes::parse_capsule_export(input).unwrap_err();
    }
    Ok(())
}

#[test]
fn report_requires_frozen_repository_relative_path_and_fresh_owned_file() -> Result<()> {
    for invalid in [
        "",
        "/tmp/report.json",
        "../report.json",
        "target/consumer-ci/../report.json",
        "target/consumer-ci/report.txt",
    ] {
        report_names(Path::new(invalid)).unwrap_err();
    }
    let (temp, target) = target_fixture()?;
    let (name, detail) = report_names(Path::new("target/consumer-ci/result.json"))?;
    target.preflight_report(&name)?;
    target.preflight_report(&detail)?;
    target.write_report(&name, b"first")?;
    target.preflight_report(&name).unwrap_err();
    target.write_report(&name, b"replacement").unwrap_err();
    assert_eq!(fs::read(target.report_directory().join(name))?, b"first");
    assert!(!temp.path().join("outside/result.json").exists());
    Ok(())
}

#[test]
fn repository_artifact_digest_changes_with_contents() -> Result<()> {
    let (temp, target) = target_fixture()?;
    let source = target.repository().root().join("capsule");
    fs::write(&source, b"first binary")?;
    let first = artifact(target.repository(), &target, Path::new("capsule"))?;
    fs::write(&source, b"another binary")?;
    let second = artifact(target.repository(), &target, Path::new("capsule"))?;
    assert_ne!(first.sha256, second.sha256);
    assert_eq!(first.path, "capsule");
    assert!(temp.path().exists());
    Ok(())
}

#[test]
fn capsule_requires_elf_and_hashes_the_same_open_file() -> Result<()> {
    let (_temp, target) = target_fixture()?;
    let path = target
        .report_directory()
        .parent()
        .expect("target parent")
        .join("capsule");
    fs::write(&path, b"\x7fELFproduction capsule")?;
    let relative = Path::new("target/capsule");
    let validated = capsule_artifact(target.repository(), &target, relative)?;
    assert_eq!(validated.path, "target/capsule");
    assert_eq!(
        validated.sha256,
        artifact(target.repository(), &target, relative)?.sha256
    );

    fs::write(&path, b"not an ELF file")?;
    capsule_artifact(target.repository(), &target, relative).unwrap_err();
    Ok(())
}

#[test]
fn artifact_rejects_files_with_external_hard_links() -> Result<()> {
    let (temp, target) = target_fixture()?;
    let source = target.repository().root().join("capsule");
    let outside = temp.path().join("outside-capsule");
    fs::write(&source, b"hard-linked artifact")?;
    fs::hard_link(&source, &outside)?;
    artifact(target.repository(), &target, Path::new("capsule")).unwrap_err();
    Ok(())
}

#[cfg(unix)]
#[test]
fn artifact_rejects_symlinked_ancestors_and_files() -> Result<()> {
    let (temp, target) = target_fixture()?;
    let outside = temp.path().join("outside");
    fs::create_dir(&outside)?;
    fs::write(outside.join("capsule"), b"outside artifact")?;
    std::os::unix::fs::symlink(&outside, target.repository().root().join("linked-dir"))?;
    std::os::unix::fs::symlink(
        outside.join("capsule"),
        target.repository().root().join("linked-file"),
    )?;
    artifact(
        target.repository(),
        &target,
        Path::new("linked-dir/capsule"),
    )
    .unwrap_err();
    artifact(target.repository(), &target, Path::new("linked-file")).unwrap_err();
    Ok(())
}

#[test]
fn broker_obligation_is_exact_and_excludes_child_process_helper() {
    let names = broker_cases();
    assert_eq!(names.len(), 11);
    assert!(!names.iter().any(|name| name.contains("usage_broker_child")));
    assert_eq!(
        names
            .iter()
            .filter(|name| name.starts_with("docker::"))
            .count(),
        8
    );
}
