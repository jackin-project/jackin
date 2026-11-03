// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::ffi::OsStr;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use crate::boundary::{FileKind, FileSystem, FsError};
use crate::display::plain;
pub(crate) const PROJECT_PATH: &str = "/sandbox/project";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct HostPath(PathBuf);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SandboxPath(Cow<'static, str>);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RepoPath(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ProjectRoot(HostPath);

#[derive(Debug, Error)]
pub enum PathError {
    #[error("path is not absolute: {}", plain(.0))]
    NotAbsolute(String),
    #[error("path is not relative: {}", plain(.0))]
    NotRelative(String),
    #[error("path contains a parent segment: {}", plain(.0))]
    ParentSegment(String),
    #[error("path contains an empty segment: {}", plain(.0))]
    EmptySegment(String),
    #[error("not a directory: {0}")]
    NotADirectory(HostPath),
    #[error("not a git work tree: {0}")]
    NotAGitWorkTree(HostPath),
    #[error(
        "the .git of {0} is a symlink or another kind of file; use a normal repository, a linked worktree, or a submodule checkout"
    )]
    UnsupportedGitEntry(HostPath),
    #[error(transparent)]
    Fs(#[from] FsError),
}

fn normalize_host(path: &Path) -> Result<PathBuf, PathError> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(PathError::ParentSegment(path.display().to_string()));
            }
            Component::Normal(name) => out.push(name),
        }
    }
    Ok(out)
}

impl fmt::Display for HostPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&plain(self.0.display()))
    }
}

impl fmt::Display for SandboxPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&plain(self.as_str()))
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&plain(&self.0))
    }
}

impl fmt::Display for ProjectRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl HostPath {
    pub fn parse(path: &Path) -> Result<HostPath, PathError> {
        if path.is_absolute() {
            normalize_host(path).map(HostPath)
        } else {
            Err(PathError::NotAbsolute(path.display().to_string()))
        }
    }

    pub fn resolve(path: &Path, cwd: &HostPath) -> Result<HostPath, PathError> {
        normalize_host(&cwd.0.join(path)).map(HostPath)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn join_name(&self, name: &str) -> HostPath {
        HostPath(self.0.join(name))
    }

    pub(crate) fn with_file_name(&self, name: &OsStr) -> HostPath {
        HostPath(self.0.with_file_name(name))
    }

    pub(crate) fn parent(&self) -> Option<HostPath> {
        self.0.parent().map(|dir| HostPath(dir.to_path_buf()))
    }

    pub(crate) fn file_name(&self) -> Option<&OsStr> {
        self.0.file_name()
    }
}

impl SandboxPath {
    pub const fn from_static(path: &'static str) -> SandboxPath {
        SandboxPath(Cow::Borrowed(path))
    }

    pub fn parse(text: &str) -> Result<SandboxPath, PathError> {
        let Some(rest) = text.strip_prefix('/') else {
            return Err(PathError::NotAbsolute(text.to_owned()));
        };
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        let mut segments = Vec::new();
        if !rest.is_empty() {
            for segment in rest.split('/') {
                match segment {
                    "" => return Err(PathError::EmptySegment(text.to_owned())),
                    ".." => return Err(PathError::ParentSegment(text.to_owned())),
                    "." => {}
                    other => segments.push(other),
                }
            }
        }
        Ok(SandboxPath(Cow::Owned(format!("/{}", segments.join("/")))))
    }

    pub fn join(&self, segment: &str) -> Result<SandboxPath, PathError> {
        if segment.is_empty() {
            return Err(PathError::EmptySegment(segment.to_owned()));
        }
        let base = self.as_str().trim_end_matches('/');
        SandboxPath::parse(&format!("{base}/{segment}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl RepoPath {
    pub fn parse(text: &str) -> Result<RepoPath, PathError> {
        if text.starts_with('/') {
            return Err(PathError::NotRelative(text.to_owned()));
        }
        for segment in text.split('/') {
            match segment {
                "" | "." => return Err(PathError::EmptySegment(text.to_owned())),
                ".." => return Err(PathError::ParentSegment(text.to_owned())),
                _ => {}
            }
        }
        Ok(RepoPath(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ProjectRoot {
    pub async fn open(fs: &dyn FileSystem, path: &HostPath) -> Result<ProjectRoot, PathError> {
        if fs.stat(path).await? != Some(FileKind::Dir) {
            return Err(PathError::NotADirectory(path.clone()));
        }
        let git = path.join_name(".git");
        match fs.stat(&git).await? {
            None => Err(PathError::NotAGitWorkTree(path.clone())),
            Some(FileKind::Dir) => Ok(ProjectRoot(path.clone())),
            Some(FileKind::File) => Ok(ProjectRoot(path.clone())),
            Some(_) => Err(PathError::UnsupportedGitEntry(path.clone())),
        }
    }

    pub fn path(&self) -> &HostPath {
        &self.0
    }
}

pub mod sandbox_layout {
    use std::borrow::Cow;

    use super::SandboxPath;
    use crate::ids::LaunchId;
    use crate::manifest::HookKind;

    pub static PROJECT: SandboxPath = SandboxPath::from_static(super::PROJECT_PATH);
    pub static JACKIN_DIR: SandboxPath = SandboxPath::from_static("/sandbox/.jackin");
    pub static RUN_STATE: SandboxPath = SandboxPath::from_static("/sandbox/.jackin/run/state");
    pub static RUN_START: SandboxPath = SandboxPath::from_static("/sandbox/.jackin/run/start");
    pub static DOCKER_DIR: SandboxPath = SandboxPath::from_static("/sandbox/.jackin/docker");
    pub static HOOKS_DIR: SandboxPath = SandboxPath::from_static("/usr/local/lib/jackin/hooks");
    pub static BUNDLE_DIR: SandboxPath = SandboxPath::from_static("/sandbox/.jackin/bundle");
    pub static GIT: SandboxPath = SandboxPath::from_static("/usr/bin/git");
    pub static PRINTENV: SandboxPath = SandboxPath::from_static("/usr/bin/printenv");
    pub static SHA256SUM: SandboxPath = SandboxPath::from_static("/usr/bin/sha256sum");
    pub static STAT: SandboxPath = SandboxPath::from_static("/usr/bin/stat");

    pub fn fixed() -> [&'static SandboxPath; 11] {
        [
            &PROJECT,
            &JACKIN_DIR,
            &RUN_STATE,
            &RUN_START,
            &DOCKER_DIR,
            &HOOKS_DIR,
            &BUNDLE_DIR,
            &GIT,
            &PRINTENV,
            &SHA256SUM,
            &STAT,
        ]
    }

    fn under(dir: &SandboxPath, name: String) -> SandboxPath {
        SandboxPath(Cow::Owned(format!("{}/{name}", dir.as_str())))
    }

    pub fn uploaded_marker(launch: &LaunchId) -> SandboxPath {
        under(&JACKIN_DIR, format!("uploaded-{launch}"))
    }

    pub fn setup_done_marker(launch: &LaunchId) -> SandboxPath {
        under(&JACKIN_DIR, format!("setup-done-{launch}"))
    }

    pub fn env_file(launch: &LaunchId) -> SandboxPath {
        under(&JACKIN_DIR, format!("env-{launch}"))
    }

    pub fn docker_file(name: &str) -> SandboxPath {
        under(&DOCKER_DIR, name.to_owned())
    }

    pub fn hook_path(kind: HookKind) -> SandboxPath {
        let name = match kind {
            HookKind::Setup => "setup",
            HookKind::Preflight => "preflight",
        };
        under(&HOOKS_DIR, name.to_owned())
    }
}
