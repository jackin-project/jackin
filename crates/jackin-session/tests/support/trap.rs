// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

pub(crate) struct Trap {
    _dir: TempDir,
    marker: PathBuf,
    fsmonitor: PathBuf,
    hooks: PathBuf,
}

fn script(marker: &Path, label: &str) -> String {
    format!(
        "#!/bin/sh\necho \"{label}: $(ps -o args= -p $PPID)\" >> '{}'\nexit 0\n",
        marker.display()
    )
}

async fn git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = tokio::process::Command::new("/usr/bin/git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .current_dir(repo)
        .env_clear()
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into())
    }
}

impl Trap {
    pub(crate) async fn new() -> Result<Trap, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let root = std::fs::canonicalize(dir.path())?;
        let marker = root.join("marker");
        let fsmonitor = root.join("fsmonitor.sh");
        let hooks = root.join("hooks");
        tokio::fs::create_dir(&hooks).await?;
        tokio::fs::write(&fsmonitor, script(&marker, "fsmonitor")).await?;
        let mut executables = vec![fsmonitor.clone()];
        for name in ["reference-transaction", "post-index-change"] {
            let hook = hooks.join(name);
            tokio::fs::write(&hook, script(&marker, name)).await?;
            executables.push(hook);
        }
        for file in executables {
            tokio::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).await?;
        }
        Ok(Trap {
            _dir: dir,
            marker,
            fsmonitor,
            hooks,
        })
    }

    pub(crate) async fn arm(&self, repo: &Path) -> Result<(), Box<dyn Error>> {
        let fsmonitor = self.fsmonitor.display().to_string();
        let hooks = self.hooks.display().to_string();
        git(repo, &["config", "core.fsmonitor", &fsmonitor]).await?;
        git(repo, &["config", "core.hooksPath", &hooks]).await
    }

    pub(crate) async fn prove_armed(&self, repo: &Path) -> Result<(), Box<dyn Error>> {
        git(repo, &["ls-files", "-o"]).await?;
        git(repo, &["update-ref", "refs/trap/probe", "HEAD"]).await?;
        git(repo, &["update-ref", "-d", "refs/trap/probe"]).await?;
        let fired = self.fired();
        for needed in ["fsmonitor:", "reference-transaction:"] {
            if !fired.contains(needed) {
                return Err(
                    format!("the trap is not armed, {needed} did not fire: {fired:?}").into(),
                );
            }
        }
        std::fs::write(&self.marker, "")?;
        Ok(())
    }

    pub(crate) fn fired(&self) -> String {
        std::fs::read_to_string(&self.marker).unwrap_or_default()
    }
}
