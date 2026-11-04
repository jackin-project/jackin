// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jackin_core::plain;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::host::{HostCommand, HostError, HostProcess};

const KEEP: [&str; 9] = [
    "PATH",
    "HOME",
    "TMPDIR",
    "USER",
    "LANG",
    "DOCKER_HOST",
    "DOCKER_CONFIG",
    "DOCKER_CONTEXT",
    "SSH_AUTH_SOCK",
];
const GIT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
pub enum ScratchError {
    #[error("scratch i/o at {}: {}", .path.display(), plain(.message))]
    Io { path: PathBuf, message: String },
    #[error("git {} failed in {}: {}", plain(.command), .dir.display(), plain(.message))]
    Git {
        command: String,
        dir: PathBuf,
        message: String,
    },
    #[error(transparent)]
    Host(#[from] HostError),
}

fn io_error(path: &Path, error: &io::Error) -> ScratchError {
    ScratchError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

#[derive(Clone, Debug)]
pub struct Scratch {
    root: PathBuf,
}

#[must_use]
pub fn base_env(parent: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    KEEP.iter()
        .filter_map(|name| {
            parent
                .get(*name)
                .map(|value| ((*name).to_owned(), value.clone()))
        })
        .collect()
}

#[must_use]
pub fn child_env(
    parent: &BTreeMap<String, String>,
    scratch: &Scratch,
    terminal: bool,
) -> BTreeMap<String, String> {
    let mut env = base_env(parent);
    if terminal {
        env.insert("TERM".to_owned(), "xterm-256color".to_owned());
    }
    env.insert(
        "XDG_CONFIG_HOME".to_owned(),
        scratch.xdg_config().display().to_string(),
    );
    env.insert(
        "XDG_CACHE_HOME".to_owned(),
        scratch.xdg_cache().display().to_string(),
    );
    env
}

impl Scratch {
    pub fn create(root: &Path) -> Result<Scratch, ScratchError> {
        let scratch = Scratch {
            root: root.to_path_buf(),
        };
        for dir in [
            scratch.xdg_config(),
            scratch.xdg_cache(),
            scratch.projects(),
            scratch.codex_homes(),
            scratch.role_copies(),
        ] {
            fs::create_dir_all(&dir).map_err(|error| io_error(&dir, &error))?;
        }
        Ok(scratch)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn xdg_config(&self) -> PathBuf {
        self.root.join("xdg/config")
    }

    #[must_use]
    pub fn xdg_cache(&self) -> PathBuf {
        self.root.join("xdg/cache")
    }

    #[must_use]
    pub fn projects(&self) -> PathBuf {
        self.root.join("projects")
    }

    #[must_use]
    pub fn codex_homes(&self) -> PathBuf {
        self.root.join("codex-homes")
    }

    #[must_use]
    pub fn role_copies(&self) -> PathBuf {
        self.root.join("role-copies")
    }

    #[must_use]
    pub fn config_file(&self) -> PathBuf {
        self.xdg_config().join("jackin/config.toml")
    }

    pub fn link_registry(&self, target: &Path) -> Result<(), ScratchError> {
        let link = self.xdg_config().join("openshell");
        std::os::unix::fs::symlink(target, &link).map_err(|error| io_error(&link, &error))
    }

    pub async fn fixture_project(
        &self,
        host: &dyn HostProcess,
        env: &BTreeMap<String, String>,
        name: &str,
        cancel: &CancellationToken,
    ) -> Result<PathBuf, ScratchError> {
        let dir = self.projects().join(name);
        let write = |relative: &str, content: &str, mode: u32| -> Result<(), ScratchError> {
            let path = dir.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| io_error(parent, &error))?;
            }
            fs::write(&path, content).map_err(|error| io_error(&path, &error))?;
            fs::set_permissions(&path, fs::Permissions::from_mode(mode))
                .map_err(|error| io_error(&path, &error))
        };
        write("README.md", &format!("# {name}\n"), 0o644)?;
        write("notes and docs/guide.txt", "guide\n", 0o644)?;
        write("bin/run.sh", "#!/bin/sh\necho run\n", 0o755)?;
        write("src/main.txt", "main\n", 0o644)?;
        write(".gitignore", "ignored.log\n", 0o644)?;
        write("ignored.log", "ignored\n", 0o644)?;
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["add", "-A"],
            vec![
                "-c",
                "user.name=e2e",
                "-c",
                "user.email=e2e@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "-q",
                "-m",
                "fixture",
            ],
        ] {
            let command = HostCommand::new("git", GIT_TIMEOUT)
                .args(args.clone())
                .cwd(&dir)
                .env(env.clone());
            let output = host.run(&command, cancel).await?;
            if !output.ok() {
                return Err(ScratchError::Git {
                    command: args.join(" "),
                    dir: dir.clone(),
                    message: output.stderr_text().trim().to_owned(),
                });
            }
        }
        write("untracked.txt", "untracked\n", 0o644)?;
        Ok(dir)
    }

    pub fn remove(&self) -> Result<(), ScratchError> {
        match fs::remove_dir_all(&self.root) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(&self.root, &error)),
        }
    }
}

pub fn snapshot_tree(dir: &Path) -> Vec<(String, u128, u64)> {
    let mut found = Vec::new();
    walk(dir, dir, &mut found);
    found.sort();
    found
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<(String, u128, u64)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        let nanos = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |elapsed| elapsed.as_nanos());
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        found.push((relative, nanos, meta.len()));
        if meta.is_dir() {
            walk(root, &path, found);
        }
    }
}
