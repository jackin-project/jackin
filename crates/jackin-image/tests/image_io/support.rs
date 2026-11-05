// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_image::ContextEntry;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fs;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, DockerSubcommand, FileSystem, HostPath,
    OutputStream, Platform, ProcessRunner, ProcessStatus, Program, RepoPath, RoleName, Sha256,
    StdFileSystem,
};
use jackin_image::{
    BuildStage, BuildxEngine, BuiltImage, HookFiles, ImageError, ImageEvent, ImageRequest, Rebuild,
    RoleRevision, SourceFile,
};
use jackin_testkit::{
    ProcessEffect, ProcessReply, ScriptedProcessRunner, ScriptedProgram, SeenCall, SeenEnv,
};
use tempfile::TempDir;
use tokio::sync::mpsc::Receiver;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const CONSTRUCT_DOCKERFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/images/construct/Dockerfile"
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

pub(crate) fn tag_id(tag: &str) -> String {
    format!("sha256:{}", Sha256::of(tag.as_bytes()).as_str())
}

pub(crate) fn model_construct_layer() -> String {
    tag_id("the model construct layer")
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Exit(ProcessStatus),
    Timeout,
    UntilCancelled,
}

#[derive(Clone, Debug)]
pub(crate) enum IidFile {
    Id,
    Text(String),
    Absent,
}

#[derive(Clone, Debug)]
pub(crate) enum Delivery {
    Normal,
    Dropped,
    Moved(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Reply {
    pub(crate) status: i32,
    pub(crate) stdout: String,
}

#[derive(Clone, Debug)]
pub(crate) enum AfterBuild {
    Retag { tag: String, id: String },
    FailInspect { tag: String },
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

#[derive(Debug)]
struct ModelState {
    images: BTreeMap<String, String>,
    driver: String,
    scripts: HashMap<BuildStage, Script>,
    bodies: HashMap<BuildStage, Reply>,
    delivery: Delivery,
    after: Vec<(BuildStage, AfterBuild)>,
    broken: Vec<String>,
    fresh: bool,
    built: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct DockerModel {
    state: Arc<Mutex<ModelState>>,
}

fn locked(state: &Mutex<ModelState>) -> MutexGuard<'_, ModelState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn stage_of(tag: &str) -> BuildStage {
    if tag.starts_with("jackin.invalid/construct:") {
        BuildStage::Construct
    } else if tag.starts_with("jackin.invalid/role/") {
        BuildStage::Role
    } else {
        BuildStage::Derived
    }
}

pub(crate) fn tag_of(call: &SeenCall) -> Option<&str> {
    call.args.iter().find_map(|arg| arg.strip_prefix("--tag="))
}

impl DockerModel {
    pub(crate) fn new() -> DockerModel {
        DockerModel {
            state: Arc::new(Mutex::new(ModelState {
                images: BTreeMap::new(),
                driver: "docker".to_owned(),
                scripts: HashMap::new(),
                bodies: HashMap::new(),
                delivery: Delivery::Normal,
                after: Vec::new(),
                broken: Vec::new(),
                fresh: false,
                built: 0,
            })),
        }
    }

    pub(crate) fn fresh_ids(&self) {
        locked(&self.state).fresh = true;
    }

    pub(crate) fn stable_ids(&self) {
        locked(&self.state).fresh = false;
    }

    pub(crate) fn driver(&self, driver: &str) {
        driver.clone_into(&mut locked(&self.state).driver);
    }

    pub(crate) fn script(&self, stage: BuildStage, script: Script) {
        locked(&self.state).scripts.insert(stage, script);
    }

    pub(crate) fn inspect_reply(&self, stage: BuildStage, reply: Reply) {
        locked(&self.state).bodies.insert(stage, reply);
    }

    pub(crate) fn delivery(&self, delivery: Delivery) {
        locked(&self.state).delivery = delivery;
    }

    pub(crate) fn after_build(&self, stage: BuildStage, action: AfterBuild) {
        locked(&self.state).after.push((stage, action));
    }

    pub(crate) fn preload(&self, tag: &str) -> String {
        let id = tag_id(tag);
        locked(&self.state)
            .images
            .insert(tag.to_owned(), id.clone());
        id
    }

    pub(crate) fn plant(&self, tag: &str, id: &str) {
        locked(&self.state)
            .images
            .insert(tag.to_owned(), id.to_owned());
    }

    pub(crate) fn images(&self) -> BTreeMap<String, String> {
        locked(&self.state).images.clone()
    }

    pub(crate) fn install(&self, runner: &ScriptedProcessRunner) {
        let model = self.clone();
        runner.expect_fn(ScriptedProgram::Docker, &[], move |call| model.answer(call));
    }

    fn answer(&self, call: &SeenCall) -> ProcessReply {
        match call.program {
            Program::Docker(DockerSubcommand::BuildxBuild) => self.build(call),
            Program::Docker(DockerSubcommand::ImageInspect) => self.inspect(call),
            Program::Docker(DockerSubcommand::BuildxInspect) => self.builder(),
            _ => ProcessReply::spawn("the model knows only buildx build and the two inspects"),
        }
    }

    fn build(&self, call: &SeenCall) -> ProcessReply {
        let tag = tag_of(call).unwrap_or_default();
        let stage = stage_of(tag);
        let mut state = locked(&self.state);
        let script = state.scripts.get(&stage).cloned().unwrap_or_default();
        state.built += 1;
        let id = if state.fresh {
            tag_id(&format!("{tag}#{}", state.built))
        } else {
            tag_id(tag)
        };
        let succeeded = script.outcome == Outcome::Exit(ProcessStatus::Exited(0));
        if succeeded {
            let stored = match (&state.delivery, stage) {
                (Delivery::Dropped, BuildStage::Derived) => None,
                (Delivery::Moved(other), BuildStage::Derived) => Some(other.clone()),
                _ => Some(id.clone()),
            };
            if let Some(stored) = stored {
                state.images.insert(tag.to_owned(), stored);
            }
            let actions: Vec<AfterBuild> = state
                .after
                .iter()
                .filter(|(due, _)| *due == stage)
                .map(|(_, action)| action.clone())
                .collect();
            for action in actions {
                match action {
                    AfterBuild::Retag { tag, id } => {
                        state.images.insert(tag, id);
                    }
                    AfterBuild::FailInspect { tag } => state.broken.push(tag),
                }
            }
        }
        let mut reply = match script.outcome {
            Outcome::Exit(status) => ProcessReply::lines(status, script.lines),
            Outcome::Timeout => ProcessReply::timeout(),
            Outcome::UntilCancelled => ProcessReply::until_cancelled(script.lines),
        };
        let written = match script.iid {
            IidFile::Id => Some(format!("{id}\n")),
            IidFile::Text(text) => Some(text),
            IidFile::Absent => None,
        };
        if let (true, Some(text)) = (succeeded, written) {
            reply = reply.with_effect(ProcessEffect::WriteArgFile {
                prefix: "--iidfile=".to_owned(),
                bytes: text.into_bytes(),
            });
        }
        reply.with_effect(ProcessEffect::CaptureContext)
    }

    fn inspect_body(&self, id: &str) -> ProcessReply {
        let state = locked(&self.state);
        let tag = state
            .images
            .iter()
            .find_map(|(tag, found)| (found == id).then_some(tag.as_str()));
        let stage = tag.map(stage_of);
        if let Some(reply) = stage.and_then(|stage| state.bodies.get(&stage)) {
            return ProcessReply::exited(
                reply.status,
                reply.stdout.as_bytes(),
                b"inspect failed\n",
            );
        }
        let mut layers = vec![format!("\"{}\"", model_construct_layer())];
        if let (Some(tag), Some(BuildStage::Role | BuildStage::Derived)) = (tag, stage) {
            layers.push(format!("\"{}\"", tag_id(tag)));
        }
        let body = format!(
            "{{\"Id\":\"{id}\",\"Config\":{{\"Env\":[\"PATH=/usr/bin\"],\"OnBuild\":null}},\"RootFS\":{{\"Type\":\"layers\",\"Layers\":[{}]}}}}\n",
            layers.join(",")
        );
        ProcessReply::exited(0, body.as_bytes(), b"")
    }

    fn inspect(&self, call: &SeenCall) -> ProcessReply {
        let tag = call.args.last().map_or("", String::as_str);
        if call.args.iter().any(|arg| arg == "{{json .}}") {
            return self.inspect_body(tag);
        }
        let state = locked(&self.state);
        if state.broken.iter().any(|broken| broken == tag) {
            return ProcessReply::exited(
                1,
                b"",
                b"Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?\n",
            );
        }
        match state.images.get(tag) {
            Some(id) => ProcessReply::exited(0, format!("{id}\n").as_bytes(), b""),
            None => ProcessReply::exited(
                1,
                b"",
                format!("Error response from daemon: No such image: {tag}\n").as_bytes(),
            ),
        }
    }

    fn builder(&self) -> ProcessReply {
        let driver = locked(&self.state).driver.clone();
        let text = format!(
            "Name:          orbstack\nDriver:        {driver}\nLast Activity: 2026-11-02 08:00:00 +0000 UTC\n\nNodes:\nName:             orbstack\nEndpoint:         orbstack\nStatus:           running\n"
        );
        ProcessReply::exited(0, text.as_bytes(), b"")
    }
}

pub(crate) struct Stack {
    pub(crate) fixture: Fixture,
    pub(crate) runner: Arc<ScriptedProcessRunner>,
    pub(crate) model: DockerModel,
    pub(crate) engine: BuildxEngine,
}

impl Stack {
    pub(crate) fn new() -> Result<Stack, Box<dyn Error>> {
        Stack::with_fs(Arc::new(StdFileSystem::default()))
    }

    pub(crate) fn with_fs(fs: Arc<dyn FileSystem>) -> Result<Stack, Box<dyn Error>> {
        let fixture = Fixture::new()?;
        let runner = Arc::new(ScriptedProcessRunner::new());
        let model = DockerModel::new();
        model.install(&runner);
        let shared: Arc<dyn ProcessRunner> = Arc::<ScriptedProcessRunner>::clone(&runner);
        let engine = BuildxEngine::new(shared, fs, fixture.work.clone());
        Ok(Stack {
            fixture,
            runner,
            model,
            engine,
        })
    }

    pub(crate) fn calls(&self, sub: DockerSubcommand) -> Vec<SeenCall> {
        self.runner
            .seen()
            .into_iter()
            .filter(|call| call.program == Program::Docker(sub))
            .collect()
    }

    pub(crate) fn built_tags(&self) -> Vec<String> {
        self.calls(DockerSubcommand::BuildxBuild)
            .iter()
            .filter_map(|call| tag_of(call).map(str::to_owned))
            .collect()
    }
}

pub(crate) async fn until_role_line<F>(
    future: &mut Pin<Box<F>>,
    rx: &mut Receiver<ImageEvent>,
) -> TestResult
where
    F: Future<Output = Result<BuiltImage, ImageError>> + ?Sized,
{
    loop {
        tokio::select! {
            result = future.as_mut() => {
                return Err(format!("the call ended before the role build printed: {result:?}").into());
            }
            event = rx.recv() => match event {
                Some(ImageEvent::Line { stage: BuildStage::Role, .. }) => return Ok(()),
                Some(_) => {}
                None => return Err("the progress channel closed".into()),
            },
        }
    }
}

pub(crate) fn drain(rx: &mut Receiver<ImageEvent>) -> Vec<ImageEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

pub(crate) fn assert_local_only(runner: &ScriptedProcessRunner) -> TestResult {
    for call in runner.seen() {
        let allowed = matches!(
            call.program,
            Program::Docker(
                DockerSubcommand::BuildxBuild
                    | DockerSubcommand::BuildxInspect
                    | DockerSubcommand::ImageInspect
            )
        );
        if !allowed {
            return Err(format!("a non-local program ran: {:?}", call.program).into());
        }
        for arg in &call.args {
            if arg == "--push"
                || arg == "-o"
                || arg.starts_with("--output")
                || arg.contains("type=registry")
            {
                return Err(format!("a registry argument: {arg}").into());
            }
            if let Some(tag) = arg.strip_prefix("--tag=")
                && !tag.starts_with("jackin.invalid/")
            {
                return Err(format!("a tag outside jackin.invalid/: {tag}").into());
            }
        }
        if call.env.iter().any(|(_, value)| *value == SeenEnv::Secret) {
            return Err("a secret env value reached docker".into());
        }
    }
    Ok(())
}
