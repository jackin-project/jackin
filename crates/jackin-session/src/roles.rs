// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Agent, Clock, CommitId, Config, FileKind, FileLock, FileSystem, FsError, HookKind, HostPath,
    LinkProblem, ManifestError, PathError, PollError, PollPlan, ProcessError, ProcessOutput,
    ProcessRunner, ProcessStatus, RegularFileProblem, RepoPath, RoleManifest, RoleName, RoleRev,
    RoleSource, RoleUrl, Sha256, TerminalText, Workspace, hold_lock, lock_path, plain_list,
};
use jackin_image::{
    ContextEntry, DockerfileError, HookFiles, RoleRevision, SourceFile, validate_dockerfile,
};
use jackin_openshell::{
    AuthoredPolicy, ClosureViolation, EngineHostViolation, PolicyError, ProviderProfile,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::project::git::{HostGit, Pace, words};

pub const MANIFEST_FILE: &str = "jackin.role.toml";
const KIND_FILE: u8 = b'f';
const KIND_LINK: u8 = b'l';
const KIND_OTHER: u8 = b'o';
const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const LOCK_PLAN: PollPlan = PollPlan {
    interval: Duration::from_millis(50),
    timeout: Duration::from_mins(10),
};

#[derive(Clone, Debug)]
pub struct RoleCatalog {
    fs: Arc<dyn FileSystem>,
    runner: Arc<dyn ProcessRunner>,
    clock: Arc<dyn Clock>,
    cache_dir: HostPath,
}

#[derive(Clone, Debug)]
pub struct LoadedRole {
    name: RoleName,
    root: HostPath,
    manifest: RoleManifest,
    policy: AuthoredPolicy,
    dockerfile: SourceFile,
    hooks: HookFiles,
    revision: RoleRevision,
    files: Vec<ContextEntry>,
}

#[derive(Debug, Error)]
pub enum RoleError {
    #[error("no role is selected: set a default role or name one")]
    NoRole,
    #[error("the workspace does not allow role {0}")]
    NotAllowed(RoleName),
    #[error("unknown role {0}")]
    UnknownRole(RoleName),
    #[error("invalid role manifest at {path}")]
    Manifest {
        path: HostPath,
        source: ManifestError,
    },
    #[error("{path} does not exist")]
    RootMissing { path: HostPath },
    #[error("{path} is not a directory")]
    RootNotDirectory { path: HostPath },
    #[error("{root} has no {}", MANIFEST_FILE)]
    ManifestMissing { root: HostPath },
    #[error("missing file {path}")]
    FileMissing { path: RepoPath },
    #[error("not a regular file: {path}")]
    FileNotRegular { path: RepoPath },
    #[error("path escapes the repository: {path}")]
    EscapesRepository { path: RepoPath },
    #[error("{path} is larger than {limit} bytes")]
    FileTooLarge { path: RepoPath, limit: usize },
    #[error("the role files are larger than {limit} bytes together")]
    FilesTooLarge { limit: usize },
    #[error("the manifest names role {manifest}, but the configuration calls it {configured}")]
    NameMismatch {
        configured: RoleName,
        manifest: RoleName,
    },
    #[error("a role file name is not valid UTF-8: {0}")]
    NonUtf8Path(TerminalText),
    #[error("cancelled while loading the role")]
    Cancelled,
    #[error("timed out waiting for the lock {path}")]
    LockTimeout { path: HostPath },
    #[error(transparent)]
    Dockerfile(#[from] DockerfileError),
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error("the role policy overlaps provider hosts: {}", plain_list(.0))]
    ProviderHostClosure(Vec<ClosureViolation>),
    #[error("the role policy names a host-gateway alias in the rules {}", plain_list(.0))]
    EngineHost(Vec<EngineHostViolation>),
    #[error("git failed with {status}: {output}")]
    Git {
        status: ProcessStatus,
        output: TerminalText,
    },
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Process(ProcessError),
}

struct Inspected {
    manifest: RoleManifest,
    policy: AuthoredPolicy,
    dockerfile: SourceFile,
    hooks: HookFiles,
}

impl LoadedRole {
    pub fn name(&self) -> &RoleName {
        &self.name
    }

    pub fn root(&self) -> &HostPath {
        &self.root
    }

    pub fn manifest(&self) -> &RoleManifest {
        &self.manifest
    }

    pub fn policy(&self) -> &AuthoredPolicy {
        &self.policy
    }

    pub fn dockerfile(&self) -> &SourceFile {
        &self.dockerfile
    }

    pub fn hooks(&self) -> &HookFiles {
        &self.hooks
    }

    pub fn revision(&self) -> &RoleRevision {
        &self.revision
    }

    pub fn files(&self) -> &[ContextEntry] {
        &self.files
    }
}

fn join(base: &HostPath, segment: &str) -> Result<HostPath, PathError> {
    HostPath::resolve(Path::new(segment), base)
}

fn process_failure(error: ProcessError) -> RoleError {
    match error {
        ProcessError::Cancelled { .. } => RoleError::Cancelled,
        other => RoleError::Process(other),
    }
}

fn git_failure(status: ProcessStatus, stderr: &[u8]) -> RoleError {
    RoleError::Git {
        status,
        output: TerminalText::sanitize(stderr),
    }
}

impl RoleCatalog {
    pub fn new(
        fs: Arc<dyn FileSystem>,
        runner: Arc<dyn ProcessRunner>,
        clock: Arc<dyn Clock>,
        cache_dir: HostPath,
    ) -> RoleCatalog {
        RoleCatalog {
            fs,
            runner,
            clock,
            cache_dir,
        }
    }

    pub async fn load(
        &self,
        config: &Config,
        role: Option<&RoleName>,
        workspace: &Workspace,
        cancel: &CancellationToken,
    ) -> Result<LoadedRole, RoleError> {
        let name = if let Some(name) = role.or(workspace.default_role.as_ref()) {
            name.clone()
        } else {
            let mut roles = config
                .roles()
                .filter(|(name, _)| workspace.roles.allows(name));
            match (roles.next(), roles.next()) {
                (Some((only, _)), None) => only.clone(),
                _ => return Err(RoleError::NoRole),
            }
        };
        let entry = config
            .role(&name)
            .ok_or_else(|| RoleError::UnknownRole(name.clone()))?;
        if !workspace.roles.allows(&name) {
            return Err(RoleError::NotAllowed(name));
        }
        let loaded = match &entry.source {
            RoleSource::Path(path) => self.load_path(path, cancel).await?,
            RoleSource::Git { url, rev } => self.load_git(&name, url, rev.as_ref(), cancel).await?,
        };
        if loaded.name() != &name {
            return Err(RoleError::NameMismatch {
                configured: name,
                manifest: loaded.name().clone(),
            });
        }
        Ok(loaded)
    }

    pub async fn load_path(
        &self,
        root: &HostPath,
        cancel: &CancellationToken,
    ) -> Result<LoadedRole, RoleError> {
        let inspected = match self.inspect(root).await {
            Ok(inspected) => inspected,
            Err(mut findings) => return Err(findings.remove(0)),
        };
        let (revision, files) = self.snapshot(root, cancel).await?;
        Ok(LoadedRole {
            name: inspected.manifest.role.name.clone(),
            root: root.clone(),
            manifest: inspected.manifest,
            policy: inspected.policy,
            dockerfile: inspected.dockerfile,
            hooks: inspected.hooks,
            revision,
            files,
        })
    }

    async fn load_git(
        &self,
        name: &RoleName,
        url: &RoleUrl,
        rev: Option<&RoleRev>,
        cancel: &CancellationToken,
    ) -> Result<LoadedRole, RoleError> {
        let parent = self.cache_parent().await?;
        let dir = join(&parent, name.as_str())?;
        let _held = self.hold_dir(&dir, cancel).await?;
        self.checkout(&parent, &dir, name.as_str(), url, rev, cancel)
            .await?;
        self.load_path(&dir, cancel).await
    }

    pub async fn fetch_git(
        &self,
        url: &RoleUrl,
        rev: Option<&RoleRev>,
        cancel: &CancellationToken,
    ) -> Result<LoadedRole, RoleError> {
        let parent = self.cache_parent().await?;
        let digest = Sha256::of(url.as_str().as_bytes());
        let staging = format!(".incoming-{}", &digest.as_str()[..16]);
        let incoming = join(&parent, &staging)?;
        let _staged = self.hold_dir(&incoming, cancel).await?;
        let found = async {
            self.checkout(&parent, &incoming, &staging, url, rev, cancel)
                .await?;
            self.load_path(&incoming, cancel).await
        }
        .await;
        let loaded = match found {
            Ok(loaded) => loaded,
            Err(error) => {
                drop(self.fs.remove_dir_all(&incoming).await);
                return Err(error);
            }
        };
        let dir = join(&parent, loaded.name().as_str())?;
        let _held = self.hold_dir(&dir, cancel).await?;
        self.fs.remove_dir_all(&dir).await?;
        self.fs.rename(&incoming, &dir).await?;
        self.load_path(&dir, cancel).await
    }

    async fn cache_parent(&self) -> Result<HostPath, RoleError> {
        let parent = join(&self.cache_dir, "roles")?;
        self.fs.create_private_dir_all(&parent).await?;
        Ok(parent)
    }

    async fn hold_dir(
        &self,
        dir: &HostPath,
        cancel: &CancellationToken,
    ) -> Result<FileLock, RoleError> {
        let lock = lock_path(dir)?;
        hold_lock(&*self.fs, &*self.clock, &lock, LOCK_PLAN, cancel)
            .await
            .map_err(|error| match error {
                PollError::Cancelled => RoleError::Cancelled,
                PollError::Timeout { .. } => RoleError::LockTimeout { path: lock.clone() },
                PollError::Probe(error) => RoleError::Fs(error),
            })
    }

    pub async fn findings(&self, root: &HostPath) -> Vec<RoleError> {
        self.inspect(root).await.err().unwrap_or_default()
    }

    pub async fn findings_of(&self, root: &HostPath, manifest: RoleManifest) -> Vec<RoleError> {
        self.inspect_manifest(root, manifest)
            .await
            .err()
            .unwrap_or_default()
    }

    async fn inspect(&self, root: &HostPath) -> Result<Inspected, Vec<RoleError>> {
        let manifest = self.read_manifest(root).await?;
        self.inspect_manifest(root, manifest).await
    }

    async fn inspect_manifest(
        &self,
        root: &HostPath,
        manifest: RoleManifest,
    ) -> Result<Inspected, Vec<RoleError>> {
        let mut findings = Vec::new();
        let dockerfile = self
            .read_checked(root, &manifest.role.dockerfile, &mut findings)
            .await;
        if let Some(bytes) = &dockerfile {
            match std::str::from_utf8(bytes) {
                Ok(text) => {
                    if let Err(error) = validate_dockerfile(text) {
                        findings.push(RoleError::Dockerfile(error));
                    }
                }
                Err(_) => findings.push(RoleError::Dockerfile(DockerfileError::NotUtf8)),
            }
        }
        let policy_bytes = self
            .read_checked(root, &manifest.role.policy, &mut findings)
            .await;
        let policy = policy_bytes.and_then(|bytes| {
            let parsed = std::str::from_utf8(&bytes)
                .map_err(|_| PolicyError::Syntax {
                    message: "the file is not valid UTF-8".to_owned(),
                })
                .and_then(AuthoredPolicy::parse);
            match parsed {
                Ok(policy) => Some(policy),
                Err(error) => {
                    findings.push(RoleError::Policy(error));
                    None
                }
            }
        });
        let setup = self
            .hook(root, &manifest, HookKind::Setup, &mut findings)
            .await;
        let preflight = self
            .hook(root, &manifest, HookKind::Preflight, &mut findings)
            .await;
        if let Some(policy) = &policy {
            Self::check_closure(policy, &mut findings);
            Self::check_engine_host(policy, &mut findings);
        }
        match (dockerfile, policy) {
            (Some(bytes), Some(policy)) if findings.is_empty() => Ok(Inspected {
                dockerfile: SourceFile {
                    path: manifest.role.dockerfile.clone(),
                    bytes,
                },
                hooks: HookFiles { setup, preflight },
                policy,
                manifest,
            }),
            _ => Err(findings),
        }
    }

    pub async fn read_manifest(&self, root: &HostPath) -> Result<RoleManifest, Vec<RoleError>> {
        let name = RepoPath::parse(MANIFEST_FILE).map_err(|error| vec![RoleError::from(error)])?;
        match self
            .fs
            .stat_following(root)
            .await
            .map_err(|error| vec![RoleError::from(error)])?
        {
            None => return Err(vec![RoleError::RootMissing { path: root.clone() }]),
            Some(FileKind::Dir) => {}
            Some(FileKind::File | FileKind::Socket | FileKind::Symlink | FileKind::Other) => {
                return Err(vec![RoleError::RootNotDirectory { path: root.clone() }]);
            }
        }
        let manifest_missing = || vec![RoleError::ManifestMissing { root: root.clone() }];
        let path = join_repo(root, &name).map_err(|error| vec![RoleError::from(error)])?;
        let bytes = match self.read_input(root, &name).await {
            Ok(bytes) => bytes,
            Err(RoleError::FileMissing { .. }) => return Err(manifest_missing()),
            Err(error) => return Err(vec![error]),
        };
        let text = String::from_utf8(bytes).map_err(|_| {
            vec![RoleError::Manifest {
                path: path.clone(),
                source: ManifestError::Syntax {
                    line: 1,
                    column: 1,
                    message: "the file is not valid UTF-8".to_owned(),
                },
            }]
        })?;
        let found = RoleManifest::findings(&text);
        if !found.is_empty() {
            return Err(found
                .into_iter()
                .map(|source| RoleError::Manifest {
                    path: path.clone(),
                    source,
                })
                .collect());
        }
        RoleManifest::parse(&text).map_err(|source| vec![RoleError::Manifest { path, source }])
    }

    async fn read_input(&self, root: &HostPath, path: &RepoPath) -> Result<Vec<u8>, RoleError> {
        match self.fs.read_regular(root, path, MAX_FILE_BYTES).await? {
            Ok(file) if file.bytes.len() > MAX_FILE_BYTES => Err(RoleError::FileTooLarge {
                path: path.clone(),
                limit: MAX_FILE_BYTES,
            }),
            Ok(file) => Ok(file.bytes),
            Err(RegularFileProblem::Missing) => Err(RoleError::FileMissing { path: path.clone() }),
            Err(RegularFileProblem::Symlink) => {
                Err(RoleError::EscapesRepository { path: path.clone() })
            }
            Err(RegularFileProblem::NotARegularFile) => {
                Err(RoleError::FileNotRegular { path: path.clone() })
            }
        }
    }

    async fn read_checked(
        &self,
        root: &HostPath,
        path: &RepoPath,
        findings: &mut Vec<RoleError>,
    ) -> Option<Vec<u8>> {
        match self.read_input(root, path).await {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                findings.push(error);
                None
            }
        }
    }

    async fn hook(
        &self,
        root: &HostPath,
        manifest: &RoleManifest,
        kind: HookKind,
        findings: &mut Vec<RoleError>,
    ) -> Option<SourceFile> {
        let path = manifest.hook(kind)?;
        let bytes = self.read_checked(root, path, findings).await?;
        Some(SourceFile {
            path: path.clone(),
            bytes,
        })
    }

    fn check_engine_host(policy: &AuthoredPolicy, findings: &mut Vec<RoleError>) {
        let mut violations = policy.document().engine_host_violations();
        violations.sort();
        violations.dedup();
        if !violations.is_empty() {
            findings.push(RoleError::EngineHost(violations));
        }
    }

    fn check_closure(policy: &AuthoredPolicy, findings: &mut Vec<RoleError>) {
        let mut violations = Vec::new();
        for agent in Agent::ALL {
            for asset in agent.spec().profiles() {
                match ProviderProfile::parse(asset) {
                    Ok(profile) => {
                        violations.extend(policy.document().closure_violations(&profile, &[]));
                    }
                    Err(error) => findings.push(RoleError::Policy(error)),
                }
            }
        }
        violations.sort();
        violations.dedup();
        if !violations.is_empty() {
            findings.push(RoleError::ProviderHostClosure(violations));
        }
    }

    async fn capture(
        &self,
        root: &HostPath,
        cancel: &CancellationToken,
        args: &[&str],
    ) -> Result<ProcessOutput, RoleError> {
        let git = HostGit::role_checkout(self.runner.as_ref(), root, cancel);
        git.capture(Pace::Quick, words(args), Vec::new())
            .await
            .map_err(process_failure)
    }

    async fn git(
        &self,
        root: &HostPath,
        cancel: &CancellationToken,
        args: &[&str],
    ) -> Result<Vec<u8>, RoleError> {
        let output = self.capture(root, cancel, args).await?;
        if output.status == ProcessStatus::Exited(0) {
            Ok(output.stdout)
        } else {
            Err(git_failure(output.status, &output.stderr))
        }
    }

    async fn resolves(
        &self,
        root: &HostPath,
        cancel: &CancellationToken,
        reference: &str,
    ) -> Result<bool, RoleError> {
        let output = self
            .capture(
                root,
                cancel,
                &["rev-parse", "--verify", "--quiet", reference],
            )
            .await?;
        Ok(output.status == ProcessStatus::Exited(0))
    }

    async fn snapshot(
        &self,
        root: &HostPath,
        cancel: &CancellationToken,
    ) -> Result<(RoleRevision, Vec<ContextEntry>), RoleError> {
        let head = self.git(root, cancel, &["rev-parse", "HEAD"]).await?;
        let commit = CommitId::parse(String::from_utf8_lossy(&head).trim())
            .map_err(|_| git_failure(ProcessStatus::Exited(0), &head))?;
        let listing = self
            .git(
                root,
                cancel,
                &["ls-files", "-z", "-co", "--exclude-standard"],
            )
            .await?;
        let mut names: Vec<&[u8]> = listing
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .collect();
        names.sort_unstable();
        names.dedup();
        let mut digest_input = Vec::new();
        let mut entries = Vec::new();
        let mut total = 0usize;
        for name in names {
            let text = std::str::from_utf8(name)
                .map_err(|_| RoleError::NonUtf8Path(TerminalText::sanitize(name)))?;
            let path = RepoPath::parse(text)?;
            let (kind, exec, content) = match self
                .fs
                .read_regular(root, &path, MAX_FILE_BYTES)
                .await?
            {
                Ok(file) => {
                    if file.bytes.len() > MAX_FILE_BYTES {
                        return Err(RoleError::FileTooLarge {
                            path,
                            limit: MAX_FILE_BYTES,
                        });
                    }
                    total += file.bytes.len();
                    if total > MAX_TOTAL_BYTES {
                        return Err(RoleError::FilesTooLarge {
                            limit: MAX_TOTAL_BYTES,
                        });
                    }
                    let executable = file.mode.0 & 0o111 != 0;
                    let content = Sha256::of(&file.bytes);
                    entries.push(ContextEntry::File {
                        path,
                        bytes: file.bytes,
                        executable,
                    });
                    (KIND_FILE, u8::from(executable), content)
                }
                Err(RegularFileProblem::Missing) => continue,
                Err(RegularFileProblem::Symlink) => match self.fs.read_link(root, &path).await? {
                    Ok(target) => {
                        let content = Sha256::of(target.as_bytes());
                        entries.push(ContextEntry::Link { path, target });
                        (KIND_LINK, 0, content)
                    }
                    Err(LinkProblem::Missing | LinkProblem::NotALink) => continue,
                    Err(LinkProblem::Symlink | LinkProblem::NonUtf8) => {
                        return Err(RoleError::EscapesRepository { path });
                    }
                },
                Err(RegularFileProblem::NotARegularFile) => {
                    match self.fs.stat(&join_repo(root, &path)?).await? {
                        None => continue,
                        Some(FileKind::Dir | FileKind::Socket) => (KIND_OTHER, 0, Sha256::of(b"")),
                        Some(FileKind::File | FileKind::Symlink | FileKind::Other) => {
                            return Err(RoleError::FileNotRegular { path });
                        }
                    }
                }
            };
            digest_input.extend([kind, exec]);
            digest_input.extend((name.len() as u64).to_be_bytes());
            digest_input.extend(name);
            digest_input.extend(content.as_str().as_bytes());
        }
        let revision = RoleRevision {
            commit,
            digest: Sha256::of(&digest_input),
        };
        Ok((revision, entries))
    }

    async fn origin_url(
        &self,
        dir: &HostPath,
        cancel: &CancellationToken,
    ) -> Result<Option<String>, RoleError> {
        if self.fs.stat(&join(dir, ".git")?).await?.is_none() {
            return Ok(None);
        }
        let output = self
            .capture(dir, cancel, &["config", "--get", "remote.origin.url"])
            .await?;
        Ok((output.status == ProcessStatus::Exited(0))
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned()))
    }

    async fn checkout(
        &self,
        parent: &HostPath,
        dir: &HostPath,
        leaf: &str,
        url: &RoleUrl,
        rev: Option<&RoleRev>,
        cancel: &CancellationToken,
    ) -> Result<(), RoleError> {
        if self.origin_url(dir, cancel).await?.as_deref() == Some(url.as_str()) {
            self.git(
                dir,
                cancel,
                &[
                    "fetch",
                    "--quiet",
                    "--prune",
                    "--prune-tags",
                    "--tags",
                    "--force",
                    "origin",
                ],
            )
            .await?;
            self.git(dir, cancel, &["remote", "set-head", "origin", "--auto"])
                .await?;
        } else {
            self.fs.remove_dir_all(dir).await?;
            self.git(
                parent,
                cancel,
                &["clone", "--quiet", "--", url.as_str(), leaf],
            )
            .await?;
        }
        let target = match rev {
            None => "origin/HEAD".to_owned(),
            Some(rev) => {
                let branch = format!("origin/{}^{{commit}}", rev.as_str());
                if self.resolves(dir, cancel, &branch).await? {
                    format!("origin/{}", rev.as_str())
                } else {
                    format!("{}^{{commit}}", rev.as_str())
                }
            }
        };
        self.git(dir, cancel, &["checkout", "--quiet", "--detach", &target])
            .await?;
        Ok(())
    }
}

fn join_repo(root: &HostPath, path: &RepoPath) -> Result<HostPath, PathError> {
    HostPath::resolve(Path::new(path.as_str()), root)
}
