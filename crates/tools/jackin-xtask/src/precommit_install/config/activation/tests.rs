// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use anyhow::{Context, Result, bail, ensure};
use nix::{
    sys::signal::{Signal, kill, raise},
    unistd::{Pid, fsync},
};
use std::{
    fs,
    io::Write,
    os::unix::{fs::MetadataExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const SIGNAL_FINALIZATION_CHILD: &str = "JACKIN_CONFIG_SIGNAL_FINALIZATION_CHILD";
const SIGNAL_FINALIZATION_ROOT: &str = "JACKIN_CONFIG_SIGNAL_FINALIZATION_ROOT";
const SIGNAL_FINALIZATION_MARKER: &str = "JACKIN_CONFIG_SIGNAL_FINALIZATION_MARKER";
const SIGNAL_FINALIZATION_READY: &str = "JACKIN_CONFIG_SIGNAL_FINALIZATION_READY";
const SIGNAL_FINALIZATION_RELEASE: &str = "JACKIN_CONFIG_SIGNAL_FINALIZATION_RELEASE";
const SIGNAL_FINALIZATION_TEST: &str =
    "config_signal_finalization_preserves_commit_and_default_action";

#[test]
fn config_activation_sync_failures_preserve_the_correct_commit_state() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let parent = open_config_parent(temporary.path())?;
    let config_name = std::ffi::OsStr::new("config");
    let lock_name = std::ffi::OsStr::new("config.lock");
    fs::write(temporary.path().join(config_name), b"old config\n")?;
    fs::write(temporary.path().join(lock_name), b"new config\n")?;
    let lock = open_config_file(&parent, lock_name)?;
    let old_config = open_config_file(&parent, config_name)?;
    let activation = ConfigActivationEntry {
        parent: &parent,
        lock_name,
        config_name,
        lock: &lock,
        destination: Some(&old_config),
    };

    let sync_error = activate_config_entry(
        &activation,
        || Ok(()),
        |_| anyhow::bail!("injected lock sync failure"),
        |parent, lock, config| {
            renameat(parent, lock, parent, config)?;
            Ok(())
        },
        |_| Ok(()),
    )
    .expect_err("a lock sync failure must stop activation");
    assert!(format!("{sync_error:#}").contains("injected lock sync failure"));
    assert!(!super::super::activation_failed_after_rename(&sync_error));
    assert_eq!(
        fs::read(temporary.path().join(config_name))?,
        b"old config\n"
    );
    assert_eq!(fs::read(temporary.path().join(lock_name))?, b"new config\n");

    let parent_sync_error = activate_config_entry(
        &activation,
        || Ok(()),
        |file| {
            file.sync_all()?;
            Ok(())
        },
        |parent, lock, config| {
            renameat(parent, lock, parent, config)?;
            Ok(())
        },
        |_| anyhow::bail!("injected parent sync failure"),
    )
    .expect_err("a parent sync failure must be reported after rename");
    assert!(super::super::activation_failed_after_rename(
        &parent_sync_error
    ));
    assert!(format!("{parent_sync_error:#}").contains("directory sync failed"));
    assert_eq!(
        fs::read(temporary.path().join(config_name))?,
        b"new config\n"
    );
    assert!(!temporary.path().join(lock_name).exists());
    Ok(())
}

#[test]
fn config_activation_rejects_a_destination_created_after_admission() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let parent = open_config_parent(temporary.path())?;
    let config_name = std::ffi::OsStr::new("config");
    let lock_name = std::ffi::OsStr::new("config.lock");
    fs::write(temporary.path().join(lock_name), b"candidate\n")?;
    let lock = open_config_file(&parent, lock_name)?;
    let activation = ConfigActivationEntry {
        parent: &parent,
        lock_name,
        config_name,
        lock: &lock,
        destination: None,
    };

    let error = activate_config_entry(
        &activation,
        || {
            fs::write(temporary.path().join(config_name), b"concurrent config\n")?;
            Ok(())
        },
        |file| {
            file.sync_all()?;
            Ok(())
        },
        |parent, lock, config| {
            renameat(parent, lock, parent, config)?;
            Ok(())
        },
        |parent| {
            fsync(parent)?;
            Ok(())
        },
    )
    .expect_err("activation must not replace a newly appeared config");

    assert!(format!("{error:#}").contains("destination appeared"));
    assert_eq!(
        fs::read(temporary.path().join(config_name))?,
        b"concurrent config\n"
    );
    assert_eq!(fs::read(temporary.path().join(lock_name))?, b"candidate\n");
    Ok(())
}

#[test]
fn config_signal_finalization_preserves_commit_and_default_action() -> Result<()> {
    if let Some(mode) = std::env::var_os(SIGNAL_FINALIZATION_CHILD) {
        return run_signal_finalization_child(mode.to_string_lossy().as_ref());
    }

    let temporary = tempfile::tempdir()?;
    let committed_root = temporary.path().join("committed config");
    fs::create_dir(&committed_root)?;
    fs::write(committed_root.join("config"), b"old config\n")?;
    let old_config = fs::symlink_metadata(committed_root.join("config"))?;
    let lock_inode_marker = temporary.path().join("activated lock inode");

    let preparation_root = temporary.path().join("candidate preparation");
    fs::create_dir(&preparation_root)?;
    fs::write(preparation_root.join("config"), b"old config\n")?;
    let mut preparation_child = SignalChildGuard::new(
        signal_child("prepare", &preparation_root, &lock_inode_marker, None, None)?
            .spawn()
            .context("starting candidate preparation signal child")?,
    );
    let status = preparation_child.wait(Duration::from_secs(15))?;
    ensure!(
        status.signal() == Some(Signal::SIGTERM as i32),
        "candidate preparation did not restore default SIGTERM after cleanup: {status}"
    );
    ensure!(
        fs::read(preparation_root.join("config"))? == b"old config\n",
        "SIGTERM during candidate preparation changed the config"
    );
    ensure_config_directory_is_clean(&preparation_root)?;

    let mut commit_child = SignalChildGuard::new(
        signal_child("commit", &committed_root, &lock_inode_marker, None, None)?
            .spawn()
            .context("starting config commit signal child")?,
    );
    let status = commit_child.wait(Duration::from_secs(15))?;
    ensure!(
        status.success(),
        "config commit signal child failed: {status}"
    );
    ensure!(
        fs::read(committed_root.join("config"))? == b"new config\n",
        "SIGTERM after rename did not preserve the committed config"
    );
    let committed = fs::symlink_metadata(committed_root.join("config"))?;
    let (lock_device, lock_inode) = read_inode_marker(&lock_inode_marker)?;
    ensure!(
        (committed.dev(), committed.ino()) == (lock_device, lock_inode)
            && (committed.dev(), committed.ino()) != (old_config.dev(), old_config.ino()),
        "committed config does not identify the activated lock"
    );
    ensure_config_directory_is_clean(&committed_root)?;

    let late_root = temporary.path().join("late config");
    fs::create_dir(&late_root)?;
    fs::write(late_root.join("config"), b"old config\n")?;
    let ready = temporary.path().join("late child ready");
    let release = temporary.path().join("late child release");
    let _release_on_drop = SignalReleaseOnDrop(release.clone());
    let late_child = signal_child(
        "late",
        &late_root,
        &lock_inode_marker,
        Some(&ready),
        Some(&release),
    )?
    .spawn()
    .context("starting late config signal child")?;
    let late_child_id = late_child.id();
    let mut late_child = SignalChildGuard::new(late_child);
    wait_for_signal_child_path(late_child.child_mut()?, &ready, Duration::from_secs(15))?;
    ensure!(
        fs::read(late_root.join("config"))? == b"new config\n",
        "late-signal child did not commit the config before readiness"
    );
    ensure_config_directory_is_clean(&late_root)?;
    kill(Pid::from_raw(late_child_id as i32), Signal::SIGTERM)
        .context("sending late SIGTERM to finalized config child")?;
    fs::write(&release, b"release")?;
    let status = late_child.wait(Duration::from_secs(15))?;
    ensure!(
        status.signal() == Some(Signal::SIGTERM as i32),
        "finalized config child did not retain default SIGTERM termination: {status}"
    );
    Ok(())
}

fn run_signal_finalization_child(mode: &str) -> Result<()> {
    let root = PathBuf::from(
        std::env::var_os(SIGNAL_FINALIZATION_ROOT).context("missing config fixture root")?,
    );
    let lock_inode_marker = PathBuf::from(
        std::env::var_os(SIGNAL_FINALIZATION_MARKER).context("missing config inode marker")?,
    );
    match mode {
        "prepare" => {
            run_config_activation(&root, &lock_inode_marker, SignalPoint::Preparation)?;
            bail!("candidate preparation returned after SIGTERM")
        }
        "commit" => {
            let error = run_config_activation(&root, &lock_inode_marker, SignalPoint::AfterRename)
                .expect_err("a captured post-rename SIGTERM must be reported after cleanup");
            ensure!(
                format!("{error:#}")
                    .contains("config committed, but installation received SIGTERM"),
                "post-rename SIGTERM lost committed-state context: {error:#}"
            );
            ensure_config_directory_is_clean(&root)?;
            Ok(())
        }
        "late" => {
            run_config_activation(&root, &lock_inode_marker, SignalPoint::None)?;
            let ready = PathBuf::from(
                std::env::var_os(SIGNAL_FINALIZATION_READY).context("missing child ready path")?,
            );
            let release = PathBuf::from(
                std::env::var_os(SIGNAL_FINALIZATION_RELEASE)
                    .context("missing child release path")?,
            );
            fs::write(ready, b"ready")?;
            wait_for_release(&release, Duration::from_secs(20))
        }
        _ => bail!("unknown config signal finalization child mode"),
    }
}

fn run_config_activation(
    root: &Path,
    lock_inode_marker: &Path,
    signal_point: SignalPoint,
) -> Result<()> {
    let config_name = std::ffi::OsStr::new("config");
    super::super::signals::with_install_signals(|signals| {
        let parent = open_config_parent(root)?;
        let destination = open_config_file(&parent, config_name)?;
        let mut candidate = tempfile::NamedTempFile::new_in(root)?;
        candidate.write_all(b"new config\n")?;
        if matches!(signal_point, SignalPoint::Preparation) {
            raise(Signal::SIGTERM)?;
            signals.check()?;
        }
        candidate.as_file().sync_all()?;
        let bytes = fs::read(candidate.path())?;
        let lock_path = root.join("config.lock");
        signals.enter_critical_phase()?;
        let lock_name = lock_path
            .file_name()
            .context("config lock has no filename")?;
        let lock_file = File::from(open(
            &lock_path,
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )?);
        let mut lock = super::super::ConfigLock::create(lock_path.clone(), lock_file);
        super::super::write_held_lock(lock.file(), &lock_path, &bytes)?;
        let metadata = lock.file().metadata()?;
        fs::write(
            lock_inode_marker,
            format!("{}:{}", metadata.dev(), metadata.ino()),
        )?;
        let activation = ConfigActivationEntry {
            parent: &parent,
            lock_name,
            config_name,
            lock: lock.file(),
            destination: Some(&destination),
        };
        signals.check()?;
        let result = activate_config_entry(
            &activation,
            || Ok(()),
            |file| {
                file.sync_all()?;
                Ok(())
            },
            |parent, lock, config| {
                renameat(parent, lock, parent, config)?;
                Ok(())
            },
            |parent| {
                if matches!(signal_point, SignalPoint::AfterRename) {
                    raise(Signal::SIGTERM)?;
                }
                fsync(parent)?;
                Ok(())
            },
        );
        if let Err(error) = result {
            if super::super::activation_failed_after_rename(&error) {
                lock.disarm();
                return Err(error)
                    .context("hook config was renamed, but final durability checks failed");
            }
            return match lock.remove_if_owned() {
                Ok(()) => Err(error),
                Err(cleanup) => {
                    Err(error.context(format!("Git config lock cleanup also failed: {cleanup:#}")))
                }
            };
        }
        lock.disarm();
        Ok(())
    })
}

#[derive(Clone, Copy)]
enum SignalPoint {
    Preparation,
    AfterRename,
    None,
}

fn signal_child(
    mode: &str,
    root: &Path,
    lock_inode_marker: &Path,
    ready: Option<&Path>,
    release: Option<&Path>,
) -> Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg(SIGNAL_FINALIZATION_TEST)
        .arg("--nocapture")
        .env(SIGNAL_FINALIZATION_CHILD, mode)
        .env(SIGNAL_FINALIZATION_ROOT, root)
        .env(SIGNAL_FINALIZATION_MARKER, lock_inode_marker)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(ready) = ready {
        command.env(SIGNAL_FINALIZATION_READY, ready);
    }
    if let Some(release) = release {
        command.env(SIGNAL_FINALIZATION_RELEASE, release);
    }
    Ok(command)
}

fn read_inode_marker(path: &Path) -> Result<(u64, u64)> {
    let marker = fs::read_to_string(path)?;
    let (device, inode) = marker
        .split_once(':')
        .context("invalid candidate inode marker")?;
    Ok((device.parse()?, inode.parse()?))
}

fn ensure_config_directory_is_clean(root: &Path) -> Result<()> {
    let names = crate::fs_util::read_dir_sorted(root)?
        .into_iter()
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    ensure!(
        names == [std::ffi::OsString::from("config")],
        "config activation left a lock or candidate file"
    );
    Ok(())
}

fn wait_for_signal_child_path(child: &mut Child, path: &Path, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        if let Some(status) = child.try_wait()? {
            bail!("config signal child exited before readiness: {status}");
        }
        ensure!(
            Instant::now() < deadline,
            "config signal child did not become ready"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
    Ok(())
}

fn wait_for_release(path: &Path, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        ensure!(
            Instant::now() < deadline,
            "config signal child release timed out"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
    Ok(())
}

struct SignalChildGuard(Option<Child>);

impl SignalChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn child_mut(&mut self) -> Result<&mut Child> {
        self.0.as_mut().context("signal child was already reaped")
    }

    fn wait(&mut self, timeout: Duration) -> Result<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            let child = self.0.as_mut().context("signal child was already reaped")?;
            if let Some(status) = child.try_wait()? {
                self.0 = None;
                return Ok(status);
            }
            ensure!(
                Instant::now() < deadline,
                "config signal child did not exit"
            );
            thread::park_timeout(Duration::from_millis(10));
        }
    }
}

impl Drop for SignalChildGuard {
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

struct SignalReleaseOnDrop(PathBuf);

impl Drop for SignalReleaseOnDrop {
    fn drop(&mut self) {
        drop(fs::write(&self.0, b"release"));
    }
}
