// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use jackin_core::{Config, FileSystem, HostPath, RoleName, StdFileSystem, WorkspaceName};
use jackin_image::RoleRevision;
use jackin_session::{RoleCatalog, RoleError};
use jackin_testkit::{FakeClock, RoleFixture, TestRunner, WorkingFs};

pub(crate) const URL: &str = "https://example.invalid/role.git";

pub(crate) async fn git(dir: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = tokio::process::Command::new("/usr/bin/git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .await?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

pub(crate) struct Origin {
    pub(crate) role: RoleFixture,
    pub(crate) commits: Vec<String>,
    pub(crate) home: HostPath,
    pub(crate) url: String,
    _home: tempfile::TempDir,
}

impl Origin {
    pub(crate) async fn new() -> Result<Origin, Box<dyn Error>> {
        Origin::at(URL).await
    }

    pub(crate) async fn at(url: &str) -> Result<Origin, Box<dyn Error>> {
        let role = RoleFixture::new("sentinel")?.model("model-1");
        role.commit().await?;
        let root = role.root().as_path();
        git(root, &["tag", "v1"]).await?;
        let first = git(root, &["rev-parse", "HEAD"]).await?;
        git(root, &["checkout", "-q", "-b", "dev"]).await?;
        role.clone().model("model-dev").commit().await?;
        let dev = git(root, &["rev-parse", "HEAD"]).await?;
        git(root, &["checkout", "-q", "main"]).await?;
        let home_dir = tempfile::tempdir()?;
        let home = HostPath::parse(&std::fs::canonicalize(home_dir.path())?)?;
        tokio::fs::write(
            home.as_path().join(".gitconfig"),
            format!("[url \"file://{}\"]\n\tinsteadOf = {url}\n", root.display()),
        )
        .await?;
        Ok(Origin {
            role,
            commits: vec![first, dev],
            home,
            url: url.to_owned(),
            _home: home_dir,
        })
    }

    pub(crate) async fn advance(&self, model: &str) -> Result<String, Box<dyn Error>> {
        self.role.clone().model(model).commit().await?;
        git(self.role.root().as_path(), &["rev-parse", "HEAD"]).await
    }

    pub(crate) fn config(&self, rev: Option<&str>) -> Result<Config, Box<dyn Error>> {
        let rev = rev.map_or_else(String::new, |rev| format!(", rev = \"{rev}\""));
        Ok(Config::parse(&format!(
            "[roles.sentinel]\nsource = {{ git = \"{}\"{rev} }}\n\n[workspaces.w]\npath = \"{}\"\n",
            self.url,
            self.home.as_path().display()
        ))?)
    }

    pub(crate) async fn load(
        &self,
        cache: &HostPath,
        rev: Option<&str>,
    ) -> Result<Result<(String, Option<String>), RoleError>, Box<dyn Error>> {
        let runner = Arc::new(TestRunner::with_home(&self.home));
        let clock = Arc::new(FakeClock::new());
        let fs: Arc<dyn FileSystem> =
            Arc::new(WorkingFs::new(Arc::new(StdFileSystem::default()), &clock));
        let catalog = RoleCatalog::new(fs, runner, clock, cache.clone());
        let config = self.config(rev)?;
        let workspace = config
            .workspace(&WorkspaceName::parse("w")?)
            .ok_or("no workspace")?;
        let name = RoleName::parse("sentinel")?;
        Ok(catalog
            .load(
                &config,
                Some(&name),
                workspace,
                &tokio_util::sync::CancellationToken::new(),
            )
            .await
            .map(|loaded| {
                let RoleRevision { commit, .. } = loaded.revision();
                let commit = commit.as_str().to_owned();
                (
                    commit,
                    loaded
                        .manifest()
                        .codex
                        .model
                        .as_ref()
                        .map(|m| m.as_str().to_owned()),
                )
            }))
    }
}

pub(crate) fn cache() -> Result<(HostPath, tempfile::TempDir), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    Ok((HostPath::parse(&std::fs::canonicalize(dir.path())?)?, dir))
}
