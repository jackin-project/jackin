// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use anyhow::Result;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

fn fixture() -> Result<(tempfile::TempDir, RepositoryDirectory, PathBuf)> {
    let temp = tempfile::tempdir()?;
    let repo = temp.path().join("repo");
    let managed = temp.path().join("cache/targets");
    let target = managed.join("v1/checkout-id");
    fs::create_dir_all(&repo)?;
    fs::create_dir_all(&target)?;
    fs::canonicalize(&managed)?;
    symlink("../cache/targets/v1/checkout-id", repo.join("target"))?;
    let repository = RepositoryDirectory::open(&repo)?;
    Ok((temp, repository, managed.canonicalize()?))
}

#[test]
fn managed_target_requires_exact_v1_target_and_owns_fresh_reports() -> Result<()> {
    let (_temp, repository, managed) = fixture()?;
    let target = TargetDirectory::open_with_target_root(repository, &managed)?;
    let report = OsStr::new("broker.json");
    target.preflight_report(report)?;
    let run = target.create_run_directory()?;
    run.write_file(&target, "junit.xml", b"<testsuites/>")?;
    assert_eq!(
        run.read_relative_file(&target, Path::new("junit.xml"), 1024)?,
        b"<testsuites/>"
    );
    target.write_report(report, b"first")?;
    assert!(target.preflight_report(report).is_err());
    assert!(target.write_report(report, b"second").is_err());
    target.verify_link()?;
    assert!(run.path().starts_with(target.report_directory()));
    let capsule = run
        .path()
        .join("capsule-home/cache/jackin-capsule/1/linux-x86_64/jackin-capsule");
    assert_eq!(
        target.artifact_path(&capsule)?,
        Path::new("target")
            .join("consumer-ci")
            .join(run.id()?)
            .join("capsule-home/cache/jackin-capsule/1/linux-x86_64/jackin-capsule")
    );
    target
        .artifact_path(Path::new("/tmp/outside-capsule"))
        .unwrap_err();
    Ok(())
}

#[test]
fn replacing_visible_report_directory_with_real_directory_rejects_receipts() -> Result<()> {
    let (_temp, repository, managed) = fixture()?;
    let target = TargetDirectory::open_with_target_root(repository, &managed)?;
    let run = target.create_run_directory()?;
    let reports = target.report_directory().to_path_buf();
    let detached = reports.with_file_name("consumer-ci-detached");
    fs::rename(&reports, &detached)?;
    fs::create_dir(&reports)?;
    fs::set_permissions(&reports, fs::Permissions::from_mode(0o700))?;

    assert!(target.verify_link().is_err());
    assert!(
        target
            .write_report(OsStr::new("receipt.json"), b"passed")
            .is_err()
    );
    assert!(
        run.write_file(&target, "junit.xml", b"<testsuites/>")
            .is_err()
    );
    assert!(!reports.join("receipt.json").exists());
    assert!(!detached.join("receipt.json").exists());
    Ok(())
}

#[test]
fn replacing_visible_report_directory_with_symlink_is_rejected() -> Result<()> {
    let (_temp, repository, managed) = fixture()?;
    let target = TargetDirectory::open_with_target_root(repository, &managed)?;
    let reports = target.report_directory().to_path_buf();
    let detached = reports.with_file_name("consumer-ci-detached");
    let outside = reports.with_file_name("outside");
    fs::create_dir(&outside)?;
    fs::rename(&reports, &detached)?;
    symlink(&outside, &reports)?;

    assert!(target.verify_link().is_err());
    assert!(
        target
            .write_report(OsStr::new("receipt.json"), b"passed")
            .is_err()
    );
    assert!(!outside.join("receipt.json").exists());
    Ok(())
}

#[test]
fn replacing_visible_run_directory_with_stale_real_directory_is_rejected() -> Result<()> {
    let (_temp, repository, managed) = fixture()?;
    let target = TargetDirectory::open_with_target_root(repository, &managed)?;
    let run = target.create_run_directory()?;
    let visible = run.path().to_path_buf();
    let detached = visible.with_file_name("run-detached");
    fs::rename(&visible, &detached)?;
    fs::create_dir(&visible)?;
    fs::set_permissions(&visible, fs::Permissions::from_mode(0o700))?;
    fs::write(visible.join("junit.xml"), b"<testsuites/>")?;

    assert!(target.verify_run_directory(&run).is_err());
    run.read_relative_file(&target, Path::new("junit.xml"), 1024)
        .unwrap_err();
    assert!(
        run.write_file(&target, "unexpected.xml", b"passed")
            .is_err()
    );
    assert!(!detached.join("unexpected.xml").exists());
    Ok(())
}

#[test]
fn replacing_visible_run_directory_with_symlink_is_rejected() -> Result<()> {
    let (temp, repository, managed) = fixture()?;
    let target = TargetDirectory::open_with_target_root(repository, &managed)?;
    let run = target.create_run_directory()?;
    let visible = run.path().to_path_buf();
    let detached = visible.with_file_name("run-detached");
    let outside = temp.path().join("outside-run");
    fs::create_dir(&outside)?;
    fs::write(outside.join("junit.xml"), b"<testsuites/>")?;
    fs::rename(&visible, &detached)?;
    symlink(&outside, &visible)?;

    assert!(target.verify_run_directory(&run).is_err());
    run.read_relative_file(&target, Path::new("junit.xml"), 1024)
        .unwrap_err();
    Ok(())
}

#[test]
fn replacing_managed_target_inode_after_open_is_rejected() -> Result<()> {
    let (temp, repository, managed) = fixture()?;
    let target = TargetDirectory::open_with_target_root(repository, &managed)?;
    let target_path = managed.join("v1/checkout-id");
    let detached = temp.path().join("detached-target");
    fs::rename(&target_path, &detached)?;
    fs::create_dir(&target_path)?;
    fs::create_dir(target_path.join("consumer-ci"))?;
    fs::set_permissions(
        target_path.join("consumer-ci"),
        fs::Permissions::from_mode(0o700),
    )?;

    assert!(target.verify_link().is_err());
    assert!(
        target
            .write_report(OsStr::new("receipt.json"), b"passed")
            .is_err()
    );
    assert!(!target_path.join("consumer-ci/receipt.json").exists());
    Ok(())
}

#[test]
fn replacing_checkout_target_link_after_open_is_rejected() -> Result<()> {
    let (temp, repository, managed) = fixture()?;
    let target = TargetDirectory::open_with_target_root(repository, &managed)?;
    let replacement = temp.path().join("replacement/v1/checkout-id");
    fs::create_dir_all(&replacement)?;
    fs::create_dir(replacement.join("consumer-ci"))?;
    fs::set_permissions(
        replacement.join("consumer-ci"),
        fs::Permissions::from_mode(0o700),
    )?;
    let checkout_link = temp.path().join("repo/target");
    fs::remove_file(&checkout_link)?;
    symlink(&replacement, checkout_link)?;

    assert!(target.verify_link().is_err());
    assert!(
        target
            .write_report(OsStr::new("receipt.json"), b"passed")
            .is_err()
    );
    Ok(())
}

#[test]
fn managed_report_directory_must_remain_owner_only() -> Result<()> {
    let (_temp, repository, managed) = fixture()?;
    let target = TargetDirectory::open_with_target_root(repository, &managed)?;
    fs::set_permissions(target.report_directory(), fs::Permissions::from_mode(0o755))?;
    target
        .preflight_report(OsStr::new("report.json"))
        .unwrap_err();
    fs::set_permissions(target.report_directory(), fs::Permissions::from_mode(0o700))?;
    target.preflight_report(OsStr::new("report.json"))?;
    Ok(())
}

#[test]
fn managed_target_rejects_escape_symlink_and_wrong_version_namespace() -> Result<()> {
    let (temp, repository, managed) = fixture()?;
    let other = temp.path().join("other/v1/id");
    fs::create_dir_all(&other)?;
    fs::remove_file(repository.root().join("target"))?;
    symlink(other, repository.root().join("target"))?;
    assert!(TargetDirectory::open_with_target_root(repository, &managed).is_err());

    let reports = temp.path().join("cache/targets/v1/checkout-id/consumer-ci");
    fs::create_dir_all(reports.parent().expect("target parent"))?;
    let outside = temp.path().join("outside");
    fs::create_dir(&outside)?;
    symlink(&outside, &reports)?;
    let valid_repo = temp.path().join("valid-repo");
    fs::create_dir_all(&valid_repo)?;
    symlink("../cache/targets/v1/checkout-id", valid_repo.join("target"))?;
    let valid_repository = RepositoryDirectory::open(&valid_repo)?;
    assert!(TargetDirectory::open_with_target_root(valid_repository, &managed).is_err());
    Ok(())
}

#[test]
fn managed_target_parser_requires_one_absolute_root() -> Result<()> {
    assert_eq!(
        parse_managed_target_root("ok managed targets enabled at /tmp/mbx/targets; configured")?,
        PathBuf::from("/tmp/mbx/targets")
    );
    for output in [
        "managed targets disabled",
        "managed targets enabled at relative/targets;",
        "managed targets enabled at /one; managed targets enabled at /two;",
    ] {
        parse_managed_target_root(output).unwrap_err();
    }
    Ok(())
}
