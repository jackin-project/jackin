// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_image::ContextEntry;
use std::error::Error;
use std::fs;
use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, DockerSubcommand, FileSystem, HostPath,
    OutputStream, Platform, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, ProcessStatus,
    Program, RepoPath, RoleName, SecretOutput, Sha256, StdFileSystem,
};
use jackin_image::{
    BuildxEngine, HookFiles, ImageEngine, ImageEvent, ImagePlan, ImageRequest, Rebuild,
    RoleRevision, SourceFile,
};
use jackin_testkit::{DockerBoundary, DockerBuildReply, DockerIidFile, FakeClock};
use tempfile::TempDir;
use tokio::sync::mpsc::Receiver;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const CONSTRUCT_DOCKERFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/images/construct/Dockerfile"
));
pub(crate) const WRITE_FILE_C: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/images/construct/write-file.c"
));
pub(crate) const DERIVED_DOCKERFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/images/derived/Dockerfile"
));
pub(crate) const JACKIN_START: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/images/jackin-start"));
pub(crate) const SCAN: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/images/derived/scan"));
pub(crate) const ROLE_DOCKERFILE: &[u8] = b"FROM construct AS role\n";

#[derive(Debug)]
pub(crate) struct DockerOnlyProcessRunner {
    inner: Arc<dyn ProcessRunner>,
}

impl DockerOnlyProcessRunner {
    pub(crate) fn new(inner: Arc<dyn ProcessRunner>) -> DockerOnlyProcessRunner {
        DockerOnlyProcessRunner { inner }
    }
}

#[async_trait]
impl ProcessRunner for DockerOnlyProcessRunner {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&jackin_core::LineSink>,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        if !matches!(
            spec.program,
            Program::Docker(
                DockerSubcommand::BuildxBuild
                    | DockerSubcommand::BuildxInspect
                    | DockerSubcommand::ImageInspect
                    | DockerSubcommand::ImageTag
            )
        ) {
            return Err(ProcessError::Spawn {
                program: spec.program,
                message: "the scripted external boundary accepts Docker image commands only"
                    .to_owned(),
            });
        }
        self.inner.run(spec, lines, cancel).await
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        _cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        Err(ProcessError::Spawn {
            program: spec.program,
            message: "the scripted external boundary does not accept secret process output"
                .to_owned(),
        })
    }
}

pub(crate) fn tag_id(tag: &str) -> String {
    format!("sha256:{}", Sha256::of(tag.as_bytes()).as_str())
}

pub(crate) fn source(path: &str, bytes: &[u8]) -> Result<SourceFile, Box<dyn Error>> {
    Ok(SourceFile {
        path: RepoPath::parse(path)?,
        bytes: bytes.to_vec(),
    })
}

pub(crate) struct Fixture {
    pub(crate) dir: TempDir,
    pub(crate) role: HostPath,
    pub(crate) work: HostPath,
}

impl Fixture {
    pub(crate) fn new() -> Result<Fixture, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let base = dir.path().canonicalize()?;
        let role = base.join("role");
        fs::create_dir_all(role.join("src"))?;
        fs::write(role.join("Dockerfile"), ROLE_DOCKERFILE)?;
        fs::write(role.join("src/a.rs"), b"fn a() {}\n")?;
        Ok(Fixture {
            role: HostPath::parse(&role)?,
            work: HostPath::parse(&base.join("work"))?,
            dir,
        })
    }

    fn entry(&self, path: &str) -> Result<ContextEntry, Box<dyn Error>> {
        Ok(ContextEntry::File {
            path: RepoPath::parse(path)?,
            bytes: fs::read(self.role.as_path().join(path))?,
            executable: false,
        })
    }

    pub(crate) fn request(&self) -> Result<ImageRequest, Box<dyn Error>> {
        Ok(ImageRequest {
            role: RoleName::parse("sentinel")?,
            context_files: vec![self.entry("Dockerfile")?, self.entry("src/a.rs")?],
            dockerfile: source("Dockerfile", ROLE_DOCKERFILE)?,
            hooks: HookFiles {
                setup: Some(source("hooks/setup.sh", b"#!/bin/sh\necho setup\n")?),
                preflight: None,
            },
            revision: RoleRevision {
                commit: CommitId::parse(&"1".repeat(40))?,
                digest: Sha256::of(b"clean"),
            },
            agent: Agent::Codex,
            agents: vec![Agent::Codex],
            platform: Platform::LinuxArm64,
            builder: BuilderName::parse("orbstack")?,
            docker: DockerLayer::Absent,
            read_write: Vec::new(),
            rebuild: Rebuild::No,
        })
    }

    pub(crate) fn work_entries(&self) -> Vec<String> {
        fs::read_dir(self.work.as_path()).map_or_else(
            |_| Vec::new(),
            |entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .filter(|name| !matches!(name.as_str(), "locks" | "attested"))
                    .collect()
            },
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) enum IidFile {
    Id,
    Text(String),
    Absent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Exit(ProcessStatus),
}

#[derive(Clone, Debug)]
pub(crate) struct Script {
    pub(crate) lines: Vec<(OutputStream, String)>,
    pub(crate) outcome: Outcome,
    pub(crate) iid: IidFile,
}

impl Default for Script {
    fn default() -> Script {
        Script {
            lines: Vec::new(),
            outcome: Outcome::Exit(ProcessStatus::Exited(0)),
            iid: IidFile::Id,
        }
    }
}

pub(crate) struct Stack {
    pub(crate) fixture: Fixture,
    pub(crate) model: Arc<DockerBoundary>,
    pub(crate) engine: BuildxEngine,
    pub(crate) clock: Arc<FakeClock>,
}

impl Stack {
    pub(crate) fn new() -> Result<Stack, Box<dyn Error>> {
        Stack::with_fs(Arc::new(StdFileSystem::default()))
    }

    pub(crate) fn with_fs(fs: Arc<dyn FileSystem>) -> Result<Stack, Box<dyn Error>> {
        let fixture = Fixture::new()?;
        let clock = Arc::new(FakeClock::new());
        let model = Arc::new(DockerBoundary::with_fake_clock(Arc::clone(&clock)));
        let shared: Arc<dyn ProcessRunner> = Arc::<DockerBoundary>::clone(&model);
        let engine = BuildxEngine::new(shared, fs, fixture.work.clone());
        Ok(Stack {
            fixture,
            model,
            engine,
            clock,
        })
    }

    pub(crate) fn script(&self, tag: &jackin_core::ImageTag, script: Script) {
        let iid_file = match script.iid {
            IidFile::Id => DockerIidFile::Id,
            IidFile::Text(text) => DockerIidFile::Text(text.into_bytes()),
            IidFile::Absent => DockerIidFile::Missing,
        };
        let Outcome::Exit(status) = script.outcome;
        self.script_reply(
            tag,
            DockerBuildReply {
                status,
                stdout: Vec::new(),
                stderr: Vec::new(),
                lines: script.lines,
                iid_file,
                published_id: None,
                inspect: None,
            },
        );
    }

    pub(crate) fn script_reply(&self, tag: &jackin_core::ImageTag, reply: DockerBuildReply) {
        self.model.script_build(tag.clone(), reply);
    }

    pub(crate) async fn plan_for(
        &self,
        request: &ImageRequest,
    ) -> Result<(ImagePlan, jackin_image::FinalBuilds), Box<dyn Error>> {
        let plan = ImagePlan::new(request)?;
        let construct_tag = &plan.construct_build().tag;
        let construct = if let Some(id) = self
            .engine
            .find(construct_tag, &tokio_util::sync::CancellationToken::new())
            .await?
        {
            id
        } else {
            let (tx, rx) = tokio::sync::mpsc::channel(8);
            drop(rx);
            self.engine
                .build(
                    plan.construct_build(),
                    &tx,
                    &tokio_util::sync::CancellationToken::new(),
                )
                .await?
        };
        Ok((plan.clone(), plan.final_builds(&construct)))
    }
}

pub(crate) fn drain(rx: &mut Receiver<ImageEvent>) -> Vec<ImageEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}
