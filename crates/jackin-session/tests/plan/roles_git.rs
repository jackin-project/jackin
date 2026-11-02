// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use jackin_core::{Config, FileSystem, HostPath, RoleName, StdFileSystem, WorkspaceName};
use jackin_image::RoleRevision;
use jackin_session::{RoleCatalog, RoleError};
use jackin_testkit::{RoleFixture, TestRunner};

use super::support::{TestResult, catalog, guard};
use crate::trap::Trap;

const URL: &str = "https://example.invalid/role.git";

async fn git(dir: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
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

struct Origin {
    role: RoleFixture,
    commits: Vec<String>,
    home: HostPath,
    _home: tempfile::TempDir,
}

impl Origin {
    async fn new() -> Result<Origin, Box<dyn Error>> {
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
            format!("[url \"file://{}\"]\n\tinsteadOf = {URL}\n", root.display()),
        )
        .await?;
        Ok(Origin {
            role,
            commits: vec![first, dev],
            home,
            _home: home_dir,
        })
    }

    async fn advance(&self, model: &str) -> Result<String, Box<dyn Error>> {
        self.role.clone().model(model).commit().await?;
        git(self.role.root().as_path(), &["rev-parse", "HEAD"]).await
    }

    async fn load(
        &self,
        cache: &HostPath,
        rev: Option<&str>,
    ) -> Result<Result<(String, Option<String>), RoleError>, Box<dyn Error>> {
        let runner = Arc::new(TestRunner::with_home(&self.home));
        let fs: Arc<dyn FileSystem> = Arc::new(StdFileSystem::default());
        let catalog = RoleCatalog::new(fs, runner, cache.clone());
        let rev = rev.map_or_else(String::new, |rev| format!(", rev = \"{rev}\""));
        let config = Config::parse(&format!(
            "[roles.sentinel]\nsource = {{ git = \"{URL}\"{rev} }}\n\n[workspaces.w]\npath = \"{}\"\n",
            self.home.as_path().display()
        ))?;
        let workspace = config
            .workspace(&WorkspaceName::parse("w")?)
            .ok_or("no workspace")?;
        let name = RoleName::parse("sentinel")?;
        Ok(catalog
            .load(&config, Some(&name), workspace)
            .await
            .map(|loaded| {
                let commit = match loaded.revision() {
                    RoleRevision::Clean { commit } | RoleRevision::Dirty { commit, .. } => {
                        commit.as_str().to_owned()
                    }
                };
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

fn cache() -> Result<(HostPath, tempfile::TempDir), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    Ok((HostPath::parse(&std::fs::canonicalize(dir.path())?)?, dir))
}

#[tokio::test]
async fn a_git_role_without_a_revision_follows_the_default_branch_as_it_moves() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let (commit, model) = origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            Some(commit),
            git(origin.role.root().as_path(), &["rev-parse", "main"])
                .await
                .ok()
        );
        assert_eq!(model.as_deref(), Some("model-1"));
        let moved = origin.advance("model-2").await?;
        let (commit, model) = origin
            .load(&cache, None)
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!((commit, model.as_deref()), (moved, Some("model-2")));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_git_role_follows_a_non_default_branch_and_its_remote_updates() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        let (commit, model) = origin
            .load(&cache, Some("dev"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            (commit, model.as_deref()),
            (origin.commits[1].clone(), Some("model-dev"))
        );
        let moved = origin.advance("model-2").await?;
        let (commit, _) = origin
            .load(&cache, Some("main"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(commit, moved);
        let (commit, _) = origin
            .load(&cache, Some("main"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(commit, moved);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_git_role_loads_a_tag_and_a_commit_and_refuses_an_unknown_revision() -> TestResult {
    guard(async {
        let origin = Origin::new().await?;
        let (cache, _keep) = cache()?;
        for rev in ["v1", origin.commits[0].as_str()] {
            let (commit, model) = origin
                .load(&cache, Some(rev))
                .await?
                .map_err(|e| format!("{e:?}"))?;
            assert_eq!(
                (commit, model.as_deref()),
                (origin.commits[0].clone(), Some("model-1")),
                "{rev}"
            );
        }
        let missing = origin.load(&cache, Some("later")).await?;
        assert!(matches!(missing, Err(RoleError::Git { .. })), "{missing:?}");
        git(origin.role.root().as_path(), &["branch", "later", "dev"]).await?;
        let (commit, _) = origin
            .load(&cache, Some("later"))
            .await?
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(commit, origin.commits[1]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn loading_a_path_role_runs_no_program_named_by_its_repository_config() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?;
        role.commit().await?;
        role.file("notes.txt", b"untracked\n").await?;
        let trap = Trap::new().await?;
        trap.arm(role.root().as_path()).await?;
        trap.prove_armed(role.root().as_path()).await?;
        let (cache, _keep) = cache()?;
        let (catalog, _runner) = catalog(&cache);
        catalog.load_path(role.root()).await?;
        assert_eq!(trap.fired(), "");
        Ok(())
    })
    .await
}
