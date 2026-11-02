// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod host;
mod sandbox;

use std::collections::BTreeMap;
use std::path::Path;

use jackin_core::{
    BranchName, CommitId, Config, EntropyError, FsError, HostPath, LabelError, LaunchId, PathError,
    Phase, ProcessError, ProcessStatus, ProjectRoot, SandboxLabels, SandboxName, SandboxPath,
    Sha256, TerminalText, WorkspaceName, sandbox_layout,
};
use jackin_openshell::GatewayError;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use self::host::{HEAD, Host, HostRefs, exported_prefix};
use self::sandbox::{Remote, RemoteError, Repository};
use crate::project::host_git;
use crate::service::Session;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedBranch {
    pub branch: BranchName,
    pub tip: CommitId,
    pub host_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedDetached {
    pub tip: CommitId,
    pub host_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportReport {
    pub sandbox: SandboxName,
    pub bundle: Option<Sha256>,
    pub branches: Vec<ExportedBranch>,
    pub detached: Option<ExportedDetached>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnknownReason {
    NotRunning(Phase),
    WorkspaceUnknown(WorkspaceName),
    NoRepository,
    InspectionFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unexported {
    pub uncommitted: bool,
    pub branches: Vec<BranchName>,
    pub detached: Option<CommitId>,
    pub unusable: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkState {
    Exported,
    Unexported(Unexported),
    Unknown(UnknownReason),
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(transparent)]
    Gateway(GatewayError),
    #[error(transparent)]
    Process(ProcessError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("the labels of {sandbox:?} are invalid")]
    Labels {
        sandbox: SandboxName,
        source: LabelError,
    },
    #[error("sandbox {0:?} was not found")]
    NotFound(SandboxName),
    #[error("sandbox {sandbox:?} is not running: {phase:?}")]
    NotRunning { sandbox: SandboxName, phase: Phase },
    #[error("unknown workspace {0:?}")]
    WorkspaceUnknown(WorkspaceName),
    #[error(transparent)]
    Project(#[from] PathError),
    #[error(transparent)]
    Entropy(#[from] EntropyError),
    #[error("git in the sandbox exited with {exit_code}: {output:?}")]
    SandboxGit {
        exit_code: i32,
        output: TerminalText,
    },
    #[error("git on the host failed with {status:?}: {output:?}")]
    HostGit {
        status: ProcessStatus,
        output: TerminalText,
    },
    #[error("git on the host printed unusable output: {message}")]
    HostOutput { message: String },
    #[error("ref conflict at {host_ref}: {output:?}")]
    RefConflict {
        host_ref: String,
        output: TerminalText,
    },
    #[error("{host_ref} is checked out in the worktree {worktree:?}")]
    RefCheckedOut {
        host_ref: String,
        worktree: HostPath,
    },
    #[error("the sandbox branch {branch:?} cannot be exported")]
    UnusableBranch { branch: String },
    #[error("{host_ref} expected tip {expected:?} but found {found:?}")]
    TipMismatch {
        host_ref: String,
        expected: CommitId,
        found: Option<CommitId>,
    },
    #[error("cancelled")]
    Cancelled,
}

impl From<GatewayError> for ExportError {
    fn from(error: GatewayError) -> ExportError {
        match error {
            GatewayError::Cancelled { .. } => ExportError::Cancelled,
            other => ExportError::Gateway(other),
        }
    }
}

impl From<ProcessError> for ExportError {
    fn from(error: ProcessError) -> ExportError {
        match error {
            ProcessError::Cancelled { .. } => ExportError::Cancelled,
            other => ExportError::Process(other),
        }
    }
}

impl From<RemoteError> for ExportError {
    fn from(error: RemoteError) -> ExportError {
        match error {
            RemoteError::Gateway(error) => error.into(),
            RemoteError::NoRepository => ExportError::SandboxGit {
                exit_code: 128,
                output: TerminalText::sanitize(b"the sandbox has no project repository"),
            },
            RemoteError::Git { exit_code, output } => ExportError::SandboxGit { exit_code, output },
            RemoteError::Malformed => ExportError::SandboxGit {
                exit_code: 0,
                output: TerminalText::sanitize(b"git printed output that jackin cannot read"),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Wanted {
    Branch(BranchName),
    Head,
}

impl Wanted {
    fn source(&self) -> String {
        match self {
            Wanted::Branch(branch) => format!("refs/heads/{}", branch.as_str()),
            Wanted::Head => HEAD.to_owned(),
        }
    }

    fn name(&self) -> &str {
        match self {
            Wanted::Branch(branch) => branch.as_str(),
            Wanted::Head => HEAD,
        }
    }

    fn target(&self, sandbox: &SandboxName) -> String {
        format!("{}{}", exported_prefix(sandbox), self.name())
    }
}

fn changed(repository: &Repository, refs: &HostRefs) -> Vec<Wanted> {
    let mut wanted: Vec<Wanted> = repository
        .branches
        .iter()
        .filter(|(branch, tip)| refs.branches.get(branch.as_str()) != Some(*tip))
        .map(|(branch, _)| Wanted::Branch(branch.clone()))
        .collect();
    if repository
        .detached
        .as_ref()
        .is_some_and(|commit| refs.head.as_ref() != Some(commit))
    {
        wanted.push(Wanted::Head);
    }
    wanted
}

fn classify(repository: &Repository, refs: &HostRefs) -> WorkState {
    let safe = |name: &str, tip: &CommitId| {
        refs.branches.get(name) == Some(tip) || refs.exported.get(name) == Some(tip)
    };
    let branches: Vec<BranchName> = repository
        .branches
        .iter()
        .filter(|(branch, tip)| !safe(branch.as_str(), tip))
        .map(|(branch, _)| branch.clone())
        .collect();
    let detached = repository
        .detached
        .clone()
        .filter(|commit| refs.head.as_ref() != Some(commit) && !safe(HEAD, commit));
    if repository.uncommitted
        || !branches.is_empty()
        || detached.is_some()
        || !repository.unusable.is_empty()
    {
        WorkState::Unexported(Unexported {
            uncommitted: repository.uncommitted,
            branches,
            detached,
            unusable: repository.unusable.clone(),
        })
    } else {
        WorkState::Exported
    }
}

fn nested(target: &str, other: &str) -> bool {
    target.starts_with(&format!("{other}/")) || other.starts_with(&format!("{target}/"))
}

fn conflict(target: &str, existing: &[String]) -> Option<ExportError> {
    existing
        .iter()
        .find(|other| nested(target, other))
        .map(|other| ExportError::RefConflict {
            host_ref: target.to_owned(),
            output: TerminalText::sanitize(format!("{target} conflicts with {other}").as_bytes()),
        })
}

struct Staging {
    dir: HostPath,
    bundle: HostPath,
}

impl Session {
    async fn resolve_project(
        &self,
        config: &Config,
        labels: &SandboxLabels,
    ) -> Result<ProjectRoot, ExportError> {
        let workspace = &labels.planned.workspace;
        let entry = config
            .workspace(workspace)
            .ok_or_else(|| ExportError::WorkspaceUnknown(workspace.clone()))?;
        Ok(ProjectRoot::open(self.deps.host.fs.as_ref(), &entry.path).await?)
    }

    fn remote<'a>(&'a self, name: &'a SandboxName, cancel: &'a CancellationToken) -> Remote<'a> {
        Remote {
            gateway: self.deps.gateway.as_ref(),
            name,
            timeout: self.deps.limits.upload,
            cancel,
        }
    }

    pub async fn export(
        &self,
        config: &Config,
        sandbox: &SandboxName,
        cancel: CancellationToken,
    ) -> Result<ExportReport, ExportError> {
        if cancel.is_cancelled() {
            return Err(ExportError::Cancelled);
        }
        let view = self
            .find_managed(sandbox)
            .await?
            .ok_or_else(|| ExportError::NotFound(sandbox.clone()))?;
        if view.phase != Phase::Ready {
            return Err(ExportError::NotRunning {
                sandbox: sandbox.clone(),
                phase: view.phase,
            });
        }
        let labels =
            SandboxLabels::from_labels(&view.labels).map_err(|source| ExportError::Labels {
                sandbox: sandbox.clone(),
                source,
            })?;
        let project = self.resolve_project(config, &labels).await?;
        let repository = self.remote(sandbox, &cancel).repository().await?;
        if let Some(branch) = repository.unusable.first() {
            return Err(ExportError::UnusableBranch {
                branch: branch.clone(),
            });
        }
        let host = Host {
            git: host_git(&self.deps, project.path(), &cancel),
        };
        let refs = host.refs(sandbox).await?;
        let wanted = changed(&repository, &refs);
        if wanted.is_empty() {
            return Ok(ExportReport {
                sandbox: sandbox.clone(),
                bundle: None,
                branches: Vec::new(),
                detached: None,
            });
        }
        let targets: Vec<String> = wanted.iter().map(|item| item.target(sandbox)).collect();
        guard(&host, &refs, &targets).await?;
        let sources: Vec<String> = wanted.iter().map(Wanted::source).collect();
        let unique = LaunchId::generate()?;
        let remote_bundle = sandbox_layout::BUNDLE_DIR.join(&format!("export-{unique}.bundle"))?;
        let result = self
            .ship(&host, sandbox, &unique, &remote_bundle, &sources, &cancel)
            .await;
        self.remote(sandbox, &CancellationToken::new())
            .discard(&remote_bundle)
            .await;
        result
    }

    async fn ship(
        &self,
        host: &Host<'_>,
        sandbox: &SandboxName,
        unique: &LaunchId,
        remote_bundle: &SandboxPath,
        sources: &[String],
        cancel: &CancellationToken,
    ) -> Result<ExportReport, ExportError> {
        self.remote(sandbox, cancel)
            .bundle(remote_bundle, sources)
            .await?;
        let staging = self.stage(sandbox, unique).await?;
        let result = self
            .fetch_exported(host, sandbox, remote_bundle, &staging, cancel)
            .await;
        drop(self.deps.host.fs.remove_dir_all(&staging.dir).await);
        result
    }

    async fn stage(
        &self,
        sandbox: &SandboxName,
        unique: &LaunchId,
    ) -> Result<Staging, ExportError> {
        let name = format!("export-{sandbox}-{unique}");
        let dir = HostPath::resolve(Path::new(&name), &self.deps.cache_dir)?;
        self.deps.host.fs.create_dir_all(&dir).await?;
        let bundle = HostPath::resolve(Path::new("export.bundle"), &dir)?;
        Ok(Staging { dir, bundle })
    }

    async fn fetch_exported(
        &self,
        host: &Host<'_>,
        sandbox: &SandboxName,
        remote_bundle: &SandboxPath,
        staging: &Staging,
        cancel: &CancellationToken,
    ) -> Result<ExportReport, ExportError> {
        let digest = self
            .deps
            .gateway
            .download_file(sandbox, remote_bundle, &staging.bundle, cancel)
            .await?;
        host.verify(&staging.bundle).await?;
        let heads = host.heads(&staging.bundle).await?;
        let prefix = exported_prefix(sandbox);
        let refspecs: Vec<String> = heads
            .keys()
            .map(|source| {
                let name = source.strip_prefix("refs/heads/").unwrap_or(source);
                format!("+{source}:{prefix}{name}")
            })
            .collect();
        host.fetch(&staging.bundle, &refspecs).await?;
        let landed = host.refs(sandbox).await?;
        report(sandbox, digest, &heads, &landed)
    }
}

async fn guard(host: &Host<'_>, refs: &HostRefs, targets: &[String]) -> Result<(), ExportError> {
    if let Some(error) = targets
        .iter()
        .find_map(|target| conflict(target, &refs.every))
    {
        return Err(error);
    }
    let worktrees = host.worktrees().await?;
    for target in targets {
        if let Some(tree) = worktrees
            .iter()
            .find(|tree| tree.branch.as_deref() == Some(target.as_str()))
        {
            return Err(ExportError::RefCheckedOut {
                host_ref: target.clone(),
                worktree: tree.path.clone(),
            });
        }
    }
    Ok(())
}

fn report(
    sandbox: &SandboxName,
    digest: Sha256,
    heads: &BTreeMap<String, CommitId>,
    landed: &HostRefs,
) -> Result<ExportReport, ExportError> {
    let prefix = exported_prefix(sandbox);
    let mut branches = Vec::new();
    let mut detached = None;
    for (source, expected) in heads {
        let name = source.strip_prefix("refs/heads/").unwrap_or(source);
        let host_ref = format!("{prefix}{name}");
        let found = landed.exported.get(name).cloned();
        if found.as_ref() != Some(expected) {
            return Err(ExportError::TipMismatch {
                host_ref,
                expected: expected.clone(),
                found,
            });
        }
        if source == HEAD {
            detached = Some(ExportedDetached {
                tip: expected.clone(),
                host_ref,
            });
        } else {
            let branch = BranchName::parse(name).map_err(|_| ExportError::UnusableBranch {
                branch: name.to_owned(),
            })?;
            branches.push(ExportedBranch {
                branch,
                tip: expected.clone(),
                host_ref,
            });
        }
    }
    Ok(ExportReport {
        sandbox: sandbox.clone(),
        bundle: Some(digest),
        branches,
        detached,
    })
}

impl Session {
    pub async fn work_state(&self, config: &Config, sandbox: &SandboxName) -> WorkState {
        match self.find_managed(sandbox).await {
            Ok(Some(view)) => {
                self.work_state_of(config, sandbox, view.phase, &view.labels)
                    .await
            }
            Ok(None) | Err(_) => WorkState::Unknown(UnknownReason::InspectionFailed),
        }
    }

    pub(crate) async fn work_state_of(
        &self,
        config: &Config,
        sandbox: &SandboxName,
        phase: Phase,
        labels: &jackin_core::Labels,
    ) -> WorkState {
        match self.inspected(config, sandbox, phase, labels).await {
            Ok(state) => state,
            Err(reason) => WorkState::Unknown(reason),
        }
    }

    async fn inspected(
        &self,
        config: &Config,
        sandbox: &SandboxName,
        phase: Phase,
        labels: &jackin_core::Labels,
    ) -> Result<WorkState, UnknownReason> {
        if phase != Phase::Ready {
            return Err(UnknownReason::NotRunning(phase));
        }
        let labels =
            SandboxLabels::from_labels(labels).map_err(|_| UnknownReason::InspectionFailed)?;
        let workspace = &labels.planned.workspace;
        let entry = config
            .workspace(workspace)
            .ok_or_else(|| UnknownReason::WorkspaceUnknown(workspace.clone()))?;
        let cancel = CancellationToken::new();
        let repository = match self.remote(sandbox, &cancel).repository().await {
            Ok(repository) => repository,
            Err(RemoteError::NoRepository) => return Err(UnknownReason::NoRepository),
            Err(_) => return Err(UnknownReason::InspectionFailed),
        };
        let project = ProjectRoot::open(self.deps.host.fs.as_ref(), &entry.path)
            .await
            .map_err(|_| UnknownReason::InspectionFailed)?;
        let host = Host {
            git: host_git(&self.deps, project.path(), &cancel),
        };
        let refs = host
            .refs(sandbox)
            .await
            .map_err(|_| UnknownReason::InspectionFailed)?;
        Ok(classify(&repository, &refs))
    }
}
