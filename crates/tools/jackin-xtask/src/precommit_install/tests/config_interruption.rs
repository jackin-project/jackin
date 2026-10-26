// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    os::unix::ffi::OsStrExt,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};

use super::super::{git, install_hooks};
use super::fixture::{git as run_git, init_repository, private_dir};

const CHILD_MODE: &str = "JACKIN_CONFIG_INTERRUPTION_CHILD_MODE";

#[test]
fn config_install_signal_during_candidate_preparation_leaves_config_untouched() -> Result<()> {
    if let Some(mode) = std::env::var_os(CHILD_MODE) {
        return run_child_mode(mode.to_string_lossy().as_ref());
    }

    let temporary = private_dir()?;
    let repository = temporary.path().join("signal repository");
    init_repository(&repository)?;
    run_git(
        &repository,
        &["config", "--worktree", "fixture.sentinel", "original"],
    )?;
    let (_, _, selected_config) = git::installation_paths(&repository)?;
    let original_config = fs::read(&selected_config)?;
    let shim = prepare_git_shim(temporary.path())?;
    let _release_on_drop = ReleaseOnDrop(shim.release.clone());
    let mut child = ChildGuard::new(
        test_child("signal", &repository, Some(&shim.directory))?
            .spawn()
            .context("starting public installer signal fixture")?,
    );

    wait_for_path(&shim.ready, Duration::from_secs(20))?;
    kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM)
        .context("sending SIGTERM to public installer fixture")?;
    fs::write(&shim.release, b"release")?;
    let status = child.wait(Duration::from_secs(20))?;
    ensure!(
        status.signal() == Some(Signal::SIGTERM as i32),
        "installer child did not retain default SIGTERM termination: {status}"
    );
    ensure!(
        !selected_config
            .with_file_name("config.worktree.lock")
            .exists(),
        "candidate preparation created a live config lock before cancellation"
    );
    ensure!(
        fs::read(&selected_config)? == original_config,
        "candidate preparation changed the live worktree config before cancellation"
    );

    let mut retry_command = test_child("install", &repository, None)?;
    let retry = ChildGuard::new(
        retry_command
            .spawn()
            .context("starting fresh public installer retry")?,
    )
    .wait(Duration::from_secs(20))?;
    ensure!(
        retry.success(),
        "fresh public installer retry failed: {retry}"
    );
    let (git_dir, _, _) = git::installation_paths(&repository)?;
    git::verify_installed_hooks(&repository, &git_dir.join("jackin-precommit-owner/owner"))?;
    Ok(())
}

#[test]
fn config_install_rejects_competing_writer_during_candidate_preparation() -> Result<()> {
    if let Some(mode) = std::env::var_os(CHILD_MODE) {
        return run_child_mode(mode.to_string_lossy().as_ref());
    }

    let temporary = private_dir()?;
    run_competing_writer_case(temporary.path(), true)?;
    run_competing_writer_case(temporary.path(), false)?;
    Ok(())
}

fn run_competing_writer_case(root: &Path, initially_present: bool) -> Result<()> {
    let name = if initially_present {
        "present"
    } else {
        "absent"
    };
    let repository = root.join(format!("{name} writer repository"));
    init_repository(&repository)?;
    if initially_present {
        run_git(
            &repository,
            &["config", "--worktree", "fixture.sentinel", "original"],
        )?;
    }
    let (_, _, selected_config) = git::installation_paths(&repository)?;
    let shim = prepare_git_shim(&root.join(format!("{name} writer")))?;
    let _release_on_drop = ReleaseOnDrop(shim.release.clone());
    let mut child = ChildGuard::new(
        test_child("conflict", &repository, Some(&shim.directory))?
            .spawn()
            .context("starting public installer competing-writer fixture")?,
    );

    wait_for_path(&shim.ready, Duration::from_secs(20))?;
    run_git(
        &repository,
        &["config", "--worktree", "fixture.sentinel", "competitor"],
    )?;
    fs::write(&shim.release, b"release")?;
    let status = child.wait(Duration::from_secs(20))?;
    ensure!(
        status.success(),
        "competing-writer installer fixture failed: {status}"
    );
    ensure!(
        run_git(
            &repository,
            &["config", "--worktree", "--get", "fixture.sentinel"]
        )?
        .stdout
            == b"competitor\n",
        "installer did not preserve the competing worktree config update"
    );
    ensure!(
        !selected_config
            .with_file_name("config.worktree.lock")
            .exists(),
        "competing-writer rejection left a live config lock"
    );
    Ok(())
}

fn run_child_mode(mode: &str) -> Result<()> {
    match mode {
        "signal" => {
            install_hooks()?;
            bail!("installer unexpectedly returned before the signal")
        }
        "conflict" => {
            install_hooks().expect_err("installer must reject a competing config writer");
            Ok(())
        }
        "install" => install_hooks(),
        _ => bail!("unknown installer subprocess mode"),
    }
}

fn test_child(mode: &str, repository: &Path, shim: Option<&Path>) -> Result<Command> {
    let name = current_test_name(if mode == "conflict" {
        "config_install_rejects_competing_writer_during_candidate_preparation"
    } else {
        "config_install_signal_during_candidate_preparation_leaves_config_untouched"
    })?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg(name)
        .arg("--nocapture")
        .current_dir(repository)
        .env(CHILD_MODE, mode);
    if let Some(shim) = shim {
        let mut search_path = vec![shim.to_path_buf()];
        search_path.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command.env("PATH", std::env::join_paths(search_path)?);
    }
    Ok(command)
}

struct GitShim {
    directory: PathBuf,
    ready: PathBuf,
    release: PathBuf,
}

fn prepare_git_shim(root: &Path) -> Result<GitShim> {
    fs::create_dir_all(root)?;
    let directory = root.join("git shim");
    fs::create_dir(&directory)?;
    let ready = root.join("shim.ready");
    let release = root.join("shim.release");
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join("git"))
        .find(|path| path.is_file())
        .context("locating real Git for the candidate-preparation shim")?;
    let mut script =
        b"#!/bin/sh\nif [ \"$1\" = config ] && [ \"$2\" = --file ] && [ ! -e ".to_vec();
    append_shell_path(&mut script, &ready)?;
    script.extend_from_slice(b" ]; then\n  : > ");
    append_shell_path(&mut script, &ready)?;
    script.extend_from_slice(b"\n  parent=$PPID\n  count=0\n  while [ ! -e ");
    append_shell_path(&mut script, &release)?;
    script.extend_from_slice(
        b" ]; do\n    kill -0 \"$parent\" 2>/dev/null || exit 143\n    count=$((count + 1))\n    [ \"$count\" -lt 6000 ] || exit 88\n    sleep 0.01\n  done\nfi\nexec ",
    );
    append_shell_path(&mut script, &real_git)?;
    script.extend_from_slice(b" \"$@\"\n");
    let shim = directory.join("git");
    fs::write(&shim, script)?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o700))?;
    Ok(GitShim {
        directory,
        ready,
        release,
    })
}

fn append_shell_path(script: &mut Vec<u8>, path: &Path) -> Result<()> {
    script.extend_from_slice(git::shell_quote_path(path)?.as_os_str().as_bytes());
    Ok(())
}

struct ReleaseOnDrop(PathBuf);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        drop(fs::write(&self.0, b"release"));
    }
}

fn current_test_name(suffix: &str) -> Result<String> {
    let output = Command::new(std::env::current_exe()?)
        .arg("--list")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting native test executable --list")?
        .wait_with_output()
        .context("listing native installer test cases")?;
    ensure!(
        output.status.success(),
        "native test executable --list failed"
    );
    let listing = String::from_utf8(output.stdout).context("test list is not UTF-8")?;
    let matches = listing
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .filter(|name| name.ends_with(suffix))
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "native installer test selector is ambiguous"
    );
    Ok(matches[0].to_owned())
}

fn wait_for_path(path: &Path, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        ensure!(
            Instant::now() < deadline,
            "candidate preparation did not reach its Git shim"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
    Ok(())
}

struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn id(&self) -> u32 {
        self.0.as_ref().expect("child guard is active").id()
    }

    fn wait(&mut self, timeout: Duration) -> Result<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            let child = self.0.as_mut().context("child guard is inactive")?;
            if let Some(status) = child.try_wait()? {
                self.0 = None;
                return Ok(status);
            }
            ensure!(
                Instant::now() < deadline,
                "installer subprocess did not exit"
            );
            thread::park_timeout(Duration::from_millis(10));
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut()
            && child.try_wait().ok().flatten().is_none()
        {
            drop(child.kill());
            drop(child.wait());
        }
    }
}
