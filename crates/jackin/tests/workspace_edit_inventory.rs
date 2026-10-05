// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! A failed isolation inventory must stop the public workspace edit transaction.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use assert_cmd::Command;
use predicates::prelude::*;

fn command(home: &Path) -> anyhow::Result<Command> {
    let mut command = Command::cargo_bin("jackin")?;
    command
        .env_clear()
        .env("HOME", home)
        .env("JACKIN_HOME_DIR", home.join(".jackin"))
        .env("JACKIN_CONFIG_DIR", home.join(".config/jackin"))
        .env(
            "DOCKER_HOST",
            "unix:///nonexistent-jackin-inventory-test.sock",
        )
        .env("PATH", "/usr/bin:/bin")
        .current_dir(home)
        .timeout(Duration::from_secs(20));
    Ok(command)
}

fn fixture() -> anyhow::Result<tempfile::TempDir> {
    let temporary = tempfile::tempdir()?;
    let home = temporary.path();
    // Keep first-run discovery local, including on macOS hosts with Keychain.
    fs::create_dir(home.join(".claude"))?;
    fs::write(
        home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"synthetic-inventory-test"}}"#,
    )?;
    fs::create_dir(home.join("project"))?;
    command(home)?
        .args(["workspace", "create", "project", "--workdir"])
        .arg(home.join("project"))
        .assert()
        .success();
    command(home)?
        .args(["workspace", "show", "project"])
        .assert()
        .success();
    Ok(temporary)
}

// Independent filesystem oracle: retain directory topology as well as every
// file's bytes; never derive the expected state from the implementation reader.
fn tree(root: &Path) -> anyhow::Result<Vec<(PathBuf, Option<Vec<u8>>)>> {
    fn visit(
        root: &Path,
        directory: &Path,
        entries: &mut Vec<(PathBuf, Option<Vec<u8>>)>,
    ) -> anyhow::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(root)?.to_path_buf();
            if entry.file_type()?.is_dir() {
                entries.push((relative, None));
                visit(root, &path, entries)?;
            } else {
                entries.push((relative, Some(fs::read(path)?)));
            }
        }
        Ok(())
    }
    let mut entries = Vec::new();
    visit(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(entries)
}

fn rejects_inventory(body: Option<&[u8]>, diagnostic: &str) -> anyhow::Result<()> {
    let temporary = fixture()?;
    let home = temporary.path();
    let data = home.join(".jackin/data");
    let state = data.join("jk-broken/.jackin");
    fs::create_dir_all(&state)?;
    let manifest = state.join("isolation.json");
    if let Some(body) = body {
        fs::write(&manifest, body)?;
    } else {
        fs::create_dir(&manifest)?;
    }
    let config = home.join(".config/jackin");
    let config_before = tree(&config)?;
    let data_before = tree(&data)?;
    command(home)?
        .args(["workspace", "edit", "project", "--git-pull", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(diagnostic))
        .stdout(predicate::str::contains("Updated workspace").not());
    assert_eq!(
        tree(&config)?,
        config_before,
        "failed inventory changed config"
    );
    assert_eq!(
        tree(&data)?,
        data_before,
        "failed inventory changed isolation state"
    );
    Ok(())
}

#[test]
fn workspace_edit_rejects_corrupt_inventory_without_mutation() -> anyhow::Result<()> {
    rejects_inventory(Some(b"{invalid json"), "parse isolation file")
}

#[test]
fn workspace_edit_rejects_unsupported_inventory_without_mutation() -> anyhow::Result<()> {
    rejects_inventory(
        Some(br#"{"version":4294967295,"records":[]}"#),
        "unsupported isolation.json version",
    )
}

#[test]
fn workspace_edit_rejects_non_file_inventory_without_mutation() -> anyhow::Result<()> {
    rejects_inventory(None, "state metadata is not a regular file")
}

#[test]
fn workspace_edit_with_empty_inventory_applies_without_docker() -> anyhow::Result<()> {
    let temporary = fixture()?;
    let home = temporary.path();
    let workspace = home.join(".config/jackin/workspaces/project.toml");
    let before = fs::read(&workspace)?;
    command(home)?
        .args(["workspace", "edit", "project", "--git-pull", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated workspace"));
    assert_ne!(fs::read(&workspace)?, before, "valid edit did not apply");
    let parsed: jackin_config::WorkspaceConfig = toml::from_str(&fs::read_to_string(&workspace)?)?;
    assert!(parsed.git_pull_on_entry);
    Ok(())
}
