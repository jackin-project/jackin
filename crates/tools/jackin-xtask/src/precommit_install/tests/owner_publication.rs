use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    thread,
};

use anyhow::{Context, Result};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};

use super::super::files;
use super::fixture::private_dir;

#[test]
fn identical_owner_install_keeps_the_existing_inode() -> Result<()> {
    let temporary = private_dir()?;
    let owner = b"stable native owner";
    let path = files::publish_owner(temporary.path(), owner)?;
    let before = fs::symlink_metadata(&path)?;
    assert_eq!(before.permissions().mode() & 0o777, 0o500);
    assert_eq!(fs::read(&path)?, owner);

    assert_eq!(files::publish_owner(temporary.path(), owner)?, path);
    let after = fs::symlink_metadata(&path)?;
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    Ok(())
}

#[test]
fn replacing_owner_preserves_held_old_bytes_and_publishes_new_bytes() -> Result<()> {
    let temporary = private_dir()?;
    let path = files::publish_owner(temporary.path(), b"old owner bytes")?;
    let mut held = File::from(open(
        &path,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?);

    files::publish_owner(temporary.path(), b"replacement owner bytes")?;

    let mut held_bytes = Vec::new();
    held.read_to_end(&mut held_bytes)?;
    assert_eq!(held_bytes, b"old owner bytes");
    assert_eq!(fs::read(&path)?, b"replacement owner bytes");
    Ok(())
}

#[test]
fn concurrent_publication_leaves_one_complete_owner_without_staging_files() -> Result<()> {
    let temporary = private_dir()?;
    let candidates = (0..8)
        .map(|index| format!("complete-owner-{index:02}-{}", "x".repeat(4096)).into_bytes())
        .collect::<Vec<_>>();
    thread::scope(|scope| {
        let installers = candidates
            .iter()
            .map(|candidate| {
                let git_dir = temporary.path();
                scope.spawn(move || files::publish_owner(git_dir, candidate))
            })
            .collect::<Vec<_>>();
        for (index, installer) in installers.into_iter().enumerate() {
            installer
                .join()
                .expect("installer thread must not panic")
                .with_context(|| format!("concurrent owner installer {index} failed"))?;
        }
        Ok::<(), anyhow::Error>(())
    })?;

    let owner_dir = temporary.path().join("jackin-precommit-owner");
    let published = fs::read(owner_dir.join("owner"))?;
    assert!(candidates.contains(&published));
    let names = crate::tests::read_dir_sorted(&owner_dir)?
        .into_iter()
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            std::ffi::OsString::from("install.lock"),
            std::ffi::OsString::from("owner")
        ]
    );
    Ok(())
}

#[test]
#[cfg(not(target_os = "macos"))]
fn public_installer_supports_non_utf8_checkout_path() -> Result<()> {
    use super::fixture::{git, init_repository};
    use std::{
        ffi::{OsStr, OsString},
        os::unix::ffi::OsStrExt,
        process::{Command, Stdio},
    };
    const CHILD_MODE: &str = "JACKIN_NON_UTF8_INSTALL_CHILD";
    if std::env::var_os(CHILD_MODE).is_some() {
        return super::super::install_hooks();
    }

    let temporary = private_dir()?;
    let mut checkout_name = OsString::from("checkout-");
    checkout_name.push(OsStr::from_bytes(b"\xff"));
    let repository = temporary.path().join(checkout_name);
    init_repository(&repository)?;
    let test_name = current_test_name()?;
    let output = Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .current_dir(&repository)
        .env_clear()
        .env("HOME", std::env::var_os("HOME").unwrap_or_default())
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env(CHILD_MODE, "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    assert!(
        output.status.success(),
        "public installer failed in non-UTF-8 checkout: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let (git_dir, _, _) = super::super::git::installation_paths(&repository)?;
    let owner = git_dir.join("jackin-precommit-owner/owner");
    assert!(owner.as_os_str().as_bytes().contains(&0xff));
    let mut expected = super::super::git::shell_quote_path(&owner)?;
    expected.push(" run-snapshot");
    let mut expected_record = expected.as_os_str().as_bytes().to_vec();
    expected_record.push(b'\n');
    assert_eq!(
        git(
            &repository,
            &[
                "config",
                "--worktree",
                "--get",
                "hook.jackin-pre-commit.command"
            ]
        )?
        .stdout,
        expected_record
    );
    super::super::git::verify_installed_hooks(&repository, &owner)?;
    files::publish_owner(&git_dir, b"#!/bin/sh\n[ \"$#\" -eq 1 ] || exit 95\n[ \"$1\" = run-snapshot ] || exit 96\nprintf '%s\\n' \"$1\" > hook-argv\nexit 37\n")?;
    let request = jackin_process::ExecRequest::new("git", ["hook", "run", "pre-commit"])
        .cwd(&repository)
        .env_clear()
        .envs([
            ("PATH", std::env::var_os("PATH").unwrap_or_default()),
            ("GIT_CONFIG_NOSYSTEM", OsString::from("1")),
            ("GIT_CONFIG_GLOBAL", OsString::from("/dev/null")),
            ("GIT_TERMINAL_PROMPT", OsString::from("0")),
        ])
        .timeout(std::time::Duration::from_secs(10))
        .output_limits(64 * 1024, 64 * 1024);
    let invoked = jackin_process::exec_sync(&request)?;
    assert_eq!(invoked.code, Some(37));
    assert_eq!(fs::read(repository.join("hook-argv"))?, b"run-snapshot\n");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn current_test_name() -> Result<String> {
    use std::process::{Command, Stdio};
    let output = Command::new(std::env::current_exe()?)
        .arg("--list")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    assert!(
        output.status.success(),
        "native installer test listing failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listing = String::from_utf8(output.stdout)?;
    let matches = listing
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .filter(|name| name.ends_with("public_installer_supports_non_utf8_checkout_path"))
        .collect::<Vec<_>>();
    anyhow::ensure!(matches.len() == 1, "test selector is ambiguous");
    Ok(matches[0].to_owned())
}
