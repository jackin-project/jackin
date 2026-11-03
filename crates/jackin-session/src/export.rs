// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod bundle;
mod diff;
mod host;
mod sandbox;

pub use diff::*;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use jackin_core::{
    BranchName, CommitId, Config, EntropyError, FsError, HostPath, LabelError, LaunchId, PathError,
    Phase, ProcessError, ProcessStatus, ProjectRoot, SandboxLabels, SandboxName, SandboxPath,
    Sha256, TerminalText, WorkspaceName, plain, plain_list, sandbox_layout,
};
use jackin_openshell::GatewayError;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

pub use self::bundle::{BundleRefusal, EntryKind};
use self::bundle::{Limits, MAX_HEADS};
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
pub struct ExportedWorktree {
    pub worktree: BranchName,
    pub tip: CommitId,
    pub host_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportReport {
    pub sandbox: SandboxName,
    pub bundle: Option<Sha256>,
    pub branches: Vec<ExportedBranch>,
    pub detached: Option<ExportedDetached>,
    pub worktrees: Vec<ExportedWorktree>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnknownReason {
    NotRunning(Phase),
    WorkspaceUnknown(WorkspaceName),
    NoRepository,
    InspectionFailed,
}

impl fmt::Display for UnknownReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnknownReason::NotRunning(phase) => write!(f, "cannot be inspected ({phase})"),
            UnknownReason::WorkspaceUnknown(workspace) => {
                write!(
                    f,
                    "cannot be inspected (workspace {workspace} is not in the config)"
                )
            }
            UnknownReason::NoRepository => f.write_str("cannot be inspected (no repository)"),
            UnknownReason::InspectionFailed => f.write_str("cannot be inspected"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unexported {
    pub uncommitted: usize,
    pub branches: Vec<BranchName>,
    pub detached: Option<CommitId>,
    pub worktrees: Vec<BranchName>,
    pub unusable: Vec<TerminalText>,
}

impl Unexported {
    pub fn unpushed(&self) -> usize {
        self.branches.len() + self.worktrees.len() + usize::from(self.detached.is_some())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkState {
    Exported,
    Unexported(Unexported),
    Unknown(UnknownReason),
}

impl fmt::Display for WorkState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkState::Exported => f.write_str("exported"),
            WorkState::Unexported(found) => {
                let mut parts = Vec::new();
                if found.uncommitted > 0 {
                    parts.push("uncommitted changes".to_owned());
                }
                for branch in &found.branches {
                    parts.push(format!("branch {branch} not exported"));
                }
                if let Some(commit) = &found.detached {
                    parts.push(format!("detached HEAD {commit} not exported"));
                }
                for name in &found.worktrees {
                    parts.push(format!("worktree {name} not exported"));
                }
                for name in &found.unusable {
                    parts.push(format!("branch {} cannot be exported", plain(name)));
                }
                f.write_str(&parts.join(", "))
            }
            WorkState::Unknown(reason) => reason.fmt(f),
        }
    }
}

fn tip_text(found: Option<&CommitId>) -> String {
    found.map_or_else(|| "no commit".to_owned(), ToString::to_string)
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(transparent)]
    Gateway(GatewayError),
    #[error(transparent)]
    Process(ProcessError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("the labels of {sandbox} are invalid")]
    Labels {
        sandbox: SandboxName,
        source: LabelError,
    },
    #[error("sandbox {0} was not found")]
    NotFound(SandboxName),
    #[error("sandbox {sandbox} is not running: {phase}")]
    NotRunning { sandbox: SandboxName, phase: Phase },
    #[error("unknown workspace {0}")]
    WorkspaceUnknown(WorkspaceName),
    #[error(transparent)]
    Project(#[from] PathError),
    #[error(transparent)]
    Entropy(#[from] EntropyError),
    #[error("sandbox {sandbox} has no project repository")]
    NoRepository { sandbox: SandboxName },
    #[error("git in {sandbox} printed unusable output: {what}")]
    SandboxOutput {
        sandbox: SandboxName,
        what: &'static str,
    },
    #[error("git in the sandbox exited with {exit_code}: {output}")]
    SandboxGit {
        exit_code: i32,
        output: TerminalText,
    },
    #[error("git on the host failed with {status}: {output}")]
    HostGit {
        status: ProcessStatus,
        output: TerminalText,
    },
    #[error("git on the host printed unusable output: {}", plain(.message))]
    HostOutput { message: String },
    #[error("ref conflict at {}: {output}", plain(.host_ref))]
    RefConflict {
        host_ref: String,
        output: TerminalText,
    },
    #[error("{} is checked out in the worktree {worktree}", plain(.host_ref))]
    RefCheckedOut {
        host_ref: String,
        worktree: HostPath,
    },
    #[error("the sandbox branch {} cannot be exported", plain(.branch))]
    UnusableBranch { branch: TerminalText },
    #[error("the bundle heads differ from the requested set: extra {}; missing {}", plain_list(.extra), plain_list(.missing))]
    BundleHeads {
        extra: Vec<TerminalText>,
        missing: Vec<String>,
    },
    #[error("{count} heads need export and the limit is {limit}")]
    TooManyHeads { count: usize, limit: usize },
    #[error("the sandbox bundle was refused: {0}")]
    BundleRefused(BundleRefusal),
    #[error("the sandbox bundle is {size} bytes and the limit is {limit}")]
    BundleTooLarge { size: u64, limit: u64 },
    #[error("{} expected tip {expected} but found {}", plain(.host_ref), tip_text(.found.as_ref()))]
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

impl ExportError {
    fn remote(sandbox: &SandboxName, error: RemoteError) -> ExportError {
        match error {
            RemoteError::Gateway(error) => error.into(),
            RemoteError::NoRepository => ExportError::NoRepository {
                sandbox: sandbox.clone(),
            },
            RemoteError::Git { exit_code, output } => ExportError::SandboxGit { exit_code, output },
            RemoteError::Malformed(what) => ExportError::SandboxOutput {
                sandbox: sandbox.clone(),
                what,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Wanted {
    Branch(BranchName),
    Head,
    Worktree(BranchName),
}

impl Wanted {
    fn source(&self) -> String {
        match self {
            Wanted::Branch(branch) => format!("refs/heads/{}", branch.as_str()),
            Wanted::Head => HEAD.to_owned(),
            Wanted::Worktree(name) => format!("worktrees/{}/HEAD", name.as_str()),
        }
    }

    fn name(&self) -> String {
        match self {
            Wanted::Branch(branch) => branch.as_str().to_owned(),
            Wanted::Head => HEAD.to_owned(),
            Wanted::Worktree(name) => format!("worktree/{}", name.as_str()),
        }
    }

    fn target(&self, sandbox: &SandboxName) -> String {
        format!("{}{}", exported_prefix(sandbox), self.name())
    }
}

fn heads(repository: &Repository) -> Vec<(Wanted, CommitId)> {
    let mut heads: Vec<(Wanted, CommitId)> = repository
        .branches
        .iter()
        .map(|(branch, tip)| (Wanted::Branch(branch.clone()), tip.clone()))
        .collect();
    if let Some(tip) = &repository.detached {
        heads.push((Wanted::Head, tip.clone()));
    }
    heads.extend(
        repository
            .linked
            .iter()
            .map(|linked| (Wanted::Worktree(linked.name.clone()), linked.tip.clone())),
    );
    heads
}

fn on_host(wanted: &Wanted, tip: &CommitId, refs: &HostRefs) -> bool {
    let host = match wanted {
        Wanted::Branch(branch) => refs.branches.get(branch.as_str()),
        Wanted::Head | Wanted::Worktree(_) => refs.head.as_ref(),
    };
    host == Some(tip)
}

fn changed(repository: &Repository, refs: &HostRefs) -> Vec<(Wanted, CommitId)> {
    heads(repository)
        .into_iter()
        .filter(|(wanted, tip)| !on_host(wanted, tip, refs))
        .collect()
}

fn pending(repository: &Repository, refs: &HostRefs) -> Vec<(Wanted, CommitId)> {
    changed(repository, refs)
        .into_iter()
        .filter(|(wanted, tip)| refs.exported.get(&wanted.name()) != Some(tip))
        .collect()
}

fn classify(repository: &Repository, uncommitted: usize, refs: &HostRefs) -> WorkState {
    let mut state = Unexported {
        uncommitted,
        branches: Vec::new(),
        detached: None,
        worktrees: Vec::new(),
        unusable: repository.unusable.clone(),
    };
    for (wanted, tip) in pending(repository, refs) {
        match wanted {
            Wanted::Branch(branch) => state.branches.push(branch),
            Wanted::Head => state.detached = Some(tip),
            Wanted::Worktree(name) => state.worktrees.push(name),
        }
    }
    if state.uncommitted > 0
        || !state.branches.is_empty()
        || state.detached.is_some()
        || !state.worktrees.is_empty()
        || !state.unusable.is_empty()
    {
        WorkState::Unexported(state)
    } else {
        WorkState::Exported
    }
}

fn nested(target: &str, other: &str) -> bool {
    target.starts_with(&format!("{other}/")) || other.starts_with(&format!("{target}/"))
}

fn clash(target: &str, other: &str) -> ExportError {
    ExportError::RefConflict {
        host_ref: target.to_owned(),
        output: TerminalText::sanitize(format!("{target} conflicts with {other}").as_bytes()),
    }
}

fn conflict(target: &str, existing: &[String]) -> Option<ExportError> {
    existing
        .iter()
        .find(|other| nested(target, other))
        .map(|other| clash(target, other))
}

fn collision(
    sandbox: &SandboxName,
    repository: &Repository,
    wanted: &[(Wanted, CommitId)],
) -> Option<ExportError> {
    let every: Vec<(Wanted, CommitId)> = heads(repository);
    wanted.iter().find_map(|(item, _)| {
        let target = item.target(sandbox);
        every
            .iter()
            .filter(|(other, _)| other != item)
            .map(|(other, _)| other.target(sandbox))
            .find(|other| *other == target || nested(&target, other))
            .map(|other| clash(&target, &other))
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
        let repository = self
            .remote(sandbox, &cancel)
            .repository()
            .await
            .map_err(|error| ExportError::remote(sandbox, error))?;
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
                worktrees: Vec::new(),
            });
        }
        if wanted.len() > MAX_HEADS {
            return Err(ExportError::TooManyHeads {
                count: wanted.len(),
                limit: MAX_HEADS,
            });
        }
        if let Some(error) = collision(sandbox, &repository, &wanted) {
            return Err(error);
        }
        let items: Vec<Wanted> = wanted.into_iter().map(|(item, _)| item).collect();
        let targets: Vec<String> = items.iter().map(|item| item.target(sandbox)).collect();
        guard(&host, &refs, &targets).await?;
        let unique = LaunchId::generate()?;
        let remote_bundle = sandbox_layout::BUNDLE_DIR.join(&format!("export-{unique}.bundle"))?;
        let result = self
            .ship(&host, sandbox, &unique, &remote_bundle, &items, &cancel)
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
        items: &[Wanted],
        cancel: &CancellationToken,
    ) -> Result<ExportReport, ExportError> {
        let sources: Vec<String> = items.iter().map(Wanted::source).collect();
        self.remote(sandbox, cancel)
            .bundle(remote_bundle, &sources)
            .await
            .map_err(|error| ExportError::remote(sandbox, error))?;
        let staging = self.stage(sandbox, unique).await?;
        let result = self
            .import_exported(host, sandbox, remote_bundle, &staging, items, cancel)
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
        self.deps.host.fs.create_private_dir_all(&dir).await?;
        let bundle = HostPath::resolve(Path::new("export.bundle"), &dir)?;
        Ok(Staging { dir, bundle })
    }

    async fn import_exported(
        &self,
        host: &Host<'_>,
        sandbox: &SandboxName,
        remote_bundle: &SandboxPath,
        staging: &Staging,
        items: &[Wanted],
        cancel: &CancellationToken,
    ) -> Result<ExportReport, ExportError> {
        let digest = self
            .deps
            .gateway
            .download_file(sandbox, remote_bundle, &staging.bundle, cancel)
            .await?;
        let limit = self.deps.limits.bundle_bytes;
        if let Some(size) = self.deps.host.fs.file_len(&staging.bundle).await?
            && size > limit
        {
            return Err(ExportError::BundleTooLarge { size, limit });
        }
        let format = host.object_format().await?;
        let header =
            bundle::header(self.deps.host.fs.as_ref(), &staging.bundle, format, cancel).await?;
        require_heads(&header.heads, items)?;
        bundle::walk(
            self.deps.host.fs.as_ref(),
            &staging.bundle,
            header.pack_offset,
            format,
            Limits {
                entry: self.deps.limits.export_entry_bytes,
                objects: self.deps.limits.export_objects,
                total: self.deps.limits.export_total_bytes,
            },
            cancel,
        )
        .await?;
        host.import(&staging.bundle, header.pack_offset).await?;
        let mut updates = Vec::new();
        for item in items {
            let tip = header
                .heads
                .get(&item.source())
                .ok_or_else(|| ExportError::BundleHeads {
                    extra: Vec::new(),
                    missing: vec![item.source()],
                })?;
            updates.push((item.target(sandbox), tip.clone()));
        }
        host.publish(&updates).await?;
        let landed = host.refs(sandbox).await?;
        report(sandbox, digest, items, &header.heads, &landed)
    }
}

fn require_heads(heads: &BTreeMap<String, CommitId>, items: &[Wanted]) -> Result<(), ExportError> {
    let wanted: BTreeSet<String> = items.iter().map(Wanted::source).collect();
    let extra: Vec<TerminalText> = heads
        .keys()
        .filter(|name| !wanted.contains(*name))
        .map(|name| TerminalText::sanitize(name.as_bytes()))
        .collect();
    let missing: Vec<String> = wanted
        .iter()
        .filter(|name| !heads.contains_key(*name))
        .cloned()
        .collect();
    if extra.is_empty() && missing.is_empty() {
        Ok(())
    } else {
        Err(ExportError::BundleHeads { extra, missing })
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
    items: &[Wanted],
    heads: &BTreeMap<String, CommitId>,
    landed: &HostRefs,
) -> Result<ExportReport, ExportError> {
    let mut report = ExportReport {
        sandbox: sandbox.clone(),
        bundle: Some(digest),
        branches: Vec::new(),
        detached: None,
        worktrees: Vec::new(),
    };
    for item in items {
        let host_ref = item.target(sandbox);
        let expected = heads
            .get(&item.source())
            .ok_or_else(|| ExportError::BundleHeads {
                extra: Vec::new(),
                missing: vec![item.source()],
            })?;
        let found = landed.exported.get(&item.name()).cloned();
        if found.as_ref() != Some(expected) {
            return Err(ExportError::TipMismatch {
                host_ref,
                expected: expected.clone(),
                found,
            });
        }
        let tip = expected.clone();
        match item {
            Wanted::Branch(branch) => report.branches.push(ExportedBranch {
                branch: branch.clone(),
                tip,
                host_ref,
            }),
            Wanted::Head => report.detached = Some(ExportedDetached { tip, host_ref }),
            Wanted::Worktree(name) => report.worktrees.push(ExportedWorktree {
                worktree: name.clone(),
                tip,
                host_ref,
            }),
        }
    }
    Ok(report)
}

#[derive(Debug, Error)]
pub enum ChangesError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("sandbox {0} was not found")]
    NotFound(SandboxName),
    #[error("sandbox {sandbox} is not running ({phase})")]
    NotRunning { sandbox: SandboxName, phase: Phase },
    #[error("sandbox {sandbox} has no git repository")]
    NoRepository { sandbox: SandboxName },
    #[error("git in the sandbox exited with code {exit_code}: {output}")]
    Git {
        exit_code: i32,
        output: TerminalText,
    },
    #[error("sandbox {sandbox}: {what}")]
    Output {
        sandbox: SandboxName,
        what: &'static str,
    },
}

impl Session {
    pub async fn changes(
        &self,
        sandbox: &SandboxName,
        cancel: &CancellationToken,
    ) -> Result<ChangeSet, ChangesError> {
        let view = self
            .find_managed(sandbox)
            .await?
            .ok_or_else(|| ChangesError::NotFound(sandbox.clone()))?;
        if view.phase != Phase::Ready {
            return Err(ChangesError::NotRunning {
                sandbox: sandbox.clone(),
                phase: view.phase,
            });
        }
        let remote = self.remote(sandbox, cancel);
        let repository = remote
            .repository()
            .await
            .map_err(|error| changes_error(sandbox, error))?;
        let files = remote
            .changes(&repository)
            .await
            .map_err(|error| changes_error(sandbox, error))?;
        Ok(ChangeSet { files })
    }
}

fn changes_error(sandbox: &SandboxName, error: RemoteError) -> ChangesError {
    match error {
        RemoteError::Gateway(error) => error.into(),
        RemoteError::NoRepository => ChangesError::NoRepository {
            sandbox: sandbox.clone(),
        },
        RemoteError::Git { exit_code, output } => ChangesError::Git { exit_code, output },
        RemoteError::Malformed(what) => ChangesError::Output {
            sandbox: sandbox.clone(),
            what,
        },
    }
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
        let remote = self.remote(sandbox, &cancel);
        let repository = match remote.repository().await {
            Ok(repository) => repository,
            Err(RemoteError::NoRepository) => return Err(UnknownReason::NoRepository),
            Err(_) => return Err(UnknownReason::InspectionFailed),
        };
        let uncommitted = remote
            .uncommitted(&repository)
            .await
            .map_err(|_| UnknownReason::InspectionFailed)?;
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
        Ok(classify(&repository, uncommitted, &refs))
    }
}
