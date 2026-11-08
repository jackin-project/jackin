// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use jackin_core::Sha256;
use openshell_sdk::raw::proto::{
    ExecSandboxEvent, ExecSandboxExit, ExecSandboxInput, ExecSandboxStderr, ExecSandboxStdout,
    SandboxPhase, exec_sandbox_event, exec_sandbox_input,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::{Barrier, Notify, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use super::sandbox_rpc::phase_of;
use super::server::{ExecBackend, ExecCtx, Fake, Shared, scope_name};
use super::service::Boxed;

pub(crate) enum Step {
    Out(Vec<u8>),
    Err(Vec<u8>),
    Exit(i32),
}

pub(crate) fn out_event(data: Vec<u8>) -> ExecSandboxEvent {
    ExecSandboxEvent {
        payload: Some(exec_sandbox_event::Payload::Stdout(ExecSandboxStdout {
            data,
        })),
    }
}

pub(crate) fn err_event(data: Vec<u8>) -> ExecSandboxEvent {
    ExecSandboxEvent {
        payload: Some(exec_sandbox_event::Payload::Stderr(ExecSandboxStderr {
            data,
        })),
    }
}

pub(crate) fn exit_event(exit_code: i32) -> ExecSandboxEvent {
    ExecSandboxEvent {
        payload: Some(exec_sandbox_event::Payload::Exit(ExecSandboxExit {
            exit_code,
        })),
    }
}

pub(crate) fn scripted(
    script: impl Fn(&openshell_sdk::raw::proto::ExecSandboxRequest, &[u8]) -> Vec<Step>
    + Send
    + Sync
    + 'static,
) -> ExecBackend {
    let script = Arc::new(script);
    Arc::new(move |mut ctx: ExecCtx| {
        let script = Arc::clone(&script);
        Box::pin(async move {
            let mut stdin = Vec::new();
            while let Some(payload) = ctx.inputs.recv().await {
                if let exec_sandbox_input::Payload::Stdin(bytes) = payload {
                    stdin.extend_from_slice(&bytes);
                }
            }
            for step in script(&ctx.start, &stdin) {
                let event = match step {
                    Step::Out(data) => Ok(out_event(data)),
                    Step::Err(data) => Ok(err_event(data)),
                    Step::Exit(code) => Ok(exit_event(code)),
                };
                if ctx.events.send(event).await.is_err() {
                    return;
                }
            }
        })
    })
}

fn rewrite(root: &Path, argument: &str) -> String {
    if argument == "/sandbox" || argument.starts_with("/sandbox/") {
        format!("{}{argument}", root.display())
    } else {
        argument.to_owned()
    }
}

async fn pump_output(
    mut source: impl tokio::io::AsyncRead + Unpin,
    events: mpsc::Sender<Result<ExecSandboxEvent, Status>>,
    is_err: bool,
) {
    let mut buffer = vec![0_u8; 8192];
    while let Ok(count) = source.read(&mut buffer).await {
        if count == 0 {
            break;
        }
        let data = buffer[..count].to_vec();
        let event = if is_err {
            err_event(data)
        } else {
            out_event(data)
        };
        if events.send(Ok(event)).await.is_err() {
            break;
        }
    }
}

const GATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const HOST_SHELL: &str = "/bin/sh";
const REMOTE_WRITER: &str = "/usr/local/lib/jackin/write-file";
const MISSING_PROGRAM: i32 = 127;

#[derive(Clone, Copy)]
pub(crate) enum WriterMutation {
    TargetMode,
    ParentMode,
    OrphanDataStage,
    OrphanIdentityStage,
    UnrelatedHiddenEntry,
    ForeignIdentity,
    LockMode,
    ExtraNamespace,
    NamespaceSymlink,
    DanglingNamespaceSymlink,
    MissingIdentity,
    MissingLock,
}

#[derive(Clone)]
enum Hook {
    Plain,
    GatedAbortAfter(usize, Arc<Notify>, Arc<Barrier>),
    GateAfter(usize, Arc<Barrier>),
    GateFirstAfter(
        usize,
        Arc<AtomicBool>,
        Arc<Notify>,
        Arc<Barrier>,
        Arc<AtomicUsize>,
        Arc<Notify>,
    ),
    ShortInputAfter(usize),
    ExtraInput,
    WriterMutationAfter(usize, WriterMutation, Arc<AtomicUsize>),
}

const SANDBOX_TOOLS: [&str; 2] = ["/usr/bin/stat", "/usr/bin/sha256sum"];

fn sandbox_tool(root: &Path, command: &[String]) -> Option<Vec<Step>> {
    let program = command.first()?.as_str();
    let target = |name: &str| PathBuf::from(rewrite(root, name));
    match program {
        "/usr/bin/stat" => {
            let file = target(command.last()?);
            Some(match std::fs::metadata(&file) {
                Ok(meta) => {
                    let kind = if meta.is_dir() {
                        "directory"
                    } else if meta.len() == 0 {
                        "regular empty file"
                    } else {
                        "regular file"
                    };
                    vec![
                        Step::Out(format!("{kind} {}\n", meta.len()).into_bytes()),
                        Step::Exit(0),
                    ]
                }
                Err(error) => vec![Step::Err(error.to_string().into_bytes()), Step::Exit(1)],
            })
        }
        "/usr/bin/sha256sum" => {
            let name = command.get(1)?;
            Some(match std::fs::read(target(name)) {
                Ok(bytes) => vec![
                    Step::Out(format!("{}  {name}\n", Sha256::of(&bytes).as_str()).into_bytes()),
                    Step::Exit(0),
                ],
                Err(error) => vec![Step::Err(error.to_string().into_bytes()), Step::Exit(1)],
            })
        }
        _ => None,
    }
}

pub(crate) async fn host_tools(root: PathBuf) -> ExecBackend {
    let shell = host_shell(root.clone()).await;
    let tools = scripted(move |start, _| {
        sandbox_tool(&root, &start.command).unwrap_or_else(|| vec![Step::Exit(MISSING_PROGRAM)])
    });
    Arc::new(move |ctx: ExecCtx| {
        let native = ctx
            .start
            .command
            .first()
            .is_some_and(|program| SANDBOX_TOOLS.contains(&program.as_str()));
        if native { tools(ctx) } else { shell(ctx) }
    })
}

pub(crate) async fn host_shell(root: PathBuf) -> ExecBackend {
    host_shell_with(root, Hook::Plain).await
}

pub(crate) async fn host_shell_writer_mutating(
    root: PathBuf,
    mutation: WriterMutation,
) -> ExecBackend {
    host_shell_with(
        root,
        Hook::WriterMutationAfter(2, mutation, Arc::new(AtomicUsize::new(0))),
    )
    .await
}

pub(crate) async fn host_shell_gated(
    root: PathBuf,
    after_chunks: usize,
    rendezvous: Arc<Barrier>,
) -> ExecBackend {
    host_shell_with(root, Hook::GateAfter(after_chunks, rendezvous)).await
}

pub(crate) async fn host_shell_gated_aborting(
    root: PathBuf,
    abort_after_chunks: usize,
    reached: Arc<Notify>,
    release: Arc<Barrier>,
) -> ExecBackend {
    host_shell_with(
        root,
        Hook::GatedAbortAfter(abort_after_chunks, reached, release),
    )
    .await
}

pub(crate) async fn host_shell_gated_first_after(
    root: PathBuf,
    chunks: usize,
    reached: Arc<Notify>,
    release: Arc<Barrier>,
    second_started: Arc<Notify>,
) -> ExecBackend {
    host_shell_with(
        root,
        Hook::GateFirstAfter(
            chunks,
            Arc::new(AtomicBool::new(false)),
            reached,
            release,
            Arc::new(AtomicUsize::new(0)),
            second_started,
        ),
    )
    .await
}

pub(crate) async fn host_shell_short_input_after(root: PathBuf, chunks: usize) -> ExecBackend {
    host_shell_with(root, Hook::ShortInputAfter(chunks)).await
}

pub(crate) async fn host_shell_extra_input(root: PathBuf) -> ExecBackend {
    host_shell_with(root, Hook::ExtraInput).await
}

async fn host_shell_with(root: PathBuf, hook: Hook) -> ExecBackend {
    let writer = root.join("jackin-write-file");
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../jackin-image/images/construct/write-file.c");
    let sandbox_root = root.join("sandbox");
    let compile = Command::new("cc")
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(format!(
            "-DJACKIN_SANDBOX_ROOT=\"{}\"",
            sandbox_root.display()
        ))
        .arg(source)
        .arg("-o")
        .arg(&writer)
        .output()
        .await
        .expect("the native C compiler must run the gateway writer test fixture");
    assert!(
        compile.status.success(),
        "native gateway writer compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    Arc::new(move |ctx: ExecCtx| {
        let root = root.clone();
        let writer = writer.clone();
        let hook = hook.clone();
        Box::pin(async move { run_host(ctx, root, writer, hook).await })
    })
}

fn mutate_writer_state(root: &Path, mutation: WriterMutation) {
    use std::os::unix::fs::PermissionsExt;

    let parent = root.join("sandbox/.contract");
    let namespace = std::fs::read_dir(&parent)
        .expect("the contract writer parent must exist")
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".jackin-write-")
        })
        .expect("the contract writer namespace must exist")
        .path();
    let target = parent.join("secret.json");
    let set_mode = |path: &Path, mode| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .expect("the contract fixture mode mutation must succeed");
    };
    match mutation {
        WriterMutation::TargetMode => set_mode(&target, 0o644),
        WriterMutation::ParentMode => set_mode(&parent, 0o755),
        WriterMutation::OrphanDataStage => {
            let stage = namespace.join("data.ffffffffffffffff");
            std::fs::write(&stage, b"orphan data stage")
                .expect("the contract fixture data stage must be created");
            set_mode(&stage, 0o600);
        }
        WriterMutation::OrphanIdentityStage => {
            let stage = namespace.join("identity.ffffffffffffffff");
            std::fs::write(&stage, b"orphan identity stage")
                .expect("the contract fixture identity stage must be created");
            set_mode(&stage, 0o600);
        }
        WriterMutation::UnrelatedHiddenEntry => {
            std::fs::write(parent.join(".foreign"), b"foreign")
                .expect("the unrelated hidden entry must be created");
        }
        WriterMutation::ForeignIdentity => {
            std::fs::write(namespace.join("identity"), b"/sandbox/.contract/other.json")
                .expect("the foreign writer identity must be written");
        }
        WriterMutation::LockMode => set_mode(&namespace.join("lock"), 0o644),
        WriterMutation::ExtraNamespace => {
            let extra = parent.join(".jackin-write-0000000000000000");
            std::fs::create_dir(&extra).expect("the extra writer namespace must be created");
            set_mode(&extra, 0o700);
        }
        WriterMutation::NamespaceSymlink => {
            let real = root.join("retained-writer-namespace");
            std::fs::rename(&namespace, &real)
                .expect("the real writer namespace must be moved outside the parent");
            std::os::unix::fs::symlink(&real, &namespace)
                .expect("the writer namespace symlink must be created");
        }
        WriterMutation::DanglingNamespaceSymlink => {
            std::fs::remove_dir_all(&namespace).expect("the real writer namespace must be removed");
            std::os::unix::fs::symlink(root.join("missing-writer-namespace"), &namespace)
                .expect("the dangling writer namespace symlink must be created");
        }
        WriterMutation::MissingIdentity => {
            std::fs::remove_file(namespace.join("identity"))
                .expect("the writer identity must be removed");
        }
        WriterMutation::MissingLock => {
            std::fs::remove_file(namespace.join("lock")).expect("the writer lock must be removed");
        }
    }
}

fn notify_child_killed(shared: &Shared, sender: Option<tokio::sync::oneshot::Sender<()>>) {
    if let Some(sender) = sender {
        match sender.send(()) {
            Ok(()) | Err(()) => {}
        }
    }
    shared.lock().observed.child_killed = true;
    shared.child_killed.notify_one();
}

async fn run_host(ctx: ExecCtx, root: PathBuf, writer: PathBuf, hook: Hook) {
    let ExecCtx {
        start,
        mut inputs,
        events,
        shared,
    } = ctx;
    let argv: Vec<String> = start
        .command
        .iter()
        .map(|part| rewrite(&root, part))
        .collect();
    let is_writer = start
        .command
        .first()
        .is_some_and(|program| program == REMOTE_WRITER);
    let is_contract_writer = is_writer
        && start
            .command
            .get(1)
            .is_some_and(|path| path == ".contract/secret.json");
    let host_program = if is_writer {
        writer.as_os_str()
    } else {
        std::ffi::OsStr::new(HOST_SHELL)
    };
    let workdir = if start.workdir.is_empty() {
        root.join("sandbox")
    } else {
        PathBuf::from(rewrite(&root, &start.workdir))
    };
    if !is_writer && argv.first().map(String::as_str) != Some(HOST_SHELL) {
        drop(events.send(Ok(exit_event(MISSING_PROGRAM))).await);
        return;
    }
    tokio::fs::create_dir_all(&workdir).await.unwrap();
    let mut command = Command::new(host_program);
    command
        .args(&argv[1..])
        .current_dir(&workdir)
        .envs(start.environment.iter())
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            drop(events.send(Err(Status::internal(error.to_string()))).await);
            return;
        }
    };
    if let Hook::GateFirstAfter(_, _, _, _, starts, second_started) = &hook
        && starts.fetch_add(1, Ordering::AcqRel) == 1
    {
        second_started.notify_one();
    }
    shared.lock().observed.child_started = true;
    shared.child_started.notify_one();
    let mut stdin = child.stdin.take();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out_task = tokio::spawn(pump_output(stdout, events.clone(), false));
    let err_task = tokio::spawn(pump_output(stderr, events.clone(), true));
    let (abort_tx, abort_rx) = tokio::sync::oneshot::channel::<()>();
    let (child_killed_tx, child_killed_rx) = tokio::sync::oneshot::channel::<()>();
    let mut child_killed_tx = Some(child_killed_tx);
    let abort_events = events.clone();
    let feeder_hook = hook.clone();
    let feeder = tokio::spawn(async move {
        let child_killed = child_killed_rx;
        let mut chunks = 0_usize;
        while let Some(payload) = inputs.recv().await {
            let exec_sandbox_input::Payload::Stdin(bytes) = payload else {
                continue;
            };
            chunks += 1;
            if let Hook::GatedAbortAfter(limit, reached, release) = &feeder_hook
                && chunks > *limit
            {
                reached.notify_one();
                drop(tokio::time::timeout(GATE_TIMEOUT, release.wait()).await);
                drop(abort_events.send(Err(Status::unavailable("aborted"))).await);
                let _sent = abort_tx.send(());
                drop(child_killed.await);
                return;
            }
            if let Some(pipe) = stdin.as_mut()
                && pipe.write_all(&bytes).await.is_err()
            {
                break;
            }
            if let Hook::GateAfter(limit, rendezvous) = &feeder_hook
                && chunks == *limit
            {
                drop(tokio::time::timeout(GATE_TIMEOUT, rendezvous.wait()).await);
            }
            if let Hook::GateFirstAfter(limit, claimed, reached, release, _, _) = &feeder_hook
                && chunks == *limit
                && claimed
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                reached.notify_one();
                drop(tokio::time::timeout(GATE_TIMEOUT, release.wait()).await);
            }
            if let Hook::ShortInputAfter(limit) = &feeder_hook
                && chunks == *limit
            {
                drop(stdin.take());
                return;
            }
        }
        if matches!(feeder_hook, Hook::ExtraInput)
            && let Some(pipe) = stdin.as_mut()
        {
            drop(pipe.write_all(&[0xff]).await);
        }
        drop(stdin);
    });
    tokio::select! {
        status = child.wait() => {
            drop(out_task.await);
            drop(err_task.await);
            let code = status.ok().and_then(|status| status.code()).unwrap_or(-1);
            if code == 0
                && is_contract_writer
                && let Hook::WriterMutationAfter(after, mutation, writers) = &hook
                && writers.fetch_add(1, Ordering::AcqRel) + 1 == *after
            {
                mutate_writer_state(&root, *mutation);
            }
            drop(events.send(Ok(exit_event(code))).await);
        }
        () = events.closed() => {
            drop(child.kill().await);
            notify_child_killed(&shared, child_killed_tx.take());
        }
        Ok(()) = abort_rx => {
            drop(child.kill().await);
            notify_child_killed(&shared, child_killed_tx.take());
        }
    }
    feeder.abort();
}

impl Fake {
    pub(crate) async fn exec_sandbox_interactive_impl(
        &self,
        request: Request<Streaming<ExecSandboxInput>>,
    ) -> Result<Response<Boxed<ExecSandboxEvent>>, Status> {
        self.enter("exec_sandbox_interactive").await?;
        let mut inbound = request.into_inner();
        let first = inbound
            .message()
            .await?
            .and_then(|message| message.payload)
            .ok_or_else(|| Status::invalid_argument("the first message must be start"))?;
        let exec_sandbox_input::Payload::Start(start) = first else {
            return Err(Status::invalid_argument("the first message must be start"));
        };
        if start.command.is_empty() {
            return Err(Status::invalid_argument("command must not be empty"));
        }
        let key = (
            scope_name(start.workspace_scope.as_ref())?,
            start.sandbox.clone(),
        );
        let backend = {
            let mut state = self.shared.lock();
            let sandbox = state
                .sandboxes
                .get(&key)
                .ok_or_else(|| Status::not_found("sandbox not found"))?;
            if phase_of(sandbox) != SandboxPhase::Ready as i32 {
                return Err(Status::failed_precondition("sandbox is not ready"));
            }
            state.exec_starts.push(start.clone());
            state.exec_streams += 1;
            state
                .exec
                .clone()
                .ok_or_else(|| Status::unimplemented("no exec backend"))?
        };
        let (inputs_tx, inputs) = mpsc::unbounded_channel();
        let shared = Arc::clone(&self.shared);
        tokio::spawn(async move {
            while let Ok(Some(message)) = inbound.message().await {
                if let Some(payload) = message.payload
                    && inputs_tx.send(payload).is_err()
                {
                    break;
                }
            }
            shared.lock().observed.request_stream_ended = true;
            shared.stream_ended.notify_one();
        });
        let (events, receiver) = mpsc::channel(16);
        tokio::spawn(backend(ExecCtx {
            start,
            inputs,
            events,
            shared: Arc::clone(&self.shared),
        }));
        Ok(Response::new(Box::pin(ReceiverStream::new(receiver))))
    }
}
