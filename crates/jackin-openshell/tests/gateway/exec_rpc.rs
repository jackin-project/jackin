// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use openshell_sdk::raw::proto::{
    ExecSandboxEvent, ExecSandboxExit, ExecSandboxInput, ExecSandboxStderr, ExecSandboxStdout,
    SandboxPhase, exec_sandbox_event, exec_sandbox_input,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use super::sandbox_rpc::phase_of;
use super::server::{ExecBackend, ExecCtx, Fake, scope_name};
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

pub(crate) fn host_shell(root: PathBuf) -> ExecBackend {
    host_shell_aborting(root, None)
}

pub(crate) fn host_shell_aborting(root: PathBuf, abort_after_chunks: Option<usize>) -> ExecBackend {
    Arc::new(move |ctx: ExecCtx| {
        let root = root.clone();
        Box::pin(async move { run_host(ctx, root, abort_after_chunks).await })
    })
}

async fn run_host(ctx: ExecCtx, root: PathBuf, abort_after_chunks: Option<usize>) {
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
    let workdir = if start.workdir.is_empty() {
        root.join("sandbox")
    } else {
        PathBuf::from(rewrite(&root, &start.workdir))
    };
    tokio::fs::create_dir_all(&workdir).await.unwrap();
    let mut command = Command::new(&argv[0]);
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
    shared.lock().observed.child_started = true;
    shared.child_started.notify_one();
    let mut stdin = child.stdin.take();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out_task = tokio::spawn(pump_output(stdout, events.clone(), false));
    let err_task = tokio::spawn(pump_output(stderr, events.clone(), true));
    let (abort_tx, abort_rx) = tokio::sync::oneshot::channel::<()>();
    let abort_events = events.clone();
    let feeder = tokio::spawn(async move {
        let mut chunks = 0_usize;
        while let Some(payload) = inputs.recv().await {
            let exec_sandbox_input::Payload::Stdin(bytes) = payload else {
                continue;
            };
            chunks += 1;
            if abort_after_chunks.is_some_and(|limit| chunks > limit) {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                drop(abort_events.send(Err(Status::unavailable("aborted"))).await);
                let _sent = abort_tx.send(());
                return;
            }
            if let Some(pipe) = stdin.as_mut()
                && pipe.write_all(&bytes).await.is_err()
            {
                break;
            }
        }
        drop(stdin);
    });
    tokio::select! {
        status = child.wait() => {
            drop(out_task.await);
            drop(err_task.await);
            let code = status.ok().and_then(|status| status.code()).unwrap_or(-1);
            drop(events.send(Ok(exit_event(code))).await);
        }
        () = events.closed() => {
            drop(child.kill().await);
            shared.lock().observed.child_killed = true;
            shared.child_killed.notify_one();
        }
        Ok(()) = abort_rx => {
            drop(child.kill().await);
            shared.lock().observed.child_killed = true;
            shared.child_killed.notify_one();
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
