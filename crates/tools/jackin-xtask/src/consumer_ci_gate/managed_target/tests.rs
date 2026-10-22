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
    run.write_file("junit.xml", b"<testsuites/>")?;
    assert_eq!(
        run.read_relative_file(Path::new("junit.xml"), 1024)?,
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
