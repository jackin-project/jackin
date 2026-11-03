// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    DockerSubcommand, EnvValue, FileKind, FileMode, FileSystem, FsError, HostPath, LineSink,
    OutputLine, OutputStream, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec,
    ProcessStatus, ProcessStdin, Program, RepoPath, SecretBytes, SecretOutput, StdFileSystem,
    write_atomic,
};
use tokio_util::sync::CancellationToken;

use crate::exec::{Failure, HostProgram, run_command};
use crate::sync::locked;

const TAIL_LIMIT: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScriptedProgram {
    Docker,
    Openshell,
    Ssh,
    Op,
}

impl ScriptedProgram {
    fn of(program: Program) -> Option<ScriptedProgram> {
        match program {
            Program::Docker(_) => Some(ScriptedProgram::Docker),
            Program::Openshell => Some(ScriptedProgram::Openshell),
            Program::Ssh => Some(ScriptedProgram::Ssh),
            Program::Op => Some(ScriptedProgram::Op),
            Program::Git => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ReplyBody {
    Output {
        status: ProcessStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    Lines {
        status: ProcessStatus,
        lines: Vec<(OutputStream, String)>,
    },
    UntilCancelled {
        lines: Vec<(OutputStream, String)>,
    },
    Timeout,
    Cancelled,
    Spawn {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessEffect {
    WriteArgFile { prefix: String, bytes: Vec<u8> },
    CaptureContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessReply {
    body: ReplyBody,
    effects: Vec<ProcessEffect>,
}

impl ProcessReply {
    fn of(body: ReplyBody) -> ProcessReply {
        ProcessReply {
            body,
            effects: Vec::new(),
        }
    }

    pub fn exited(code: i32, stdout: &[u8], stderr: &[u8]) -> ProcessReply {
        ProcessReply::of(ReplyBody::Output {
            status: ProcessStatus::Exited(code),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        })
    }

    pub fn lines(status: ProcessStatus, lines: Vec<(OutputStream, String)>) -> ProcessReply {
        ProcessReply::of(ReplyBody::Lines { status, lines })
    }

    pub fn until_cancelled(lines: Vec<(OutputStream, String)>) -> ProcessReply {
        ProcessReply::of(ReplyBody::UntilCancelled { lines })
    }

    pub fn timeout() -> ProcessReply {
        ProcessReply::of(ReplyBody::Timeout)
    }

    pub fn cancelled() -> ProcessReply {
        ProcessReply::of(ReplyBody::Cancelled)
    }

    pub fn spawn(message: &str) -> ProcessReply {
        ProcessReply::of(ReplyBody::Spawn {
            message: message.to_owned(),
        })
    }

    #[must_use]
    pub fn with_effect(mut self, effect: ProcessEffect) -> ProcessReply {
        self.effects.push(effect);
        self
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum SeenEnv {
    Plain(String),
    Secret,
}

impl fmt::Debug for SeenEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeenEnv::Plain(text) => f.debug_tuple("Plain").field(text).finish(),
            SeenEnv::Secret => f.write_str("Secret(<redacted>)"),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum SeenStdin {
    Null,
    Bytes(Vec<u8>),
    File { path: HostPath, offset: u64 },
}

impl fmt::Debug for SeenStdin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeenStdin::Null => f.write_str("Null"),
            SeenStdin::Bytes(bytes) => write!(f, "Bytes({} bytes)", bytes.len()),
            SeenStdin::File { path, offset } => f
                .debug_struct("File")
                .field("path", path)
                .field("offset", offset)
                .finish(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeenCall {
    pub program: Program,
    pub args: Vec<String>,
    pub cwd: Option<HostPath>,
    pub env: Vec<(String, SeenEnv)>,
    pub env_remove_prefixes: Vec<String>,
    pub stdin: SeenStdin,
    pub timeout: Duration,
}

impl SeenCall {
    fn of(spec: &ProcessSpec) -> SeenCall {
        SeenCall {
            program: spec.program,
            args: spec.args.clone(),
            cwd: spec.cwd.clone(),
            env: spec
                .env
                .iter()
                .map(|(name, value)| {
                    let value = match value {
                        EnvValue::Plain(text) => SeenEnv::Plain(text.clone()),
                        EnvValue::Secret(_) => SeenEnv::Secret,
                    };
                    (name.clone(), value)
                })
                .collect(),
            env_remove_prefixes: spec.env_remove_prefixes.clone(),
            stdin: match &spec.stdin {
                ProcessStdin::Null => SeenStdin::Null,
                ProcessStdin::Bytes(bytes) => SeenStdin::Bytes(bytes.expose().to_vec()),
                ProcessStdin::File { path, offset } => SeenStdin::File {
                    path: path.clone(),
                    offset: *offset,
                },
            },
            timeout: spec.timeout,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapturedFile {
    File { bytes: Vec<u8>, mode: FileMode },
    Link { target: String },
}

impl CapturedFile {
    pub fn file(bytes: &[u8], mode: u32) -> CapturedFile {
        CapturedFile::File {
            bytes: bytes.to_vec(),
            mode: FileMode(mode),
        }
    }

    pub fn link(target: &str) -> CapturedFile {
        CapturedFile::Link {
            target: target.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedContext {
    pub path: HostPath,
    pub files: BTreeMap<String, CapturedFile>,
}

#[derive(Clone, Copy, Debug)]
enum Selector {
    Program(ScriptedProgram),
    Docker(DockerSubcommand),
    Git,
}

impl Selector {
    fn matches(self, program: Program) -> bool {
        match (self, program) {
            (Selector::Program(wanted), program) => ScriptedProgram::of(program) == Some(wanted),
            (Selector::Docker(wanted), Program::Docker(found)) => wanted == found,
            (Selector::Git, program) => program == Program::Git,
            (Selector::Docker(_), _) => false,
        }
    }
}

type ReplyFn = Arc<dyn Fn(&SeenCall) -> ProcessReply + Send + Sync>;

#[derive(Clone)]
enum Replier {
    Fixed(ProcessReply),
    Computed(ReplyFn),
}

impl fmt::Debug for Replier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Replier::Fixed(reply) => f.debug_tuple("Fixed").field(reply).finish(),
            Replier::Computed(_) => f.write_str("Computed(..)"),
        }
    }
}

#[derive(Debug)]
struct Expectation {
    selector: Selector,
    prefix: Vec<String>,
    replier: Replier,
    used: u32,
}

#[derive(Debug, Default)]
struct RunnerState {
    expectations: Vec<Expectation>,
    seen: Vec<SeenCall>,
    captured: Vec<CapturedContext>,
}

#[derive(Debug, Default)]
pub struct ScriptedProcessRunner {
    state: Mutex<RunnerState>,
}

impl ScriptedProcessRunner {
    pub fn new() -> ScriptedProcessRunner {
        ScriptedProcessRunner::default()
    }

    pub fn expect(&self, program: ScriptedProgram, args_prefix: &[&str], reply: ProcessReply) {
        self.push(
            Selector::Program(program),
            args_prefix,
            Replier::Fixed(reply),
        );
    }

    pub fn expect_fn(
        &self,
        program: ScriptedProgram,
        args_prefix: &[&str],
        reply: impl Fn(&SeenCall) -> ProcessReply + Send + Sync + 'static,
    ) {
        self.push(
            Selector::Program(program),
            args_prefix,
            Replier::Computed(Arc::new(reply)),
        );
    }

    pub fn expect_git_version(&self, reply: ProcessReply) {
        self.push(Selector::Git, &["--version"], Replier::Fixed(reply));
    }

    pub fn expect_docker(
        &self,
        subcommand: DockerSubcommand,
        args_prefix: &[&str],
        reply: ProcessReply,
    ) {
        self.push(
            Selector::Docker(subcommand),
            args_prefix,
            Replier::Fixed(reply),
        );
    }

    fn expects(&self, spec: &ProcessSpec) -> bool {
        locked(&self.state).expectations.iter().any(|expectation| {
            expectation.selector.matches(spec.program) && spec.args.starts_with(&expectation.prefix)
        })
    }

    pub fn seen(&self) -> Vec<SeenCall> {
        locked(&self.state).seen.clone()
    }

    pub fn captured_contexts(&self) -> Vec<CapturedContext> {
        locked(&self.state).captured.clone()
    }

    fn push(&self, selector: Selector, args_prefix: &[&str], replier: Replier) {
        locked(&self.state).expectations.push(Expectation {
            selector,
            prefix: args_prefix.iter().map(|arg| (*arg).to_owned()).collect(),
            replier,
            used: 0,
        });
    }

    fn choose(&self, spec: &ProcessSpec) -> Result<(SeenCall, Replier), ProcessError> {
        let mut state = locked(&self.state);
        let call = SeenCall::of(spec);
        state.seen.push(call.clone());
        let matching: Vec<usize> = state
            .expectations
            .iter()
            .enumerate()
            .filter(|(_, expectation)| {
                expectation.selector.matches(spec.program)
                    && spec.args.starts_with(&expectation.prefix)
            })
            .map(|(index, _)| index)
            .collect();
        let chosen = matching
            .iter()
            .copied()
            .find(|index| state.expectations[*index].used == 0)
            .or_else(|| matching.last().copied());
        let Some(index) = chosen else {
            return Err(ProcessError::Spawn {
                program: spec.program,
                message: format!(
                    "no scripted reply for {} {}",
                    spec.program.binary(),
                    spec.args.join(" ")
                ),
            });
        };
        let expectation = &mut state.expectations[index];
        expectation.used += 1;
        Ok((call, expectation.replier.clone()))
    }

    async fn apply(
        &self,
        spec: &ProcessSpec,
        effects: &[ProcessEffect],
    ) -> Result<(), ProcessError> {
        let fs = StdFileSystem::default();
        let fail = |message: String| ProcessError::Spawn {
            program: spec.program,
            message,
        };
        for effect in effects {
            match effect {
                ProcessEffect::WriteArgFile { prefix, bytes } => {
                    let target = spec
                        .args
                        .iter()
                        .find_map(|arg| arg.strip_prefix(prefix.as_str()))
                        .ok_or_else(|| fail(format!("no argument starts with {prefix}")))?;
                    let path = HostPath::parse(Path::new(target))
                        .map_err(|error| fail(error.to_string()))?;
                    write_atomic(&fs, &path, bytes, FileMode(0o644))
                        .await
                        .map_err(|error| fail(error.to_string()))?;
                }
                ProcessEffect::CaptureContext => {
                    let target = spec
                        .args
                        .last()
                        .ok_or_else(|| fail("no context argument".to_owned()))?;
                    let root = HostPath::parse(Path::new(target))
                        .map_err(|error| fail(error.to_string()))?;
                    let files = snapshot(&fs, &root)
                        .await
                        .map_err(|error| fail(error.to_string()))?;
                    locked(&self.state)
                        .captured
                        .push(CapturedContext { path: root, files });
                }
            }
        }
        Ok(())
    }

    async fn play(
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
        let (call, replier) = self.choose(spec)?;
        let reply = match replier {
            Replier::Fixed(reply) => reply,
            Replier::Computed(compute) => compute(&call),
        };
        self.apply(spec, &reply.effects).await?;
        match reply.body {
            ReplyBody::Output {
                status,
                stdout,
                stderr,
            } => {
                let output = ProcessOutput {
                    status,
                    stdout,
                    stderr,
                };
                emit_output(lines, &output).await;
                Ok(if lines.is_some() {
                    tailed(output)
                } else {
                    output
                })
            }
            ReplyBody::Lines { status, lines: all } => {
                emit(lines, &all).await;
                let output = collect(status, &all);
                Ok(if lines.is_some() {
                    tailed(output)
                } else {
                    output
                })
            }
            ReplyBody::UntilCancelled { lines: all } => {
                emit(lines, &all).await;
                cancel.cancelled().await;
                Err(ProcessError::Cancelled {
                    program: spec.program,
                    group: None,
                })
            }
            ReplyBody::Timeout => Err(ProcessError::Timeout {
                program: spec.program,
                after: spec.timeout,
                group: None,
            }),
            ReplyBody::Cancelled => Err(ProcessError::Cancelled {
                program: spec.program,
                group: None,
            }),
            ReplyBody::Spawn { message } => Err(ProcessError::Spawn {
                program: spec.program,
                message,
            }),
        }
    }
}

async fn read_link(fs: &StdFileSystem, root: &HostPath, relative: &str) -> Result<String, FsError> {
    let invalid = |message: String| FsError::Io {
        path: root.clone(),
        kind: std::io::ErrorKind::InvalidInput,
        message,
    };
    let file = RepoPath::parse(relative).map_err(|error| invalid(error.to_string()))?;
    fs.read_link(root, &file)
        .await?
        .map_err(|problem| invalid(format!("{relative}: {problem:?}")))
}

async fn snapshot(
    fs: &StdFileSystem,
    root: &HostPath,
) -> Result<BTreeMap<String, CapturedFile>, FsError> {
    let mut files = BTreeMap::new();
    let mut pending = vec![(root.clone(), String::new())];
    while let Some((dir, prefix)) = pending.pop() {
        for entry in fs.list(&dir).await? {
            let path =
                HostPath::parse(&dir.as_path().join(&entry.name)).map_err(|error| FsError::Io {
                    path: dir.clone(),
                    kind: std::io::ErrorKind::InvalidInput,
                    message: error.to_string(),
                })?;
            let relative = format!("{prefix}{}", entry.name);
            match entry.kind {
                FileKind::Dir => pending.push((path, format!("{relative}/"))),
                FileKind::File => {
                    let bytes = fs.read(&path).await?.unwrap_or_default();
                    let mode = fs.mode(&path).await?.unwrap_or(FileMode(0));
                    files.insert(relative, CapturedFile::File { bytes, mode });
                }
                FileKind::Symlink => {
                    let target = read_link(fs, root, &relative).await?;
                    files.insert(relative, CapturedFile::Link { target });
                }
                FileKind::Socket => {}
            }
        }
    }
    Ok(files)
}

fn tailed(mut output: ProcessOutput) -> ProcessOutput {
    for bytes in [&mut output.stdout, &mut output.stderr] {
        let excess = bytes.len().saturating_sub(TAIL_LIMIT);
        bytes.drain(..excess);
    }
    output
}

fn collect(status: ProcessStatus, all: &[(OutputStream, String)]) -> ProcessOutput {
    let mut output = ProcessOutput {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    for (stream, text) in all {
        let target = match stream {
            OutputStream::Stdout => &mut output.stdout,
            OutputStream::Stderr => &mut output.stderr,
        };
        target.extend_from_slice(text.as_bytes());
        target.push(b'\n');
    }
    output
}

async fn emit(sink: Option<&LineSink>, all: &[(OutputStream, String)]) {
    let Some(sink) = sink else {
        return;
    };
    for (stream, text) in all {
        drop(
            sink.send(OutputLine {
                stream: *stream,
                text: text.clone(),
            })
            .await,
        );
    }
}

async fn emit_output(sink: Option<&LineSink>, output: &ProcessOutput) {
    let all: Vec<(OutputStream, String)> = [
        (OutputStream::Stdout, &output.stdout),
        (OutputStream::Stderr, &output.stderr),
    ]
    .into_iter()
    .flat_map(|(stream, bytes)| {
        String::from_utf8_lossy(bytes)
            .lines()
            .map(|line| (stream, line.to_owned()))
            .collect::<Vec<_>>()
    })
    .collect();
    emit(sink, &all).await;
}

#[async_trait]
impl ProcessRunner for ScriptedProcessRunner {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        self.play(spec, lines, cancel).await
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        let output = self.play(spec, None, cancel).await?;
        Ok(SecretOutput {
            status: output.status,
            stdout: SecretBytes::new(output.stdout),
            stderr: output.stderr,
        })
    }
}

#[derive(Debug, Default)]
pub struct TestRunner {
    scripted: ScriptedProcessRunner,
    home: Option<PathBuf>,
    ambient: Mutex<Vec<(String, String)>>,
}

impl TestRunner {
    pub fn new() -> TestRunner {
        TestRunner::default()
    }

    pub fn with_home(home: &HostPath) -> TestRunner {
        TestRunner {
            home: Some(home.as_path().to_path_buf()),
            ..TestRunner::default()
        }
    }

    pub fn set_ambient(&self, name: &str, value: &str) {
        locked(&self.ambient).push((name.to_owned(), value.to_owned()));
    }

    pub fn scripted(&self) -> &ScriptedProcessRunner {
        &self.scripted
    }

    async fn run_git(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        if cancel.is_cancelled() {
            return Err(ProcessError::Cancelled {
                program: Program::Git,
                group: None,
            });
        }
        let mut command = HostProgram::Git.command();
        command.args(&spec.args);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd.as_path());
        }
        if let Some(home) = &self.home {
            command.env("HOME", home);
        }
        for (name, value) in locked(&self.ambient).iter() {
            let removed = spec
                .env_remove_prefixes
                .iter()
                .any(|prefix| !prefix.is_empty() && name.starts_with(prefix.as_str()));
            if !removed {
                command.env(name, value);
            }
        }
        for (name, value) in &spec.env {
            match value {
                EnvValue::Plain(text) => command.env(name, text),
                EnvValue::Secret(secret) => command.env(name, secret.expose()),
            };
        }
        let stdin = match &spec.stdin {
            ProcessStdin::Null => None,
            ProcessStdin::Bytes(bytes) => Some(bytes.expose().to_vec()),
            ProcessStdin::File { path, offset } => Some(
                StdFileSystem::default()
                    .read_at(path, *offset, usize::MAX)
                    .await
                    .map_err(|error| ProcessError::Spawn {
                        program: Program::Git,
                        message: error.to_string(),
                    })?,
            ),
        };
        match run_command(command, stdin, spec.timeout, cancel).await {
            Ok(finished) => Ok(ProcessOutput {
                status: finished.status,
                stdout: finished.stdout,
                stderr: finished.stderr,
            }),
            Err(Failure::Spawn(error)) => Err(ProcessError::Spawn {
                program: Program::Git,
                message: error.to_string(),
            }),
            Err(Failure::Timeout) => Err(ProcessError::Timeout {
                program: Program::Git,
                after: spec.timeout,
                group: None,
            }),
            Err(Failure::Cancelled) => Err(ProcessError::Cancelled {
                program: Program::Git,
                group: None,
            }),
        }
    }
}

#[async_trait]
impl ProcessRunner for TestRunner {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        if spec.program == Program::Git && !self.scripted.expects(spec) {
            let output = self.run_git(spec, cancel).await?;
            emit_output(lines, &output).await;
            return Ok(output);
        }
        self.scripted.run(spec, lines, cancel).await
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        if spec.program == Program::Git && !self.scripted.expects(spec) {
            let output = self.run_git(spec, cancel).await?;
            return Ok(SecretOutput {
                status: output.status,
                stdout: SecretBytes::new(output.stdout),
                stderr: output.stderr,
            });
        }
        self.scripted.run_secret(spec, cancel).await
    }
}
