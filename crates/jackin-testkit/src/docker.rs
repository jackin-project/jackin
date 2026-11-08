// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod stdin;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use jackin_core::{
    DockerSubcommand, HostPath, ImageId, ImageTag, LineSink, LocalImageRef, OutputLine,
    OutputStream, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, ProcessStatus, Program,
    SecretOutput, Sha256,
};
use jackin_image::ImageInspect;
use serde_json::json;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use self::stdin::DockerfileFacts;
use crate::host::{CapturedContext, CapturedFile, FakeClock, Working};

#[derive(Clone, Debug)]
pub struct DockerBoundary {
    state: Arc<Mutex<DockerState>>,
    clock: Option<Arc<FakeClock>>,
}

#[derive(Debug, Default)]
struct DockerState {
    tags: BTreeMap<ImageTag, ImageId>,
    local_pins: BTreeMap<LocalImageRef, ImageId>,
    inspect: BTreeMap<ImageId, ImageInspect>,
    requests: BTreeMap<ImageTag, DockerBuildRequest>,
    builds: BTreeMap<ImageTag, VecDeque<DockerBuildReply>>,
    inspect_replies: BTreeMap<ImageId, DockerCommandReply>,
    builder_driver: String,
    reject_unscripted_builds: bool,
    gates: BTreeMap<ImageTag, Arc<GateState>>,
    local_tag_gates: BTreeMap<LocalImageRef, Arc<GateState>>,
    local_inspect_gates: BTreeMap<LocalImageRef, Arc<GateState>>,
    backpressure: BTreeMap<ImageTag, Arc<BackpressureState>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerBuildRequest {
    pub args: Vec<String>,
    pub stdin_digest: Option<Sha256>,
    pub context: CapturedContext,
    pub context_names: Vec<String>,
    pub inputs: Vec<ImageId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DockerIidFile {
    Id,
    Text(Vec<u8>),
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerBuildReply {
    pub status: ProcessStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub lines: Vec<(OutputStream, String)>,
    pub iid_file: DockerIidFile,
    pub published_id: Option<ImageId>,
    pub inspect: Option<ImageInspect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerCommandReply {
    pub status: ProcessStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct DockerImageRegistry {
    state: Arc<Mutex<DockerState>>,
}

#[derive(Debug)]
struct GateState {
    arrived: AtomicBool,
    released: AtomicBool,
    changed: Notify,
}

#[derive(Clone, Debug)]
pub struct DockerRequestGate {
    state: Arc<GateState>,
}

#[derive(Clone, Debug)]
pub struct DockerBackpressureProbe {
    state: Arc<BackpressureState>,
}

#[derive(Debug)]
struct BackpressureState {
    arrived: AtomicBool,
    changed: Notify,
}

fn locked(state: &Mutex<DockerState>) -> std::sync::MutexGuard<'_, DockerState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Default for DockerBuildReply {
    fn default() -> DockerBuildReply {
        DockerBuildReply {
            status: ProcessStatus::Exited(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
            lines: Vec::new(),
            iid_file: DockerIidFile::Id,
            published_id: None,
            inspect: None,
        }
    }
}

impl DockerBuildReply {
    pub fn failed(message: &[u8]) -> DockerBuildReply {
        DockerBuildReply {
            status: ProcessStatus::Exited(1),
            stderr: message.to_vec(),
            iid_file: DockerIidFile::Missing,
            ..DockerBuildReply::default()
        }
    }

    #[must_use]
    pub fn with_lines(mut self, lines: Vec<(OutputStream, String)>) -> DockerBuildReply {
        self.lines = lines;
        self
    }

    #[must_use]
    pub fn with_iid_file(mut self, iid_file: DockerIidFile) -> DockerBuildReply {
        self.iid_file = iid_file;
        self
    }

    #[must_use]
    pub fn with_published_id(mut self, id: ImageId) -> DockerBuildReply {
        self.published_id = Some(id);
        self
    }

    #[must_use]
    pub fn with_inspect(mut self, inspect: ImageInspect) -> DockerBuildReply {
        self.inspect = Some(inspect);
        self
    }
}

impl DockerCommandReply {
    pub fn exited(code: i32, stdout: &[u8], stderr: &[u8]) -> DockerCommandReply {
        DockerCommandReply {
            status: ProcessStatus::Exited(code),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }
}

impl DockerBoundary {
    pub fn new() -> DockerBoundary {
        DockerBoundary::with_clock(None)
    }

    pub fn rejecting_builds() -> DockerBoundary {
        let boundary = DockerBoundary::new();
        locked(&boundary.state).reject_unscripted_builds = true;
        boundary
    }

    pub fn with_fake_clock(clock: Arc<FakeClock>) -> DockerBoundary {
        DockerBoundary::with_clock(Some(clock))
    }

    fn with_clock(clock: Option<Arc<FakeClock>>) -> DockerBoundary {
        DockerBoundary {
            state: Arc::new(Mutex::new(DockerState {
                builder_driver: "docker".to_owned(),
                ..DockerState::default()
            })),
            clock,
        }
    }

    pub fn registry(&self) -> DockerImageRegistry {
        DockerImageRegistry {
            state: Arc::clone(&self.state),
        }
    }

    pub fn seed_image(&self, tag: ImageTag, id: ImageId, inspect: ImageInspect) {
        let mut state = locked(&self.state);
        state.tags.insert(tag, id.clone());
        state.inspect.insert(id, inspect);
    }

    pub fn retag(&self, tag: ImageTag, id: Option<ImageId>) {
        let mut state = locked(&self.state);
        match id {
            Some(id) => {
                state.inspect.entry(id.clone()).or_default();
                state.tags.insert(tag, id);
            }
            None => {
                state.tags.remove(&tag);
            }
        }
    }

    pub fn retag_local(&self, reference: LocalImageRef, id: Option<ImageId>) {
        let mut state = locked(&self.state);
        match id {
            Some(id) => {
                state.inspect.entry(id.clone()).or_default();
                state.local_pins.insert(reference, id);
            }
            None => {
                state.local_pins.remove(&reference);
            }
        }
    }

    pub fn local_image_id(&self, reference: &LocalImageRef) -> Option<ImageId> {
        self.registry().local_image_id(reference)
    }

    pub fn local_images(&self) -> BTreeMap<LocalImageRef, ImageId> {
        locked(&self.state).local_pins.clone()
    }

    pub fn image_id(&self, tag: &ImageTag) -> Option<ImageId> {
        self.registry().image_id(tag)
    }

    pub fn images(&self) -> BTreeMap<ImageTag, ImageId> {
        self.registry().images()
    }

    pub fn request_for(&self, tag: &ImageTag) -> Option<DockerBuildRequest> {
        locked(&self.state).requests.get(tag).cloned()
    }

    pub fn set_builder_driver(&self, driver: &str) {
        driver.clone_into(&mut locked(&self.state).builder_driver);
    }

    pub fn script_build(&self, tag: ImageTag, reply: DockerBuildReply) {
        locked(&self.state)
            .builds
            .entry(tag)
            .or_default()
            .push_back(reply);
    }

    pub fn script_inspect(&self, id: ImageId, reply: DockerCommandReply) {
        locked(&self.state).inspect_replies.insert(id, reply);
    }

    pub fn hold_build(&self, tag: ImageTag) -> DockerRequestGate {
        let state = Arc::new(GateState {
            arrived: AtomicBool::new(false),
            released: AtomicBool::new(false),
            changed: Notify::new(),
        });
        locked(&self.state).gates.insert(tag, Arc::clone(&state));
        DockerRequestGate { state }
    }

    pub fn hold_local_tag(&self, reference: LocalImageRef) -> DockerRequestGate {
        let state = Arc::new(GateState {
            arrived: AtomicBool::new(false),
            released: AtomicBool::new(false),
            changed: Notify::new(),
        });
        locked(&self.state)
            .local_tag_gates
            .insert(reference, Arc::clone(&state));
        DockerRequestGate { state }
    }

    pub fn hold_local_inspect(&self, reference: LocalImageRef) -> DockerRequestGate {
        let state = Arc::new(GateState {
            arrived: AtomicBool::new(false),
            released: AtomicBool::new(false),
            changed: Notify::new(),
        });
        locked(&self.state)
            .local_inspect_gates
            .insert(reference, Arc::clone(&state));
        DockerRequestGate { state }
    }

    pub fn observe_backpressure(&self, tag: ImageTag) -> DockerBackpressureProbe {
        let state = Arc::new(BackpressureState {
            arrived: AtomicBool::new(false),
            changed: Notify::new(),
        });
        locked(&self.state)
            .backpressure
            .insert(tag, Arc::clone(&state));
        DockerBackpressureProbe { state }
    }

    fn take_gate(&self, tag: &ImageTag) -> Option<Arc<GateState>> {
        locked(&self.state).gates.remove(tag)
    }

    fn take_local_tag_gate(&self, reference: &LocalImageRef) -> Option<Arc<GateState>> {
        locked(&self.state).local_tag_gates.remove(reference)
    }

    fn take_local_inspect_gate(&self, reference: &LocalImageRef) -> Option<Arc<GateState>> {
        locked(&self.state).local_inspect_gates.remove(reference)
    }

    fn signal_backpressure(&self, tag: &ImageTag) {
        let state = locked(&self.state).backpressure.remove(tag);
        if let Some(state) = state {
            state.arrived.store(true, Ordering::SeqCst);
            state.changed.notify_waiters();
        }
    }

    async fn build(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        let program = spec.program;
        validate_build_spec(spec)?;
        let tag_text = spec
            .args
            .iter()
            .find_map(|arg| arg.strip_prefix("--tag="))
            .ok_or_else(|| spawn(program, "build request has no output tag"))?;
        let tag = ImageTag::parse(tag_text).map_err(|error| spawn(program, error.to_string()))?;
        let gate = self.take_gate(&tag);
        let mut working = self.clock.as_ref().map(FakeClock::working);
        let parsed = docker_request(spec).await?;
        let contexts = build_contexts(&spec.args)?;
        let (request, context_layers, missing) = {
            let mut state = locked(&self.state);
            let mut inputs = Vec::new();
            let mut context_layers = BTreeMap::new();
            let mut missing = None;
            for context in &contexts {
                let Some(id) = state.tags.get(&context.tag).cloned() else {
                    missing = Some(context.tag.clone());
                    continue;
                };
                let layers = state
                    .inspect
                    .get(&id)
                    .map(|inspect| inspect.layers.clone())
                    .unwrap_or_default();
                inputs.push(id);
                context_layers.insert(context.name.clone(), layers);
            }
            let request = DockerBuildRequest {
                args: parsed.args,
                stdin_digest: parsed.stdin_digest,
                context: parsed.context,
                context_names: contexts
                    .iter()
                    .filter(|context| state.tags.contains_key(&context.tag))
                    .map(|context| context.name.clone())
                    .collect(),
                inputs: inputs.clone(),
            };
            state.requests.insert(tag.clone(), request.clone());
            (request, context_layers, missing)
        };
        if let Some(gate) = &gate {
            drop(working.take());
            gate.wait(cancel, program).await?;
            working = self.clock.as_ref().map(FakeClock::working);
        }
        if cancel.is_cancelled() {
            drop(working);
            return Err(ProcessError::Cancelled {
                program,
                group: None,
            });
        }
        if let Some(missing) = missing {
            drop(working);
            return Ok(ProcessOutput {
                status: ProcessStatus::Exited(1),
                stdout: Vec::new(),
                stderr: format!("No such image: {missing}\n").into_bytes(),
            });
        }
        let mut reply = {
            let mut state = locked(&self.state);
            let scripted = state.builds.get_mut(&tag).and_then(VecDeque::pop_front);
            match scripted {
                Some(reply) => reply,
                None if state.reject_unscripted_builds => {
                    DockerBuildReply::failed(b"unscripted Docker builds are disabled\n")
                }
                None => DockerBuildReply::default(),
            }
        };
        let mut completed = None;
        if reply.status == ProcessStatus::Exited(0) {
            if cancel.is_cancelled() {
                drop(working);
                return Err(ProcessError::Cancelled {
                    program,
                    group: None,
                });
            }
            let (id, inspect) = external_result(&request, &parsed.dockerfile, &context_layers)?;
            if reply.inspect.is_none() {
                reply.inspect = Some(inspect.clone());
            }
            if matches!(reply.iid_file, DockerIidFile::Id) {
                reply.iid_file = DockerIidFile::Text(format!("{id}\n").into_bytes());
            }
            completed = Some((id, inspect));
        }
        self.emit(&tag, lines, &reply.lines, cancel, program, &mut working)
            .await?;
        if cancel.is_cancelled() {
            drop(working);
            return Err(ProcessError::Cancelled {
                program,
                group: None,
            });
        }
        if let Some((id, inspect)) = completed {
            write_iid(spec, &reply.iid_file)?;
            let published = reply.published_id.clone().unwrap_or(id);
            let mut state = locked(&self.state);
            state
                .inspect
                .insert(published.clone(), reply.inspect.clone().unwrap_or(inspect));
            state.tags.insert(tag, published);
        }
        drop(working);
        Ok(ProcessOutput {
            status: reply.status,
            stdout: reply.stdout,
            stderr: reply.stderr,
        })
    }

    async fn emit(
        &self,
        tag: &ImageTag,
        lines: Option<&LineSink>,
        items: &[(OutputStream, String)],
        cancel: &CancellationToken,
        program: Program,
        working: &mut Option<Working>,
    ) -> Result<(), ProcessError> {
        let Some(lines) = lines else {
            return Ok(());
        };
        for (stream, text) in items {
            let output = OutputLine {
                stream: *stream,
                text: text.clone(),
            };
            if !lines.is_closed() && lines.capacity() == 0 {
                drop(working.take());
                self.signal_backpressure(tag);
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    return Err(ProcessError::Cancelled { program, group: None });
                }
                sent = lines.send(output) => drop(sent),
            }
            if cancel.is_cancelled() {
                return Err(ProcessError::Cancelled {
                    program,
                    group: None,
                });
            }
            if working.is_none() {
                *working = self.clock.as_ref().map(FakeClock::working);
            }
        }
        Ok(())
    }

    async fn inspect(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        let target = spec.args.last().map_or("", String::as_str);
        let format = spec
            .args
            .windows(2)
            .find_map(|pair| (pair[0] == "--format").then_some(pair[1].as_str()));
        if format == Some("{{.Id}}")
            && let Some(reference) = local_ref_from_text(target)
            && let Some(gate) = self.take_local_inspect_gate(&reference)
        {
            gate.wait(cancel, spec.program).await?;
        }
        let state = locked(&self.state);
        let id = match format {
            Some("{{.Id}}") if spec.args.len() == 3 => ImageTag::parse(target)
                .ok()
                .and_then(|tag| state.tags.get(&tag).cloned())
                .or_else(|| {
                    local_ref_from_text(target)
                        .and_then(|reference| state.local_pins.get(&reference).cloned())
                }),
            Some("{{json .}}") if spec.args.len() == 3 => ImageId::parse(target)
                .ok()
                .filter(|id| state.inspect.contains_key(id)),
            _ => {
                return Ok(ProcessOutput {
                    status: ProcessStatus::Exited(1),
                    stdout: Vec::new(),
                    stderr: b"unsupported Docker image inspect request\n".to_vec(),
                });
            }
        };
        let Some(id) = id else {
            return Ok(ProcessOutput {
                status: ProcessStatus::Exited(1),
                stdout: Vec::new(),
                stderr: format!("Error response from daemon: No such image: {target}\n")
                    .into_bytes(),
            });
        };
        if let Some(reply) = state.inspect_replies.get(&id) {
            return Ok(ProcessOutput {
                status: reply.status,
                stdout: reply.stdout.clone(),
                stderr: reply.stderr.clone(),
            });
        }
        if format == Some("{{.Id}}") {
            return Ok(ProcessOutput {
                status: ProcessStatus::Exited(0),
                stdout: format!("{id}\n").into_bytes(),
                stderr: Vec::new(),
            });
        }
        let inspect = state.inspect.get(&id).cloned().unwrap_or_default();
        let body = json!({
            "Id": id,
            "Config": { "Env": inspect.env, "OnBuild": inspect.on_build },
            "RootFS": { "Type": "layers", "Layers": inspect.layers },
        });
        Ok(ProcessOutput {
            status: ProcessStatus::Exited(0),
            stdout: serde_json::to_vec(&body).unwrap_or_default(),
            stderr: Vec::new(),
        })
    }

    async fn tag_local(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        let program = spec.program;
        let [source, destination] = spec.args.as_slice() else {
            return Err(spawn(
                program,
                "Docker image tag requires an image ID and destination",
            ));
        };
        let id = ImageId::parse(source).map_err(|error| spawn(program, error))?;
        let reference = local_ref_from_text(destination).ok_or_else(|| {
            spawn(
                program,
                "Docker image tag destination is not a native local image reference",
            )
        })?;
        if let Some(gate) = self.take_local_tag_gate(&reference) {
            gate.wait(cancel, program).await?;
        }
        if cancel.is_cancelled() {
            return Err(ProcessError::Cancelled {
                program,
                group: None,
            });
        }
        let mut state = locked(&self.state);
        if !state.inspect.contains_key(&id) {
            return Ok(ProcessOutput {
                status: ProcessStatus::Exited(1),
                stdout: Vec::new(),
                stderr: format!("Error response from daemon: No such image: {id}\n").into_bytes(),
            });
        }
        state.local_pins.insert(reference, id);
        Ok(ProcessOutput {
            status: ProcessStatus::Exited(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }

    fn builder(&self, spec: &ProcessSpec) -> ProcessOutput {
        let state = locked(&self.state);
        let Some(name) = spec.args.first().filter(|_| spec.args.len() == 1) else {
            return ProcessOutput {
                status: ProcessStatus::Exited(1),
                stdout: Vec::new(),
                stderr: b"unsupported Docker builder inspect request\n".to_vec(),
            };
        };
        ProcessOutput {
            status: ProcessStatus::Exited(0),
            stdout: format!(
                "Name:          {name}\nDriver:        {}\n",
                state.builder_driver
            )
            .into_bytes(),
            stderr: Vec::new(),
        }
    }
}

impl Default for DockerBoundary {
    fn default() -> DockerBoundary {
        DockerBoundary::new()
    }
}

impl DockerImageRegistry {
    pub fn image_id(&self, tag: &ImageTag) -> Option<ImageId> {
        locked(&self.state).tags.get(tag).cloned()
    }

    pub fn images(&self) -> BTreeMap<ImageTag, ImageId> {
        locked(&self.state).tags.clone()
    }

    pub fn local_image_id(&self, reference: &LocalImageRef) -> Option<ImageId> {
        locked(&self.state).local_pins.get(reference).cloned()
    }
}

impl DockerRequestGate {
    pub async fn wait_arrived(&self) {
        loop {
            let changed = self.state.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.state.arrived.load(Ordering::SeqCst) {
                return;
            }
            changed.await;
        }
    }

    pub fn release(&self) {
        self.state.released.store(true, Ordering::SeqCst);
        self.state.changed.notify_waiters();
    }
}

impl DockerBackpressureProbe {
    pub async fn wait_arrived(&self) {
        loop {
            let changed = self.state.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.state.arrived.load(Ordering::SeqCst) {
                return;
            }
            changed.await;
        }
    }
}

impl GateState {
    async fn wait(&self, cancel: &CancellationToken, program: Program) -> Result<(), ProcessError> {
        self.arrived.store(true, Ordering::SeqCst);
        self.changed.notify_waiters();
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.released.load(Ordering::SeqCst) {
                return Ok(());
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(ProcessError::Cancelled { program, group: None }),
                () = changed => {}
            }
        }
    }
}

#[async_trait]
impl ProcessRunner for DockerBoundary {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        if cancel.is_cancelled() {
            return Err(ProcessError::Cancelled {
                program: spec.program,
                group: None,
            });
        }
        match spec.program {
            Program::Docker(DockerSubcommand::BuildxBuild) => self.build(spec, lines, cancel).await,
            Program::Docker(DockerSubcommand::ImageInspect) => {
                if spec.cwd.is_some()
                    || !spec.env.is_empty()
                    || !spec.env_remove_prefixes.is_empty()
                {
                    return Err(spawn(
                        spec.program,
                        "Docker process environment customization is unsupported",
                    ));
                }
                self.inspect(spec, cancel).await
            }
            Program::Docker(DockerSubcommand::ImageTag) => {
                if spec.cwd.is_some()
                    || !spec.env.is_empty()
                    || !spec.env_remove_prefixes.is_empty()
                {
                    return Err(spawn(
                        spec.program,
                        "Docker process environment customization is unsupported",
                    ));
                }
                self.tag_local(spec, cancel).await
            }
            Program::Docker(DockerSubcommand::BuildxInspect) => {
                if spec.cwd.is_some()
                    || !spec.env.is_empty()
                    || !spec.env_remove_prefixes.is_empty()
                {
                    return Err(spawn(
                        spec.program,
                        "Docker process environment customization is unsupported",
                    ));
                }
                Ok(self.builder(spec))
            }
            _ => Err(spawn(
                spec.program,
                "the Docker boundary does not support this command",
            )),
        }
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        _cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        Err(spawn(spec.program, "secret process output is not modeled"))
    }
}

struct ParsedRequest {
    args: Vec<String>,
    stdin_digest: Option<Sha256>,
    dockerfile: DockerfileFacts,
    context: CapturedContext,
}

struct BuildContext {
    name: String,
    tag: ImageTag,
}

async fn docker_request(spec: &ProcessSpec) -> Result<ParsedRequest, ProcessError> {
    if spec.cwd.is_some() || !spec.env.is_empty() || !spec.env_remove_prefixes.is_empty() {
        return Err(spawn(
            spec.program,
            "Docker process environment customization is unsupported",
        ));
    }
    let root_text = spec
        .args
        .last()
        .ok_or_else(|| spawn(spec.program, "build request has no context"))?;
    let root = HostPath::parse(Path::new(root_text))
        .map_err(|error| spawn(spec.program, error.to_string()))?;
    let files = snapshot(root.as_path()).map_err(|error| spawn(spec.program, error))?;
    let captured = stdin::capture(&spec.stdin, &spec.args, spec.program).await?;
    let (stdin_digest, dockerfile) = if let Some(captured) = captured {
        (Some(captured.digest), captured.dockerfile)
    } else {
        let bytes = files
            .get("Dockerfile")
            .and_then(|file| match file {
                CapturedFile::File { bytes, .. } => Some(bytes.as_slice()),
                CapturedFile::Link { .. } => None,
            })
            .ok_or_else(|| spawn(spec.program, "Docker context has no regular Dockerfile"))?;
        (None, stdin::from_context(bytes, &spec.args, spec.program)?)
    };
    Ok(ParsedRequest {
        args: spec.args.clone(),
        stdin_digest,
        dockerfile,
        context: CapturedContext { path: root, files },
    })
}

fn validate_build_spec(spec: &ProcessSpec) -> Result<(), ProcessError> {
    let program = spec.program;
    let invalid = || spawn(program, "unsupported Docker build request");
    if program != Program::Docker(DockerSubcommand::BuildxBuild)
        || spec.cwd.is_some()
        || !spec.env.is_empty()
        || !spec.env_remove_prefixes.is_empty()
        || spec.args.len() < 3
    {
        return Err(invalid());
    }
    let mut tags = 0;
    let mut iid_files = 0;
    let mut builders = 0;
    let mut platforms = 0;
    let mut contexts = BTreeSet::<String>::new();
    for argument in &spec.args[..spec.args.len() - 1] {
        if argument.starts_with("--tag=") {
            tags += 1;
        } else if argument.starts_with("--iidfile=") {
            iid_files += 1;
        } else if argument.starts_with("--builder=") {
            builders += 1;
        } else if argument.starts_with("--platform=") {
            platforms += 1;
        } else if let Some(context) = argument.strip_prefix("--build-context=") {
            let (name, _) = context.split_once("=docker-image://").ok_or_else(invalid)?;
            if name.is_empty() || !contexts.insert(name.to_owned()) {
                return Err(invalid());
            }
        } else if argument == "--provenance=false"
            || argument == "--sbom=false"
            || argument == "--load"
            || argument == "--progress=plain"
            || argument == "--no-cache"
            || argument == "--file=-"
            || argument.starts_with("--build-arg=")
        {
        } else {
            return Err(invalid());
        }
    }
    let context = spec.args.last().map_or("", String::as_str);
    if tags != 1
        || iid_files != 1
        || builders != 1
        || platforms != 1
        || !Path::new(context).is_absolute()
    {
        return Err(invalid());
    }
    Ok(())
}

fn snapshot(root: &Path) -> Result<BTreeMap<String, CapturedFile>, String> {
    let mut files = BTreeMap::new();
    snapshot_dir(root, root, &mut files)?;
    Ok(files)
}

fn snapshot_dir(
    root: &Path,
    path: &Path,
    files: &mut BTreeMap<String, CapturedFile>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let target = entry.path();
        let relative = target
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = fs::symlink_metadata(&target).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            let link = fs::read_link(&target).map_err(|error| error.to_string())?;
            let text = link
                .to_str()
                .ok_or_else(|| "non-UTF-8 context link".to_owned())?;
            files.insert(relative, CapturedFile::link(text));
        } else if metadata.is_dir() {
            snapshot_dir(root, &target, files)?;
        } else if metadata.is_file() {
            let bytes = fs::read(&target).map_err(|error| error.to_string())?;
            files.insert(
                relative,
                CapturedFile::file(&bytes, metadata.permissions().mode() & 0o777),
            );
        } else {
            return Err("unsupported Docker context entry".to_owned());
        }
    }
    Ok(())
}

fn build_contexts(args: &[String]) -> Result<Vec<BuildContext>, ProcessError> {
    args.iter()
        .filter_map(|arg| arg.strip_prefix("--build-context="))
        .map(|arg| {
            let (name, source) = arg.split_once("=docker-image://").ok_or_else(|| {
                spawn(
                    Program::Docker(DockerSubcommand::BuildxBuild),
                    "unsupported build context",
                )
            })?;
            ImageTag::parse(source)
                .map(|tag| BuildContext {
                    name: name.to_owned(),
                    tag,
                })
                .map_err(|error| {
                    spawn(
                        Program::Docker(DockerSubcommand::BuildxBuild),
                        error.to_string(),
                    )
                })
        })
        .collect()
}

fn external_result(
    request: &DockerBuildRequest,
    dockerfile: &DockerfileFacts,
    context_layers: &BTreeMap<String, Vec<String>>,
) -> Result<(ImageId, ImageInspect), ProcessError> {
    let mut identity = Vec::new();
    for arg in &request.args {
        if !arg.starts_with("--iidfile=")
            && !arg.starts_with("--tag=")
            && !arg.starts_with("--builder=")
            && !arg.starts_with("--build-context=")
            && !matches!(
                arg.as_str(),
                "--no-cache"
                    | "--provenance=false"
                    | "--sbom=false"
                    | "--load"
                    | "--progress=plain"
                    | "--file=-"
            )
            && !Path::new(arg).is_absolute()
        {
            identity.extend_from_slice(arg.as_bytes());
            identity.push(0);
        }
    }
    for (name, id) in request.context_names.iter().zip(&request.inputs) {
        identity.extend_from_slice(name.as_bytes());
        identity.push(0);
        identity.extend_from_slice(id.to_string().as_bytes());
        identity.push(0);
    }
    if let Some(stdin_digest) = &request.stdin_digest {
        identity.extend_from_slice(stdin_digest.as_str().as_bytes());
    }
    for (name, file) in &request.context.files {
        identity.extend_from_slice(name.as_bytes());
        identity.push(0);
        match file {
            CapturedFile::File { bytes, mode } => {
                identity.extend_from_slice(&mode.0.to_be_bytes());
                identity.extend_from_slice(bytes);
            }
            CapturedFile::Link { target } => identity.extend_from_slice(target.as_bytes()),
        }
        identity.push(0);
    }
    let digest = Sha256::of(&identity);
    let id = ImageId::parse(&format!("sha256:{}", digest.as_str())).map_err(|_| {
        spawn(
            Program::Docker(DockerSubcommand::BuildxBuild),
            "invalid image ID",
        )
    })?;
    let layers = final_base_layers(
        dockerfile,
        context_layers,
        &digest,
        Program::Docker(DockerSubcommand::BuildxBuild),
    )?;
    Ok((
        id,
        ImageInspect {
            layers,
            env: dockerfile.env.clone(),
            on_build: dockerfile.on_build.clone(),
        },
    ))
}

fn final_base_layers(
    dockerfile: &DockerfileFacts,
    context_layers: &BTreeMap<String, Vec<String>>,
    request_digest: &Sha256,
    program: Program,
) -> Result<Vec<String>, ProcessError> {
    fn layer_digest(label: &str) -> String {
        format!("sha256:{}", Sha256::of(label.as_bytes()).as_str())
    }

    fn stage_layers(
        index: usize,
        dockerfile: &DockerfileFacts,
        context_layers: &BTreeMap<String, Vec<String>>,
        request_digest: &Sha256,
        program: Program,
    ) -> Result<Vec<String>, ProcessError> {
        let stage = dockerfile
            .stages
            .get(index)
            .ok_or_else(|| spawn(program, "unsupported Dockerfile stage reference"))?;
        let base = dockerfile
            .expanded_base(&stage.base)
            .ok_or_else(|| spawn(program, "unsupported Dockerfile base reference"))?;
        let mut layers = if base.eq_ignore_ascii_case("scratch") {
            Vec::new()
        } else if let Some(layers) = context_layers.get(&base) {
            layers.clone()
        } else if let Some(previous) = dockerfile.stages[..index].iter().rposition(|candidate| {
            candidate
                .name
                .as_ref()
                .is_some_and(|name| name.eq_ignore_ascii_case(&base))
        }) {
            stage_layers(
                previous,
                dockerfile,
                context_layers,
                request_digest,
                program,
            )?
        } else {
            vec![layer_digest(&format!("external-base:{base}"))]
        };
        layers.push(layer_digest(&format!("stage:{index}:{request_digest}")));
        Ok(layers)
    }

    let index = dockerfile
        .stages
        .len()
        .checked_sub(1)
        .ok_or_else(|| spawn(program, "Dockerfile has no stages"))?;
    stage_layers(index, dockerfile, context_layers, request_digest, program)
}

fn write_iid(spec: &ProcessSpec, iid: &DockerIidFile) -> Result<(), ProcessError> {
    let Some(path) = spec
        .args
        .iter()
        .find_map(|arg| arg.strip_prefix("--iidfile="))
    else {
        return Err(spawn(spec.program, "build request has no iid file"));
    };
    let path = Path::new(path);
    match iid {
        DockerIidFile::Missing => {
            drop(fs::remove_file(path));
        }
        DockerIidFile::Id => return Err(spawn(spec.program, "iid file was not resolved")),
        DockerIidFile::Text(bytes) => {
            fs::write(path, bytes).map_err(|error| spawn(spec.program, error.to_string()))?;
        }
    }
    Ok(())
}

fn local_ref_from_text(text: &str) -> Option<LocalImageRef> {
    let digest = text.strip_prefix("openshell/sandbox-from:")?;
    let id = ImageId::parse(&format!("sha256:{digest}")).ok()?;
    Some(LocalImageRef::for_image(&id))
}

fn spawn(program: Program, message: impl ToString) -> ProcessError {
    ProcessError::Spawn {
        program,
        message: message.to_string(),
    }
}
