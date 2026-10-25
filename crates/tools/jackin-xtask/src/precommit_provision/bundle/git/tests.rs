// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, os::unix::fs::PermissionsExt};

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
