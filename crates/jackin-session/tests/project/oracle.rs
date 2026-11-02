// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::error::Error;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

use super::support::TestResult;

pub(crate) fn run(dir: &Path, args: &[&str]) -> Result<std::process::Output, Box<dyn Error>> {
    Ok(Command::new("/usr/bin/git")
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?)
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = run(dir, args)?;
    if output.status.success() {
        Ok(String::from_utf8(output.stdout)?)
    } else {
        Err(format!(
            "git {args:?} in {} failed: {}",
            dir.display(),
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn lines(text: &str) -> BTreeSet<String> {
    text.lines().map(str::to_owned).collect()
}

fn refs(dir: &Path) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let listed = git(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname) %(symref)",
            "refs/heads",
            "refs/tags",
            "refs/remotes",
        ],
    )?;
    Ok(listed
        .lines()
        .filter(|line| !line.starts_with("refs/heads/jackin/"))
        .filter_map(|line| line.strip_suffix(' '))
        .map(str::to_owned)
        .collect())
}

fn head(dir: &Path) -> Result<(Option<String>, Option<String>), Box<dyn Error>> {
    let symbolic = run(dir, &["symbolic-ref", "-q", "HEAD"])?;
    let branch = symbolic
        .status
        .success()
        .then(|| String::from_utf8_lossy(&symbolic.stdout).trim().to_owned());
    let resolved = run(dir, &["rev-parse", "-q", "--verify", "HEAD"])?;
    let commit = resolved
        .status
        .success()
        .then(|| String::from_utf8_lossy(&resolved.stdout).trim().to_owned());
    Ok((branch, commit))
}

pub(crate) fn listed(dir: &Path) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    let output = run(dir, &["ls-files", "-z", "-co", "--exclude-standard"])?;
    let mut names: BTreeSet<Vec<u8>> = BTreeSet::new();
    for name in output.stdout.split(|byte| *byte == 0) {
        if !name.is_empty() {
            names.insert(name.to_vec());
        }
    }
    Ok(names.into_iter().collect())
}

fn same_files(host: &Path, sandbox: &Path) -> TestResult {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    for name in listed(host)? {
        let relative = Path::new(OsStr::from_bytes(&name));
        let from = host.join(relative);
        if !from.is_file() {
            continue;
        }
        let to = sandbox.join(relative);
        let shown = relative.display();
        assert_eq!(
            std::fs::read(&to).map_err(|error| format!("{shown}: {error}"))?,
            std::fs::read(&from)?,
            "{shown}: content"
        );
        assert_eq!(
            std::fs::metadata(&to)?.permissions().mode() & 0o111,
            std::fs::metadata(&from)?.permissions().mode() & 0o111,
            "{shown}: execute bits"
        );
    }
    let ignored = run(host, &["ls-files", "-z", "-o", "-i", "--exclude-standard"])?;
    for name in ignored.stdout.split(|byte| *byte == 0) {
        if !name.is_empty() {
            let relative = Path::new(OsStr::from_bytes(name));
            assert!(
                !sandbox.join(relative).exists(),
                "{}: an ignored file arrived",
                relative.display()
            );
        }
    }
    Ok(())
}

pub(crate) fn arrived(host: &Path, sandbox_root: &Path, dirty: &[&str]) -> TestResult {
    let project = sandbox_root.join("project");
    assert!(project.join(".git").is_dir(), ".git is not a directory");
    assert_eq!(head(&project)?, head(host)?, "HEAD");
    assert_eq!(
        git(&project, &["rev-parse", "--show-object-format"])?,
        git(host, &["rev-parse", "--show-object-format"])?,
        "object format"
    );
    assert_eq!(refs(&project)?, refs(host)?, "refs");
    git(&project, &["fsck", "--full"])?;
    same_files(host, &project)?;
    let status = git(
        &project,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    let expected: BTreeSet<String> = dirty.iter().map(|line| (*line).to_owned()).collect();
    assert_eq!(lines(&status), expected, "status");
    let bundles = sandbox_root.join(".jackin/bundle");
    let left: Vec<_> = std::fs::read_dir(&bundles)
        .map(|entries| entries.flatten().map(|entry| entry.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "bundle left in the sandbox: {left:?}");
    git(
        &project,
        &[
            "-c",
            "user.name=probe",
            "-c",
            "user.email=probe@example.invalid",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "probe",
        ],
    )?;
    Ok(())
}
