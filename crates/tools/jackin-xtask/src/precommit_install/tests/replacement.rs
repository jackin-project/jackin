use std::{
    fs,
    os::unix::fs::MetadataExt,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};

use super::super::files;
use super::fixture::private_dir;

const READY_ENV: &str = "JACKIN_OWNER_TEST_READY";
const RELEASE_ENV: &str = "JACKIN_OWNER_TEST_RELEASE";

#[test]
fn stable_owner_replacement_keeps_running_native_process() -> Result<()> {
    if let Some(ready_path) = std::env::var_os(READY_ENV) {
        return child_readiness_mode(&ready_path);
    }
    let Some(release_path) = std::env::var_os(RELEASE_ENV) else {
        return run_replacement_parent();
    };
    ensure!(
        fs::metadata(release_path).is_ok(),
        "replacement invocation did not receive its release marker"
    );
    Ok(())
}

fn child_readiness_mode(ready_path: &std::ffi::OsStr) -> Result<()> {
    let release = std::env::var_os(RELEASE_ENV).context("missing owner child release path")?;
    fs::write(ready_path, b"ready")?;
    wait_for_path(&std::path::PathBuf::from(release), Duration::from_secs(20))
}

fn run_replacement_parent() -> Result<()> {
    let temporary = private_dir()?;
    let executable = std::env::current_exe()?;
    let initial = fs::read(&executable)?;
    let owner = files::publish_owner(temporary.path(), &initial)?;
    let before = fs::symlink_metadata(&owner)?;
    let ready = temporary.path().join("child.ready");
    let release = temporary.path().join("child.release");
    let test_name = current_test_name("stable_owner_replacement_keeps_running_native_process")?;
    let mut child = Command::new(&owner)
        .arg("--exact")
        .arg(&test_name)
        .arg("--nocapture")
        .env(READY_ENV, &ready)
        .env(RELEASE_ENV, &release)
        .spawn()
        .context("starting installed native test executable")?;

    if let Err(error) = wait_child_readiness(&mut child, &ready) {
        terminate(&mut child)?;
        return Err(error);
    }

    let replacement = fs::read("/usr/bin/true").context("reading native replacement executable")?;
    files::publish_owner(temporary.path(), &replacement)?;
    let after = fs::symlink_metadata(&owner)?;
    ensure!(
        before.ino() != after.ino(),
        "owner pathname was not atomically replaced"
    );
    ensure!(
        fs::read(&owner)? == replacement,
        "replacement owner bytes are incomplete"
    );

    fs::write(&release, b"release")?;
    wait_child_exit(&mut child)?;
    ensure!(
        child.wait()?.success(),
        "old native owner process failed after replacement"
    );
    ensure!(
        Command::new(&owner).status()?.success(),
        "new native owner executable failed after replacement"
    );
    Ok(())
}

fn current_test_name(suffix: &str) -> Result<String> {
    let output = Command::new(std::env::current_exe()?)
        .arg("--list")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting native test executable --list")?
        .wait_with_output()
        .context("listing native owner test executable cases")?;
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
        "native replacement test selector is ambiguous"
    );
    Ok(matches[0].to_owned())
}

fn wait_child_readiness(child: &mut Child, ready: &std::path::Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if ready.exists() {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            bail!("native owner child exited before readiness: {status}");
        }
        ensure!(
            Instant::now() < deadline,
            "native owner child readiness timed out"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

fn wait_child_exit(child: &mut Child) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "native owner child exit timed out"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

fn wait_for_path(path: &std::path::Path, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        ensure!(
            Instant::now() < deadline,
            "native owner child release timed out"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
    Ok(())
}

fn terminate(child: &mut Child) -> Result<()> {
    if child.try_wait()?.is_none() {
        child
            .kill()
            .context("stopping failed native owner test child")?;
        child
            .wait()
            .context("reaping failed native owner test child")?;
    }
    Ok(())
}
