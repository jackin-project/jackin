use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};

use super::super::files;
use super::fixture::private_dir;

const IMAGE_REPLACEMENT_CHILD: &str = "JACKIN_OWNER_IMAGE_REPLACEMENT_CHILD";

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
fn public_installer_rejects_replaced_running_image_before_publication() -> Result<()> {
    if std::env::var_os(IMAGE_REPLACEMENT_CHILD).is_some() {
        return super::super::install_hooks();
    }

    let temporary = private_dir()?;
    let repository = temporary.path().join("image replacement repository");
    super::fixture::init_repository(&repository)?;
    super::fixture::git(
        &repository,
        &["config", "--worktree", "fixture.sentinel", "original"],
    )?;
    let (_, _, selected_config) = super::super::git::installation_paths(&repository)?;
    let original_config = fs::read(&selected_config)?;
    let executable = temporary.path().join("running owner");
    fs::copy(std::env::current_exe()?, &executable)?;
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))?;
    let shim = prepare_waiting_git_shim(temporary.path())?;
    let mut search_path = vec![shim.directory.clone()];
    search_path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let test_name =
        current_test_name("public_installer_rejects_replaced_running_image_before_publication")?;
    let mut child = ChildGuard::new(
        Command::new(&executable)
            .arg("--exact")
            .arg(test_name)
            .arg("--nocapture")
            .current_dir(&repository)
            .env(IMAGE_REPLACEMENT_CHILD, "1")
            .env("PATH", std::env::join_paths(search_path)?)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("starting copied public installer")?,
    );

    let _release_on_drop = ReleaseOnDrop(shim.release.clone());
    wait_for_child_path(child.child_mut()?, &shim.ready, Duration::from_secs(20))?;
    let replacement = temporary.path().join("replacement image");
    fs::write(&replacement, b"replacement executable image")?;
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o700))?;
    fs::rename(&replacement, &executable)
        .context("replacing copied owner pathname while process is running")?;
    fs::write(&shim.release, b"release")?;
    let output = wait_child_output(child.into_child()?, Duration::from_secs(20))?;

    ensure!(
        !output.status.success(),
        "public installer accepted a replacement at its running image pathname"
    );
    ensure!(
        fs::read(&selected_config)? == original_config,
        "rejected running image changed worktree config"
    );
    let (git_dir, _, _) = super::super::git::installation_paths(&repository)?;
    ensure!(
        !git_dir.join("jackin-precommit-owner/owner").exists(),
        "rejected running image published a hook owner"
    );
    ensure!(
        shim.ready.exists(),
        "Git query did not reach the blocking shim"
    );
    Ok(())
}

struct WaitingGitShim {
    directory: PathBuf,
    ready: PathBuf,
    release: PathBuf,
}

fn prepare_waiting_git_shim(root: &Path) -> Result<WaitingGitShim> {
    let directory = root.join("waiting git shim");
    fs::create_dir(&directory)?;
    let ready = root.join("git query ready");
    let release = root.join("git query release");
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|path| path.join("git"))
        .find(|path| path.is_file())
        .context("locating real Git for the image replacement fixture")?;
    let mut script = b"#!/bin/sh\n: > ".to_vec();
    append_shell_path(&mut script, &ready)?;
    script.extend_from_slice(b"\nwhile [ ! -e ");
    append_shell_path(&mut script, &release)?;
    script.extend_from_slice(b" ]; do sleep 0.01; done\nexec ");
    append_shell_path(&mut script, &real_git)?;
    script.extend_from_slice(b" \"$@\"\n");
    let shim = directory.join("git");
    fs::write(&shim, script)?;
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o700))?;
    Ok(WaitingGitShim {
        directory,
        ready,
        release,
    })
}

fn append_shell_path(script: &mut Vec<u8>, path: &Path) -> Result<()> {
    script.extend_from_slice(
        super::super::git::shell_quote_path(path)?
            .as_os_str()
            .as_encoded_bytes(),
    );
    Ok(())
}

struct ReleaseOnDrop(PathBuf);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        drop(fs::write(&self.0, b"release"));
    }
}

struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn child_mut(&mut self) -> Result<&mut Child> {
        self.0
            .as_mut()
            .context("public installer child was already consumed")
    }

    fn into_child(mut self) -> Result<Child> {
        self.0
            .take()
            .context("public installer child was already consumed")
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let Some(child) = self.0.as_mut() else {
            return;
        };
        if !matches!(child.try_wait(), Ok(Some(_))) {
            match child.kill() {
                Ok(()) | Err(_) => {}
            }
            match child.wait() {
                Ok(_) | Err(_) => {}
            }
        }
    }
}

fn wait_for_child_path(child: &mut Child, path: &Path, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let (_poll_sender, poll_receiver) = std::sync::mpsc::channel::<()>();
    loop {
        if path.exists() {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            bail!("public installer exited before Git query was blocked: {status}");
        }
        ensure!(
            Instant::now() < deadline,
            "timed out waiting for blocked Git query"
        );
        assert_eq!(
            poll_receiver.recv_timeout(Duration::from_millis(10)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        );
    }
}

fn wait_child_output(mut child: Child, timeout: Duration) -> Result<std::process::Output> {
    let deadline = Instant::now() + timeout;
    let (_poll_sender, poll_receiver) = std::sync::mpsc::channel::<()>();
    loop {
        if let Some(status) = child.try_wait()? {
            let output = child.wait_with_output()?;
            ensure!(
                output.status == status,
                "child status changed while collecting output"
            );
            return Ok(output);
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            bail!("public installer did not exit after Git query release");
        }
        assert_eq!(
            poll_receiver.recv_timeout(Duration::from_millis(10)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        );
    }
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
    let test_name = current_test_name("public_installer_supports_non_utf8_checkout_path")?;
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

fn current_test_name(suffix: &str) -> Result<String> {
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
        .filter(|name| name.ends_with(suffix))
        .collect::<Vec<_>>();
    anyhow::ensure!(matches.len() == 1, "test selector is ambiguous");
    Ok(matches[0].to_owned())
}
