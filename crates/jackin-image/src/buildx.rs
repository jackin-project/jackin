// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    BuilderName, DockerSubcommand, FileLock, FileSystem, FsError, HostPath, ImageId, ImageTag,
    OutputLine, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, ProcessStatus,
    ProcessStdin, Program, SecretBytes, Sha256, TerminalText,
};
use rustix::io::Errno;
use rustix::process::{Pid, test_kill_process};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep};
use tokio_util::sync::CancellationToken;

use crate::context::stage;
use crate::engine::{EngineError, ImageEngine};
use crate::ensure::{ImageEvent, ProgressSink};
use crate::plan::{BuildStage, PlannedBuild};

const BUILD_TIMEOUT: Duration = Duration::from_secs(3600);
const QUERY_TIMEOUT: Duration = Duration::from_secs(60);
const TAIL_LINES: usize = 40;
const LINE_BUFFER: usize = 64;
const LOCK_POLL: Duration = Duration::from_millis(100);
const LOCKS_DIR: &str = "locks";

static CALLS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct BuildxEngine {
    runner: Arc<dyn ProcessRunner>,
    fs: Arc<dyn FileSystem>,
    work_dir: HostPath,
}

fn invalid_path(path: &HostPath) -> EngineError {
    EngineError::Fs(FsError::Io {
        path: path.clone(),
        kind: ErrorKind::InvalidInput,
        message: "the path is not valid UTF-8".to_owned(),
    })
}

fn path_text(path: &HostPath) -> Result<String, EngineError> {
    path.as_path()
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid_path(path))
}

fn child(parent: &HostPath, name: &str) -> Result<HostPath, EngineError> {
    HostPath::parse(&parent.as_path().join(name)).map_err(|_| invalid_path(parent))
}

fn from_process(error: ProcessError) -> EngineError {
    match error {
        ProcessError::Cancelled { group, .. } => EngineError::Cancelled { group },
        other => EngineError::Process(other),
    }
}

fn spec(
    sub: DockerSubcommand,
    args: Vec<String>,
    stdin: ProcessStdin,
    timeout: Duration,
) -> ProcessSpec {
    ProcessSpec {
        program: Program::Docker(sub),
        args,
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: Vec::new(),
        stdin,
        timeout,
    }
}

fn combined(output: &ProcessOutput) -> TerminalText {
    let mut bytes = output.stderr.clone();
    bytes.extend_from_slice(&output.stdout);
    TerminalText::sanitize(&bytes)
}

fn mentions(bytes: &[u8], needle: &str) -> bool {
    String::from_utf8_lossy(bytes).contains(needle)
}

async fn forward(
    rx: &mut mpsc::Receiver<OutputLine>,
    stage: BuildStage,
    progress: &ProgressSink,
    cancel: &CancellationToken,
) -> VecDeque<String> {
    let mut tail = VecDeque::with_capacity(TAIL_LINES + 1);
    while let Some(line) = rx.recv().await {
        let event = ImageEvent::Line {
            stage,
            text: TerminalText::sanitize(line.text.as_bytes()),
        };
        if tail.len() == TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line.text);
        tokio::select! {
            biased;
            () = cancel.cancelled() => {}
            sent = progress.send(event) => drop(sent),
        }
    }
    tail
}

struct CallDir(HostPath);

impl Drop for CallDir {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(self.0.as_path()));
    }
}

fn call_owner(name: &str) -> Option<Pid> {
    let (pid, counter) = name.split_once('-')?;
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(pid) || !digits(counter) {
        return None;
    }
    Pid::from_raw(pid.parse().ok()?)
}

fn process_alive(pid: Pid) -> bool {
    match test_kill_process(pid) {
        Ok(()) => true,
        Err(error) => error != Errno::SRCH,
    }
}

fn sweep_stale_calls(work_dir: &HostPath) {
    let Ok(entries) = std::fs::read_dir(work_dir.as_path()) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(owner) = name.to_str().and_then(call_owner) else {
            continue;
        };
        if process_alive(owner) {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            drop(std::fs::remove_file(entry.path()));
        } else if kind.is_dir() {
            drop(std::fs::remove_dir_all(entry.path()));
        }
    }
}

impl BuildxEngine {
    pub fn new(
        runner: Arc<dyn ProcessRunner>,
        fs: Arc<dyn FileSystem>,
        work_dir: HostPath,
    ) -> BuildxEngine {
        sweep_stale_calls(&work_dir);
        BuildxEngine {
            runner,
            fs,
            work_dir,
        }
    }

    async fn query(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, EngineError> {
        self.runner
            .run(spec, None, cancel)
            .await
            .map_err(from_process)
    }

    async fn read_id(&self, iid: &HostPath, tag: &ImageTag) -> Result<ImageId, EngineError> {
        let unreadable = |bytes: &[u8]| EngineError::DigestUnreadable {
            tag: tag.clone(),
            output: TerminalText::sanitize(bytes),
        };
        let Some(bytes) = self.fs.read(iid).await? else {
            return Err(unreadable(b"the build wrote no image ID file"));
        };
        ImageId::parse(String::from_utf8_lossy(&bytes).trim()).map_err(|_| unreadable(&bytes))
    }

    async fn build_in(
        &self,
        call: &HostPath,
        build: &PlannedBuild,
        progress: &ProgressSink,
        cancel: &CancellationToken,
    ) -> Result<ImageId, EngineError> {
        self.fs.remove_dir_all(call).await?;
        let context = child(call, "context")?;
        let iid = child(call, "iid")?;
        self.fs.create_private_dir_all(&context).await?;
        stage(self.fs.as_ref(), &build.context, &context).await?;
        let mut args = build.args.clone();
        args.push(format!("--iidfile={}", path_text(&iid)?));
        args.push(path_text(&context)?);
        let stdin = build.stdin.as_ref().map_or(ProcessStdin::Null, |bytes| {
            ProcessStdin::Bytes(SecretBytes::new(bytes.clone()))
        });
        let spec = spec(DockerSubcommand::BuildxBuild, args, stdin, BUILD_TIMEOUT);
        let (tx, mut rx) = mpsc::channel::<OutputLine>(LINE_BUFFER);
        let run = async {
            let result = self.runner.run(&spec, Some(&tx), cancel).await;
            drop(tx);
            result
        };
        let (result, tail) = tokio::join!(run, forward(&mut rx, build.stage, progress, cancel));
        let output = result.map_err(from_process)?;
        if output.status != ProcessStatus::Exited(0) {
            let lines: Vec<&str> = tail.iter().map(String::as_str).collect();
            return Err(EngineError::BuildFailed {
                stage: build.stage,
                status: output.status,
                tail: TerminalText::sanitize(lines.join("\n").as_bytes()),
            });
        }
        self.read_id(&iid, &build.tag).await
    }
}

#[async_trait]
impl ImageEngine for BuildxEngine {
    async fn find(
        &self,
        tag: &ImageTag,
        cancel: &CancellationToken,
    ) -> Result<Option<ImageId>, EngineError> {
        let args = vec!["--format".to_owned(), "{{.Id}}".to_owned(), tag.to_string()];
        let spec = spec(
            DockerSubcommand::ImageInspect,
            args,
            ProcessStdin::Null,
            QUERY_TIMEOUT,
        );
        let output = self.query(&spec, cancel).await?;
        let unreadable = || EngineError::DigestUnreadable {
            tag: tag.clone(),
            output: combined(&output),
        };
        match output.status {
            ProcessStatus::Exited(0) => {
                let text = String::from_utf8_lossy(&output.stdout);
                ImageId::parse(text.trim())
                    .map(Some)
                    .map_err(|_| unreadable())
            }
            ProcessStatus::Exited(1) if mentions(&output.stderr, "No such image") => Ok(None),
            _ => Err(unreadable()),
        }
    }

    async fn check_builder(
        &self,
        builder: &BuilderName,
        cancel: &CancellationToken,
    ) -> Result<(), EngineError> {
        let spec = spec(
            DockerSubcommand::BuildxInspect,
            vec![builder.to_string()],
            ProcessStdin::Null,
            QUERY_TIMEOUT,
        );
        let output = self.query(&spec, cancel).await?;
        let unreadable = || EngineError::BuilderUnreadable {
            builder: builder.clone(),
            status: output.status,
            output: combined(&output),
        };
        if output.status != ProcessStatus::Exited(0) {
            return Err(if mentions(&output.stderr, "no builder") {
                EngineError::BuilderMissing {
                    builder: builder.clone(),
                }
            } else {
                unreadable()
            });
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let driver = stdout
            .lines()
            .find_map(|line| line.strip_prefix("Driver:"))
            .map(str::trim);
        match driver {
            Some("docker") => Ok(()),
            Some(other) => Err(EngineError::BuilderDriver {
                builder: builder.clone(),
                driver: other.to_owned(),
            }),
            None => Err(unreadable()),
        }
    }

    async fn build(
        &self,
        build: &PlannedBuild,
        progress: &ProgressSink,
        cancel: &CancellationToken,
    ) -> Result<ImageId, EngineError> {
        let name = format!(
            "{}-{}",
            std::process::id(),
            CALLS.fetch_add(1, Ordering::Relaxed)
        );
        let call = child(&self.work_dir, &name)?;
        let _cleanup = CallDir(call.clone());
        self.build_in(&call, build, progress, cancel).await
    }

    async fn hold(
        &self,
        key: &ImageTag,
        cancel: &CancellationToken,
    ) -> Result<Option<FileLock>, EngineError> {
        let dir = child(&self.work_dir, LOCKS_DIR)?;
        self.fs.create_dir_all(&dir).await?;
        let digest = Sha256::of(key.to_string().as_bytes());
        let file = child(&dir, &format!("{}.lock", digest.as_str()))?;
        let started = Instant::now();
        loop {
            match self.fs.lock(&file).await {
                Ok(lock) => return Ok(Some(lock)),
                Err(FsError::LockHeld(_)) => {}
                Err(other) => return Err(other.into()),
            }
            if started.elapsed() >= BUILD_TIMEOUT {
                return Err(EngineError::LockTimeout { key: key.clone() });
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(EngineError::Cancelled { group: None }),
                () = sleep(LOCK_POLL) => {}
            }
        }
    }
}
