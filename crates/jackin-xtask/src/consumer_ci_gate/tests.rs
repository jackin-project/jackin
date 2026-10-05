// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn shell_export_is_parsed_without_evaluating_shell_code() -> Result<()> {
    assert_eq!(
        parse_capsule_export("export JACKIN_CAPSULE_BIN='/tmp/a'\\''b'\n")?,
        "/tmp/a'b"
    );
    for input in [
        "export JACKIN_CAPSULE_BIN=/tmp/a",
        "export JACKIN_CAPSULE_BIN='relative'",
        "export JACKIN_CAPSULE_BIN='/tmp/a'\necho poison",
        "",
    ] {
        assert!(parse_capsule_export(input).is_err());
    }
    Ok(())
}

#[test]
fn report_requires_fresh_normal_relative_path() -> Result<()> {
    let directory = tempfile::tempdir()?;
    for path in ["", "/tmp/report.json", "../report.json", "x/../report.json"] {
        assert!(fresh_destination(directory.path(), Path::new(path)).is_err());
    }
    let relative = Path::new("target/consumer-ci/report.json");
    let path = fresh_destination(directory.path(), relative)?;
    write_fresh(directory.path(), &path, b"first")?;
    assert!(fresh_destination(directory.path(), relative).is_err());
    assert!(write_fresh(directory.path(), &path, b"replacement").is_err());
    assert_eq!(fs::read(path)?, b"first");
    Ok(())
}

#[cfg(unix)]
#[test]
fn report_rejects_symlink_ancestor() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::create_dir(directory.path().join("target"))?;
    std::os::unix::fs::symlink(outside.path(), directory.path().join("target/consumer-ci"))?;
    assert!(
        fresh_destination(
            directory.path(),
            Path::new("target/consumer-ci/report.json")
        )
        .is_err()
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn report_write_rejects_ancestor_replaced_after_preflight() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let report = fresh_destination(
        directory.path(),
        Path::new("target/consumer-ci/report.json"),
    )?;
    fs::create_dir(directory.path().join("target"))?;
    std::os::unix::fs::symlink(outside.path(), directory.path().join("target/consumer-ci"))?;
    assert!(write_fresh(directory.path(), &report, b"must remain private").is_err());
    assert!(!outside.path().join("report.json").exists());
    Ok(())
}

#[test]
fn artifact_digest_changes_when_production_binary_changes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("capsule");
    fs::write(&path, b"first binary")?;
    let first = artifact(directory.path(), &path)?;
    fs::write(&path, b"another binary")?;
    let second = artifact(directory.path(), &path)?;
    assert_ne!(first.sha256, second.sha256);
    assert_eq!(first.path, "capsule");
    Ok(())
}

#[test]
fn capsule_artifact_validates_elf_and_hashes_same_file() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("capsule");
    fs::write(&path, b"\x7fELFproduction capsule")?;
    let validated = capsule_artifact(directory.path(), &path)?;
    assert_eq!(validated.path, "capsule");
    assert_eq!(validated.sha256, artifact(directory.path(), &path)?.sha256);

    fs::write(&path, b"not an ELF file")?;
    assert!(capsule_artifact(directory.path(), &path).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn artifact_rejects_symlinked_ancestor_and_file() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::write(outside.path().join("capsule"), b"outside artifact")?;
    std::os::unix::fs::symlink(outside.path(), directory.path().join("linked-dir"))?;
    std::os::unix::fs::symlink(
        outside.path().join("capsule"),
        directory.path().join("linked-file"),
    )?;

    assert!(
        artifact(
            directory.path(),
            &directory.path().join("linked-dir/capsule")
        )
        .is_err()
    );
    assert!(
        capsule_artifact(
            directory.path(),
            &directory.path().join("linked-dir/capsule")
        )
        .is_err()
    );
    assert!(artifact(directory.path(), &directory.path().join("linked-file")).is_err());
    assert!(capsule_artifact(directory.path(), &directory.path().join("linked-file")).is_err());
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn artifact_rejects_fifo_without_waiting_for_a_writer() -> Result<()> {
    use std::{sync::mpsc, thread, time::Duration};

    let directory = tempfile::tempdir()?;
    let root_fd = fs::File::open(directory.path())?;
    rustix::fs::mkfifoat(
        &root_fd,
        "capsule",
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?;

    let root = directory.path().to_owned();
    let fifo = root.join("capsule");
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = sender.send(capsule_artifact(&root, &fifo).is_err());
    });
    assert!(
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("artifact hashing blocked on FIFO"),
        "FIFO artifact was accepted"
    );
    Ok(())
}

#[test]
fn broker_obligation_excludes_child_process_helper_and_has_exact_inventory() {
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
