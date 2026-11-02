// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Agent, CommitId, Config, FileKind, FileSystem, FsError, HookKind, HostPath, ManifestError,
    PathError, ProcessError, ProcessRunner, ProcessSpec, ProcessStatus, ProcessStdin, Program,
    RepoPath, RoleManifest, RoleName, RoleRev, RoleSource, RoleUrl, Sha256, TerminalText,
    Workspace,
};
use jackin_image::{DockerfileError, HookFiles, RoleRevision, SourceFile, validate_dockerfile};
use jackin_openshell::{ClosureViolation, PolicyDocument, PolicyError, ProviderProfile};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

const MANIFEST_FILE: &str = "jackin.role.toml";
const GIT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug)]
pub struct RoleCatalog {
    fs: Arc<dyn FileSystem>,
    runner: Arc<dyn ProcessRunner>,
    cache_dir: HostPath,
}

#[derive(Clone, Debug)]
pub struct LoadedRole {
    name: RoleName,
    root: HostPath,
    manifest: RoleManifest,
    policy: PolicyDocument,
    dockerfile: SourceFile,
    hooks: HookFiles,
    revision: RoleRevision,
    files: Vec<RepoPath>,
}

#[derive(Debug, Error)]
pub enum RoleError {
    #[error("no role is selected and none is configured")]
    NoRole,
    #[error("unknown role {0:?}")]
    UnknownRole(RoleName),
    #[error("invalid role manifest at {path:?}: {source}")]
    Manifest {
        path: HostPath,
        source: ManifestError,
    },
    #[error("missing file {path:?}")]
    FileMissing { path: RepoPath },
    #[error("not a regular file: {path:?}")]
    FileNotRegular { path: RepoPath },
    #[error("path escapes the repository: {path:?}")]
    EscapesRepository { path: RepoPath },
    #[error(transparent)]
    Dockerfile(#[from] DockerfileError),
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error("the role policy overlaps provider hosts: {0:?}")]
    ProviderHostClosure(Vec<ClosureViolation>),
    #[error("git failed with {status:?}: {output:?}")]
    Git {
        status: ProcessStatus,
        output: TerminalText,
    },
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Process(#[from] ProcessError),
}

struct Inspected {
    manifest: RoleManifest,
    policy: PolicyDocument,
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

    pub fn policy(&self) -> &PolicyDocument {
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

    pub fn files(&self) -> &[RepoPath] {
        &self.files
    }
}

fn join(base: &HostPath, segment: &str) -> Result<HostPath, PathError> {
    HostPath::resolve(Path::new(segment), base)
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
        cache_dir: HostPath,
    ) -> RoleCatalog {
        RoleCatalog {
            fs,
            runner,
            cache_dir,
        }
    }

    pub async fn load(
        &self,
        config: &Config,
        role: Option<&RoleName>,
        workspace: &Workspace,
    ) -> Result<LoadedRole, RoleError> {
        let name = if let Some(name) = role.or(workspace.default_role.as_ref()) {
            name.clone()
        } else {
            let mut roles = config.roles();
            match (roles.next(), roles.next()) {
                (Some((only, _)), None) => only.clone(),
                _ => return Err(RoleError::NoRole),
            }
        };
        let entry = config
            .role(&name)
            .ok_or_else(|| RoleError::UnknownRole(name.clone()))?;
        let root = match &entry.source {
            RoleSource::Path(path) => path.clone(),
            RoleSource::Git { url, rev } => self.checkout(&name, url, rev.as_ref()).await?,
        };
        self.load_path(&root).await
    }

    pub async fn load_path(&self, root: &HostPath) -> Result<LoadedRole, RoleError> {
        let inspected = match self.inspect(root).await {
            Ok(inspected) => inspected,
            Err(mut findings) => return Err(findings.remove(0)),
        };
        let (revision, files) = self.revision(root).await?;
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

    pub async fn findings(&self, root: &HostPath) -> Vec<RoleError> {
        self.inspect(root).await.err().unwrap_or_default()
    }

    async fn inspect(&self, root: &HostPath) -> Result<Inspected, Vec<RoleError>> {
        let manifest = self.read_manifest(root).await?;
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
            let parsed = PolicyDocument::parse_authored(&String::from_utf8_lossy(&bytes));
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

    async fn read_manifest(&self, root: &HostPath) -> Result<RoleManifest, Vec<RoleError>> {
        let name = RepoPath::parse(MANIFEST_FILE).map_err(|error| vec![RoleError::from(error)])?;
        let path = self
            .check_file(root, &name)
            .await
            .map_err(|error| vec![error])?;
        let bytes = self
            .fs
            .read(&path)
            .await
            .map_err(|error| vec![RoleError::from(error)])?
            .ok_or_else(|| vec![RoleError::FileMissing { path: name.clone() }])?;
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

    async fn read_checked(
        &self,
        root: &HostPath,
        path: &RepoPath,
        findings: &mut Vec<RoleError>,
    ) -> Option<Vec<u8>> {
        let host = match self.check_file(root, path).await {
            Ok(host) => host,
            Err(error) => {
                findings.push(error);
                return None;
            }
        };
        match self.fs.read(&host).await {
            Ok(Some(bytes)) => Some(bytes),
            Ok(None) => {
                findings.push(RoleError::FileMissing { path: path.clone() });
                None
            }
            Err(error) => {
                findings.push(RoleError::Fs(error));
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

    fn check_closure(policy: &PolicyDocument, findings: &mut Vec<RoleError>) {
        let mut violations = Vec::new();
        for agent in Agent::ALL {
            for asset in agent.spec().profiles() {
                match ProviderProfile::parse(asset) {
                    Ok(profile) => violations.extend(policy.closure_violations(&profile, &[])),
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

    async fn check_file(&self, root: &HostPath, path: &RepoPath) -> Result<HostPath, RoleError> {
        let segments: Vec<&str> = path.as_str().split('/').collect();
        let mut current = root.clone();
        for (index, segment) in segments.iter().enumerate() {
            current = join(&current, segment)?;
            let kind = self.fs.stat(&current).await?;
            let last = index + 1 == segments.len();
            match (kind, last) {
                (None, _) | (Some(FileKind::File), false) => {
                    return Err(RoleError::FileMissing { path: path.clone() });
                }
                (Some(FileKind::Symlink), _) => {
                    return Err(RoleError::EscapesRepository { path: path.clone() });
                }
                (Some(FileKind::Dir), true) => {
                    return Err(RoleError::FileNotRegular { path: path.clone() });
                }
                (Some(FileKind::Dir), false) | (Some(FileKind::File), true) => {}
            }
        }
        Ok(current)
    }

    async fn git(&self, root: &HostPath, args: &[&str]) -> Result<Vec<u8>, RoleError> {
        let spec = ProcessSpec {
            program: Program::Git,
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            cwd: Some(root.clone()),
            env: Vec::new(),
            env_remove_prefixes: vec!["GIT_".to_owned()],
            stdin: ProcessStdin::Null,
            timeout: GIT_TIMEOUT,
        };
        let output = self
            .runner
            .run(&spec, None, &CancellationToken::new())
            .await?;
        if output.status == ProcessStatus::Exited(0) {
            Ok(output.stdout)
        } else {
            Err(git_failure(output.status, &output.stderr))
        }
    }

    async fn revision(&self, root: &HostPath) -> Result<(RoleRevision, Vec<RepoPath>), RoleError> {
        let head = self.git(root, &["rev-parse", "HEAD"]).await?;
        let commit = CommitId::parse(String::from_utf8_lossy(&head).trim())
            .map_err(|_| git_failure(ProcessStatus::Exited(0), &head))?;
        let listing = self
            .git(root, &["ls-files", "-z", "-co", "--exclude-standard"])
            .await?;
        let mut names: Vec<&[u8]> = listing
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .collect();
        names.sort_unstable();
        names.dedup();
        let mut digest_input = Vec::new();
        let mut files = Vec::new();
        for name in names {
            let text = String::from_utf8_lossy(name).into_owned();
            let path = RepoPath::parse(&text)?;
            let host = join_repo(root, &path)?;
            let (content, listed) = match self.fs.stat(&host).await? {
                None => continue,
                Some(FileKind::File) => (self.fs.read(&host).await?.unwrap_or_default(), true),
                Some(FileKind::Symlink) => (Vec::new(), true),
                Some(FileKind::Dir) => (Vec::new(), false),
            };
            digest_input.extend((name.len() as u64).to_be_bytes());
            digest_input.extend(name);
            digest_input.extend((content.len() as u64).to_be_bytes());
            digest_input.extend(content);
            if listed {
                files.push(path);
            }
        }
        let status = self.git(root, &["status", "--porcelain"]).await?;
        let revision = if status.is_empty() {
            RoleRevision::Clean { commit }
        } else {
            RoleRevision::Dirty {
                commit,
                digest: Sha256::of(&digest_input),
            }
        };
        Ok((revision, files))
    }

    async fn checkout(
        &self,
        name: &RoleName,
        url: &RoleUrl,
        rev: Option<&RoleRev>,
    ) -> Result<HostPath, RoleError> {
        let parent = join(&self.cache_dir, "roles")?;
        self.fs.create_dir_all(&parent).await?;
        let dir = join(&parent, name.as_str())?;
        if self.fs.stat(&join(&dir, ".git")?).await?.is_none() {
            self.git(
                &parent,
                &["clone", "--quiet", "--", url.as_str(), name.as_str()],
            )
            .await?;
        } else {
            self.git(&dir, &["fetch", "--quiet", "origin"]).await?;
        }
        let target = rev.map_or("origin/HEAD", RoleRev::as_str);
        self.git(&dir, &["checkout", "--quiet", "--detach", target])
            .await?;
        Ok(dir)
    }
}

fn join_repo(root: &HostPath, path: &RepoPath) -> Result<HostPath, PathError> {
    HostPath::resolve(Path::new(path.as_str()), root)
}
