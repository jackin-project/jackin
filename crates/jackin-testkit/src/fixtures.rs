// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jackin_core::{CommitId, FileSystem, HookKind, HostPath, ProjectRoot, StdFileSystem};
use tempfile::TempDir;
use tokio::process::Command;

pub type FixtureError = Box<dyn Error>;

const DOCKERFILE: &str = "FROM construct\n";
const POLICY: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only:
    - /bin
    - /usr
  read_write:
    - /tmp
    - /sandbox
landlock:
  compatibility: best_effort
process:
  run_as_user: sandbox
  run_as_group: sandbox
network_policies:
  github:
    name: github
    endpoints:
      - host: github.com
        port: 443
    binaries:
      - path: /usr/bin/git
";

#[derive(Clone, Debug)]
struct EnvEntry {
    name: String,
    default: Option<String>,
    prompt: Option<String>,
}

#[derive(Clone, Debug)]
struct HookSpec {
    path: String,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct RoleFixture {
    dir: Arc<TempDir>,
    root: HostPath,
    name: String,
    model: Option<String>,
    docker: bool,
    env: Vec<EnvEntry>,
    dockerfile: (String, String),
    policy: (String, String),
    setup: Option<HookSpec>,
    preflight: Option<HookSpec>,
    manifest: Option<String>,
    gitignore: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RepoFixture {
    dir: Arc<TempDir>,
    root: HostPath,
}

async fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, FixtureError> {
    let output = Command::new("git")
        .args(["-c", "user.name=jackin-test"])
        .args(["-c", "user.email=test@example.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(["-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .stdin(std::process::Stdio::null())
        .output()
        .await?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

async fn commit_all(dir: &Path) -> Result<CommitId, FixtureError> {
    git(dir, &["add", "-A"]).await?;
    git(dir, &["commit", "-q", "--allow-empty", "-m", "fixture"]).await?;
    let head = git(dir, &["rev-parse", "HEAD"]).await?;
    Ok(CommitId::parse(String::from_utf8(head)?.trim())?)
}

async fn write_file(root: &Path, path: &str, bytes: &[u8]) -> Result<(), FixtureError> {
    let target = root.join(path);
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(target, bytes).await?;
    Ok(())
}

fn temp_root(prefix: &str) -> Result<(Arc<TempDir>, HostPath, PathBuf), FixtureError> {
    let dir = tempfile::Builder::new().prefix(prefix).tempdir()?;
    let canonical = std::fs::canonicalize(dir.path())?;
    let root = HostPath::parse(&canonical)?;
    Ok((Arc::new(dir), root, canonical))
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

impl RoleFixture {
    pub fn new(name: &str) -> Result<RoleFixture, FixtureError> {
        let (dir, root, _) = temp_root("jackin-role-")?;
        Ok(RoleFixture {
            dir,
            root,
            name: name.to_owned(),
            model: None,
            docker: false,
            env: Vec::new(),
            dockerfile: ("Dockerfile".to_owned(), DOCKERFILE.to_owned()),
            policy: ("policy.yaml".to_owned(), POLICY.to_owned()),
            setup: None,
            preflight: None,
            manifest: None,
            gitignore: None,
        })
    }

    pub fn root(&self) -> &HostPath {
        &self.root
    }

    #[must_use]
    pub fn model(mut self, model: &str) -> RoleFixture {
        self.model = Some(model.to_owned());
        self
    }

    #[must_use]
    pub fn docker(mut self, enabled: bool) -> RoleFixture {
        self.docker = enabled;
        self
    }

    #[must_use]
    pub fn env(mut self, name: &str, default: Option<&str>, prompt: Option<&str>) -> RoleFixture {
        self.env.push(EnvEntry {
            name: name.to_owned(),
            default: default.map(str::to_owned),
            prompt: prompt.map(str::to_owned),
        });
        self
    }

    #[must_use]
    pub fn hook(mut self, kind: HookKind, bytes: &[u8]) -> RoleFixture {
        let slot = match kind {
            HookKind::Setup => &mut self.setup,
            HookKind::Preflight => &mut self.preflight,
        };
        let path = slot.as_ref().map_or_else(
            || {
                match kind {
                    HookKind::Setup => "hooks/setup.sh",
                    HookKind::Preflight => "hooks/preflight.sh",
                }
                .to_owned()
            },
            |hook| hook.path.clone(),
        );
        *slot = Some(HookSpec {
            path,
            bytes: bytes.to_vec(),
        });
        self
    }

    #[must_use]
    pub fn hook_path(mut self, kind: HookKind, path: &str) -> RoleFixture {
        let slot = match kind {
            HookKind::Setup => &mut self.setup,
            HookKind::Preflight => &mut self.preflight,
        };
        match slot {
            Some(hook) => hook.path = path.to_owned(),
            None => {
                *slot = Some(HookSpec {
                    path: path.to_owned(),
                    bytes: b"#!/bin/sh\n".to_vec(),
                });
            }
        }
        self
    }

    #[must_use]
    pub fn dockerfile(mut self, text: &str) -> RoleFixture {
        self.dockerfile.1 = text.to_owned();
        self
    }

    #[must_use]
    pub fn dockerfile_path(mut self, path: &str) -> RoleFixture {
        self.dockerfile.0 = path.to_owned();
        self
    }

    #[must_use]
    pub fn policy_yaml(mut self, text: &str) -> RoleFixture {
        self.policy.1 = text.to_owned();
        self
    }

    #[must_use]
    pub fn policy_path(mut self, path: &str) -> RoleFixture {
        self.policy.0 = path.to_owned();
        self
    }

    #[must_use]
    pub fn manifest_text(mut self, text: String) -> RoleFixture {
        self.manifest = Some(text);
        self
    }

    #[must_use]
    pub fn gitignore(mut self, text: &str) -> RoleFixture {
        self.gitignore = Some(text.to_owned());
        self
    }

    pub async fn file(&self, path: &str, bytes: &[u8]) -> Result<(), FixtureError> {
        write_file(self.root.as_path(), path, bytes).await
    }

    pub async fn remove(&self, path: &str) -> Result<(), FixtureError> {
        tokio::fs::remove_file(self.root.as_path().join(path)).await?;
        Ok(())
    }

    pub async fn symlink(&self, path: &str, target: &Path) -> Result<(), FixtureError> {
        let link = self.root.as_path().join(path);
        if let Some(parent) = link.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::symlink(target, link).await?;
        Ok(())
    }

    fn manifest_text_rendered(&self) -> String {
        let mut text = format!(
            "schema = 1\n\n[role]\nname = {}\ndockerfile = {}\npolicy = {}\n",
            quote(&self.name),
            quote(&self.dockerfile.0),
            quote(&self.policy.0)
        );
        if let Some(model) = &self.model {
            text.push_str(&format!("\n[codex]\nmodel = {}\n", quote(model)));
        }
        text.push_str(&format!("\n[docker]\nenabled = {}\n", self.docker));
        if self.setup.is_some() || self.preflight.is_some() {
            text.push_str("\n[hooks]\n");
            for (name, hook) in [("setup", &self.setup), ("preflight", &self.preflight)] {
                if let Some(hook) = hook {
                    text.push_str(&format!("{name} = {}\n", quote(&hook.path)));
                }
            }
        }
        for entry in &self.env {
            text.push_str(&format!("\n[env.{}]\n", entry.name));
            if let Some(default) = &entry.default {
                text.push_str(&format!("default = {}\n", quote(default)));
            }
            if let Some(prompt) = &entry.prompt {
                text.push_str(&format!("prompt = {}\n", quote(prompt)));
            }
        }
        text
    }

    pub async fn commit(&self) -> Result<CommitId, FixtureError> {
        let root = self.root.as_path();
        let manifest = self
            .manifest
            .clone()
            .unwrap_or_else(|| self.manifest_text_rendered());
        write_file(root, "jackin.role.toml", manifest.as_bytes()).await?;
        write_file(root, &self.dockerfile.0, self.dockerfile.1.as_bytes()).await?;
        write_file(root, &self.policy.0, self.policy.1.as_bytes()).await?;
        for hook in [&self.setup, &self.preflight].into_iter().flatten() {
            write_file(root, &hook.path, &hook.bytes).await?;
        }
        if let Some(text) = &self.gitignore {
            write_file(root, ".gitignore", text.as_bytes()).await?;
        }
        if tokio::fs::symlink_metadata(root.join(".git"))
            .await
            .is_err()
        {
            git(root, &["init", "-q"]).await?;
        }
        let _keep = &self.dir;
        commit_all(root).await
    }
}

impl RepoFixture {
    pub async fn new() -> Result<RepoFixture, FixtureError> {
        let (dir, root, canonical) = temp_root("jackin-project-")?;
        git(&canonical, &["init", "-q"]).await?;
        Ok(RepoFixture { dir, root })
    }

    pub fn root(&self) -> &HostPath {
        &self.root
    }

    pub async fn project(&self) -> Result<ProjectRoot, FixtureError> {
        let fs: &dyn FileSystem = &StdFileSystem::default();
        let _keep = &self.dir;
        Ok(ProjectRoot::open(fs, &self.root).await?)
    }

    pub async fn file(&self, path: &str, bytes: &[u8]) -> Result<(), FixtureError> {
        write_file(self.root.as_path(), path, bytes).await
    }

    pub async fn ignore(&self, text: &str) -> Result<(), FixtureError> {
        write_file(self.root.as_path(), ".gitignore", text.as_bytes()).await
    }

    pub async fn commit(&self) -> Result<CommitId, FixtureError> {
        commit_all(self.root.as_path()).await
    }
}
