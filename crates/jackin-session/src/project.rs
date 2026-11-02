// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod bundle;
mod facts;
mod gate;
pub(crate) mod git;
mod listing;
mod refuse;
mod sandbox;
mod version;

use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use jackin_core::{
    FileLock, FileMode, HostPath, IdError, LaunchId, ProjectRoot, RepoPath, SandboxName,
    SandboxPath, TerminalText, plain_list, sandbox_layout,
};
use jackin_openshell::{GatewayError, GatewayOp};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::launch::LaunchError;
use crate::lock::{self, LockError};
use crate::project::facts::Facts;
use crate::project::gate::HostGate;
use crate::project::git::HostGit;
use crate::project::listing::Listing;
use crate::project::sandbox::Sandbox;
use crate::service::{Session, SessionDeps};

pub use facts::ObjectFormat;
pub use version::{GitVersion, HostGitError, check_git};

const LISTED_PATHS: usize = 5;

static STAGINGS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProjectStep {
    Describe,
    Head,
    Identity,
    Listing,
    Bundle,
    Empty,
    Init,
    Fetch,
    SetHead,
    Reset,
    Configure,
    RemoveBundle,
    HostTree,
    SandboxTree,
    TreeList,
}

impl fmt::Display for ProjectStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ProjectStep::Describe => "describe",
            ProjectStep::Head => "head",
            ProjectStep::Identity => "identity",
            ProjectStep::Listing => "listing",
            ProjectStep::Bundle => "bundle",
            ProjectStep::Empty => "empty",
            ProjectStep::Init => "init",
            ProjectStep::Fetch => "fetch",
            ProjectStep::SetHead => "set head",
            ProjectStep::Reset => "reset",
            ProjectStep::Configure => "configure",
            ProjectStep::RemoveBundle => "remove bundle",
            ProjectStep::HostTree => "host tree",
            ProjectStep::SandboxTree => "sandbox tree",
            ProjectStep::TreeList => "tree list",
        })
    }
}

#[derive(Debug, Error)]
pub enum UnsupportedProject {
    #[error(
        "the project has skip-worktree entries (a sparse checkout); run `git sparse-checkout disable`"
    )]
    SparseCheckout,
    #[error("the project has populated submodules {}; run `git submodule deinit --all`", plain_list(.0))]
    PopulatedSubmodules(Vec<RepoPath>),
    #[error("the project has nested git repositories {}; ignore or remove them", plain_list(.0))]
    NestedRepositories(Vec<RepoPath>),
    #[error("the project is a partial clone: {output}")]
    PartialClone { output: TerminalText },
    #[error(
        "listed paths lie below a symlinked directory{}; the upload would copy files from outside the project or fail, so replace the symlink with a real directory or remove the paths",
        sample("paths", .0)
    )]
    SymlinkedPath(Vec<RepoPath>),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TreeId(String);

impl TreeId {
    pub fn parse(text: &str) -> Result<TreeId, IdError> {
        let lowercase_hex = text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if matches!(text.len(), 40 | 64) && lowercase_hex {
            Ok(TreeId(text.to_owned()))
        } else {
            Err(IdError::BadFormat {
                kind: "tree ID",
                expected: "40 or 64 lowercase hex digits",
            })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TreeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeMismatch {
    pub host: TreeId,
    pub sandbox: TreeId,
    pub host_only: Vec<RepoPath>,
    pub sandbox_only: Vec<RepoPath>,
    pub differing: Vec<RepoPath>,
    pub autocrlf: Option<String>,
}

fn sample(label: &str, paths: &[RepoPath]) -> String {
    if paths.is_empty() {
        return String::new();
    }
    let shown: Vec<&str> = paths
        .iter()
        .take(LISTED_PATHS)
        .map(RepoPath::as_str)
        .collect();
    let more = paths.len().saturating_sub(LISTED_PATHS);
    let tail = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    format!("; {label}: {}{tail}", shown.join(", "))
}

impl fmt::Display for TreeMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the uploaded project tree {} differs from the host tree {}{}{}{}",
            self.sandbox,
            self.host,
            sample("only on the host", &self.host_only),
            sample("only in the sandbox", &self.sandbox_only),
            sample("different content", &self.differing),
        )?;
        if let Some(value) = &self.autocrlf {
            write!(
                f,
                "; core.autocrlf is {value:?} on the host, a likely cause: set `text eol=...` in .gitattributes instead"
            )?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct Staged {
    dir: HostPath,
    bundle: Option<HostPath>,
    facts: Facts,
    listing: Listing,
}

pub(crate) fn host_git<'a>(
    deps: &'a SessionDeps,
    root: &'a HostPath,
    cancel: &'a CancellationToken,
) -> HostGit<'a> {
    HostGit {
        runner: deps.host.runner.as_ref(),
        root,
        cancel,
    }
}

pub(crate) async fn hold(
    deps: &SessionDeps,
    launch: &LaunchId,
    cancel: &CancellationToken,
) -> Result<FileLock, LaunchError> {
    lock::hold(deps, &format!("upload-{launch}.lock"), cancel)
        .await
        .map_err(|error| match error {
            LockError::Cancelled => GatewayError::Cancelled {
                op: GatewayOp::UploadTree,
            }
            .into(),
            LockError::Fs(error) => LaunchError::Fs(error),
            LockError::Path(error) => LaunchError::Path(error),
        })
}

pub(crate) async fn precheck(
    deps: &SessionDeps,
    project: &ProjectRoot,
    launch: &LaunchId,
    cancel: &CancellationToken,
) -> Result<(), LaunchError> {
    check_git(deps.host.runner.as_ref(), cancel).await?;
    let dir = staging_dir(deps, launch)?;
    let staged = stage(deps, project, &dir, cancel).await;
    release(deps, &dir).await;
    staged.map(drop)
}

pub(crate) fn staging_dir(deps: &SessionDeps, launch: &LaunchId) -> Result<HostPath, LaunchError> {
    let name = format!(
        "upload-{launch}-{}-{}",
        std::process::id(),
        STAGINGS.fetch_add(1, Ordering::Relaxed)
    );
    Ok(HostPath::resolve(Path::new(&name), &deps.cache_dir)?)
}

pub(crate) async fn stage(
    deps: &SessionDeps,
    project: &ProjectRoot,
    dir: &HostPath,
    cancel: &CancellationToken,
) -> Result<Staged, LaunchError> {
    let git = host_git(deps, project.path(), cancel);
    let fs = deps.host.fs.as_ref();
    refuse::check(&git, fs).await?;
    let facts = facts::read(&git, fs).await?;
    let listing = listing::read(&git).await?;
    refuse::symlinked_parents(fs, project.path(), &listing).await?;
    fs.create_private_dir_all(dir).await?;
    let file = HostPath::resolve(Path::new("upload.bundle"), dir)?;
    let bundle = bundle::create(&git, &facts.head, &file).await?;
    Ok(Staged {
        dir: dir.clone(),
        bundle,
        facts,
        listing,
    })
}

pub(crate) async fn release(deps: &SessionDeps, dir: &HostPath) {
    drop(deps.host.fs.remove_dir_all(dir).await);
}

impl Staged {
    pub(crate) async fn upload(
        &self,
        deps: &SessionDeps,
        project: &ProjectRoot,
        sandbox: &SandboxName,
        launch: &LaunchId,
        cancel: &CancellationToken,
    ) -> Result<(), LaunchError> {
        let remote = Sandbox {
            gateway: deps.gateway.as_ref(),
            name: sandbox,
            timeout: deps.limits.upload,
            cancel,
        };
        remote.empty().await?;
        deps.gateway
            .upload_tree(sandbox, project, &sandbox_layout::PROJECT, cancel)
            .await?;
        let landed = self.send_bundle(deps, sandbox, launch, cancel).await?;
        remote.repository(&self.facts, landed.as_ref()).await?;
        self.verify(deps, project, &remote, launch, cancel).await
    }

    async fn send_bundle(
        &self,
        deps: &SessionDeps,
        sandbox: &SandboxName,
        launch: &LaunchId,
        cancel: &CancellationToken,
    ) -> Result<Option<SandboxPath>, LaunchError> {
        let Some(bundle) = &self.bundle else {
            return Ok(None);
        };
        let target = sandbox_layout::BUNDLE_DIR.join(&format!("upload-{launch}.bundle"))?;
        deps.gateway
            .upload_file(sandbox, bundle, &target, cancel)
            .await?;
        Ok(Some(target))
    }

    async fn verify(
        &self,
        deps: &SessionDeps,
        project: &ProjectRoot,
        remote: &Sandbox<'_>,
        launch: &LaunchId,
        cancel: &CancellationToken,
    ) -> Result<(), LaunchError> {
        let gate = sandbox_layout::JACKIN_DIR.join(&format!("gate-{launch}"))?;
        let sandbox_tree = remote.tree(&gate, &self.listing).await?;
        let objects = HostPath::resolve(Path::new("gate-objects"), &self.dir)?;
        deps.host.fs.create_private_dir_all(&objects).await?;
        let paths = HostPath::resolve(Path::new("gate-paths"), &self.dir)?;
        deps.host
            .fs
            .write_new(&paths, self.listing.as_bytes(), FileMode(0o600))
            .await?;
        let host = HostGate {
            git: host_git(deps, project.path(), cancel),
            index: HostPath::resolve(Path::new("gate-index"), &self.dir)?,
            objects,
            paths,
            alternates: &self.facts.objects,
        };
        let host_tree = host.tree().await?;
        if host_tree == sandbox_tree {
            return remote.clear(&gate).await;
        }
        let host_entries = host.entries(&host_tree).await?;
        let sandbox_entries = remote.entries(&gate, &sandbox_tree).await?;
        let autocrlf = host.autocrlf().await?;
        remote.clear(&gate).await?;
        Err(LaunchError::ProjectMismatch(Box::new(gate::mismatch(
            (host_tree, &host_entries),
            (sandbox_tree, &sandbox_entries),
            autocrlf,
        ))))
    }
}

impl Session {
    pub async fn upload_project(
        &self,
        project: &ProjectRoot,
        sandbox: &SandboxName,
        launch: &LaunchId,
        cancel: &CancellationToken,
    ) -> Result<(), LaunchError> {
        check_git(self.deps.host.runner.as_ref(), cancel).await?;
        let _held = hold(&self.deps, launch, cancel).await?;
        let dir = staging_dir(&self.deps, launch)?;
        let result = match stage(&self.deps, project, &dir, cancel).await {
            Ok(staged) => {
                staged
                    .upload(&self.deps, project, sandbox, launch, cancel)
                    .await
            }
            Err(error) => Err(error),
        };
        release(&self.deps, &dir).await;
        result
    }
}
