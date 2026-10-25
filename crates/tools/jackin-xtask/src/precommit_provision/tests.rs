// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{bundle::parse_request, validate_hook_config};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};

fn request() -> Vec<u8> {
    format!(
        "jackin-native-precommit-bootstrap/1\nreviewed-tree={}\nbuild-commit={}\nindex-sha256={}\nsource-sha256={}\ncargo-sha256={}\nrustc-sha256={}\nbuild-sha256={}\ntarget-sha256={}\n",
        "a".repeat(40),
        "b".repeat(40),
        "c".repeat(64),
        "d".repeat(64),
        "e".repeat(64),
        "f".repeat(64),
        "0".repeat(64),
        "1".repeat(64),
    )
    .into_bytes()
}

#[test]
fn request_parser_accepts_only_the_fixed_ordered_record() -> anyhow::Result<()> {
    let bytes = request();
    let parsed = parse_request(&bytes)?;
    assert_eq!(parsed.reviewed_tree, "a".repeat(40));
    assert_eq!(parsed.build_commit, "b".repeat(40));
    assert_eq!(parsed.target_sha256, "1".repeat(64));

    let mut duplicate = bytes.clone();
    duplicate.extend_from_slice(
        b"target-sha256=1111111111111111111111111111111111111111111111111111111111111111\n",
    );
    assert!(parse_request(&duplicate).is_err());

    let mut carriage_return = bytes;
    carriage_return.insert(carriage_return.len() - 1, b'\r');
    assert!(parse_request(&carriage_return).is_err());
    Ok(())
}

#[test]
fn request_parser_rejects_reordered_fields_and_wrong_digest_length() -> anyhow::Result<()> {
    let valid = String::from_utf8(request())?;
    let reordered = valid.replacen("build-commit=", "source-sha256=", 1);
    assert!(parse_request(reordered.as_bytes()).is_err());
    let malformed = valid.replace(
        &format!("cargo-sha256={}\n", "e".repeat(64)),
        "cargo-sha256=bad\n",
    );
    assert!(parse_request(malformed.as_bytes()).is_err());
    Ok(())
}

#[test]
fn hook_config_custody_checks_selected_config_and_existing_lock() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let repository = temporary.path().canonicalize()?.join("repository");
    fs::create_dir(&repository)?;
    let initialized = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .current_dir(&repository)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    assert!(initialized.success());
    let config = repository.join(".git/config");
    let lock = repository.join(".git/config.lock");
    fs::write(&lock, b"candidate\n")?;
    validate_hook_config(&repository)?;

    let external = temporary.path().join("external-config");
    fs::write(&external, b"[hook]\nunsafe = true\n")?;
    fs::remove_file(&config)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&external, &config)?;
    assert!(validate_hook_config(&repository).is_err());
    Ok(())
}

#[test]
fn hook_config_custody_selects_worktree_config_when_enabled() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let repository = temporary.path().canonicalize()?.join("repository");
    fs::create_dir(&repository)?;
    let initialized = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .current_dir(&repository)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    assert!(initialized.success());
    let setting = Command::new("git")
        .args(["config", "--local", "extensions.worktreeConfig", "true"])
        .current_dir(&repository)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()?;
    assert!(setting.success());
    let worktree_config = repository.join(".git/config.worktree");
    fs::write(&worktree_config, b"[hook]\nname = safe\n")?;
    fs::write(repository.join(".git/config.worktree.lock"), b"candidate\n")?;
    validate_hook_config(&repository)?;

    fs::remove_file(&worktree_config)?;
    let external = temporary.path().join("external-worktree-config");
    fs::write(&external, b"[hook]\nname = unsafe\n")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&external, &worktree_config)?;
    assert!(validate_hook_config(&repository).is_err());

    fs::remove_file(&worktree_config)?;
    fs::set_permissions(
        repository.join(".git/config"),
        fs::Permissions::from_mode(0o666),
    )?;
    assert!(validate_hook_config(&repository).is_err());
    Ok(())
}

#[test]
fn hook_config_custody_accepts_linked_worktree_config_parent() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let repository = temporary.path().canonicalize()?.join("repository");
    let worktree = temporary.path().canonicalize()?.join("linked-worktree");
    fs::create_dir(&repository)?;
    let initialized = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&repository)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()?;
    assert!(initialized.success());
    for (key, value) in [
        ("user.name", "Fixture"),
        ("user.email", "fixture@example.invalid"),
        ("extensions.worktreeConfig", "true"),
    ] {
        let configured = Command::new("git")
            .args(["config", "--local", key, value])
            .current_dir(&repository)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()?;
        assert!(configured.success());
    }
    fs::write(repository.join("tracked"), b"fixture\n")?;
    let added = Command::new("git")
        .args(["add", "tracked"])
        .current_dir(&repository)
        .status()?;
    assert!(added.success());
    let committed = Command::new("git")
        .args(["commit", "--quiet", "-m", "fixture"])
        .current_dir(&repository)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()?;
    assert!(committed.success());
    let linked = Command::new("git")
        .args(["worktree", "add", "--quiet", "--detach"])
        .arg(&worktree)
        .arg("HEAD")
        .current_dir(&repository)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()?;
    assert!(linked.success());

    let config_request = jackin_process::ExecRequest::new(
        "git",
        [
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config.worktree",
        ],
    )
    .env_clear()
    .envs([("PATH", std::env::var_os("PATH").unwrap_or_default())])
    .envs([
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ])
    .cwd(worktree.clone())
    .stdout_mode(jackin_process::StdioMode::Capture)
    .stderr_mode(jackin_process::StdioMode::Capture);
    let config = jackin_process::exec_sync(&config_request)?;
    assert!(config.success);
    let config_path = std::path::PathBuf::from(String::from_utf8(config.stdout)?.trim());
    let common_git_dir = repository.join(".git").canonicalize()?;
    assert_ne!(config_path.parent(), Some(common_git_dir.as_path()));
    fs::write(&config_path, b"[hook]\nname = safe\n")?;
    fs::write(
        config_path.with_file_name("config.worktree.lock"),
        b"candidate\n",
    )?;
    validate_hook_config(&worktree)?;
    Ok(())
}
