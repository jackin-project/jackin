// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
};

use super::*;

fn git(root: &Path, arguments: &[&str]) -> Result<()> {
    git_output_bytes(root, arguments)?;
    Ok(())
}

#[test]
fn worktree_check_ignores_hostile_repository_external_diff_config() -> Result<()> {
    let repository = tempfile::tempdir()?;
    git(repository.path(), &["init", "--quiet"])?;
    git(repository.path(), &["config", "user.name", "Fixture"])?;
    git(
        repository.path(),
        &["config", "user.email", "fixture@example.invalid"],
    )?;
    fs::write(repository.path().join("tracked"), "committed\n")?;
    git(repository.path(), &["add", "tracked"])?;
    git(repository.path(), &["commit", "--quiet", "-m", "fixture"])?;

    let marker = repository.path().join("external-diff-ran");
    let external = repository.path().join("external-diff");
    fs::write(
        &external,
        format!("#!/bin/sh\ntouch '{}'\nexit 0\n", marker.display()),
    )?;
    fs::set_permissions(&external, fs::Permissions::from_mode(0o700))?;
    let external = external.to_str().context("fixture path is not UTF-8")?;
    git(repository.path(), &["config", "diff.external", external])?;
    git(repository.path(), &["config", "diff.trustExitCode", "true"])?;
    fs::write(repository.path().join("tracked"), "modified\n")?;

    assert!(!worktree_diff_is_clean(repository.path()));
    assert!(!marker.exists(), "repository-configured diff helper ran");
    Ok(())
}

#[test]
fn git_path_records_preserve_non_utf8_and_embedded_newline_bytes() -> Result<()> {
    let bytes = vec![b'/', b't', b'm', b'p', b'/', 0xff, b'\n', b'x', b'\n'];
    let expected = PathBuf::from(OsString::from_vec(bytes[..bytes.len() - 1].to_vec()));
    let parsed = parse_git_path_record(bytes.clone(), false)?;
    assert_eq!(parsed, expected);

    parse_git_path_record(b"/tmp/path".to_vec(), false)
        .expect_err("unterminated Git path output must fail");
    parse_git_path_record(b"/tmp/\0path\n".to_vec(), false)
        .expect_err("NUL in Git path output must fail");
    assert!(
        parse_git_path_record(Vec::new(), true)?
            .as_os_str()
            .is_empty()
    );
    parse_git_path_record(Vec::new(), false)
        .expect_err("required Git path output must not be empty");
    Ok(())
}

#[test]
#[cfg(target_os = "linux")]
fn git_path_queries_support_non_utf8_repository_paths_with_newlines() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let component = OsString::from_vec(b"repo-\xff\npart".to_vec());
    let repository = temporary.path().join(component);
    fs::create_dir(&repository)?;
    let initialized = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&repository)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()?;
    assert!(initialized.success());

    let expected = fs::canonicalize(repository.join(".git/config"))?;
    let (common, selected) = hook_config_paths(&repository)?;
    assert_eq!(common, expected);
    assert_eq!(selected, expected);
    Ok(())
}

#[test]
fn git_path_queries_support_repository_paths_with_newlines() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let repository = temporary.path().join("repo-\npart");
    fs::create_dir(&repository)?;
    let initialized = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&repository)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()?;
    assert!(initialized.success());

    let expected = fs::canonicalize(repository.join(".git/config"))?;
    let (common, selected) = hook_config_paths(&repository)?;
    assert_eq!(common, expected);
    assert_eq!(selected, expected);
    Ok(())
}
