// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::sync::Arc;

use openshell_sdk::raw::proto::{
    ExecSandboxEvent, ExecSandboxInput, ExecSandboxWindowResize, exec_sandbox_event,
    exec_sandbox_input,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Streaming;

use super::Inner;
use super::program::exec_start;
use crate::error::{GatewayError, GatewayOp, Missing};
use crate::terminal::{
    EndReason, TermSize, TerminalCommand, TerminalDriver, TerminalEvent, TerminalKind,
    TerminalRequest, TerminalSession,
};

pub(super) const SESSION_CAPACITY: usize = 64;
const INPUT_CHUNK: usize = 4096;
const REQUEST_CAPACITY: usize = 1024;
const SHELL_PROGRAM: &str = "bash";

fn resize_input(size: TermSize) -> ExecSandboxInput {
    ExecSandboxInput {
        payload: Some(exec_sandbox_input::Payload::Resize(
            ExecSandboxWindowResize {
                cols: u32::from(size.cols.get()),
                rows: u32::from(size.rows.get()),
            },
        )),
    }
}

impl Inner {
    pub(crate) async fn open_terminal(
        self: &Arc<Self>,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError> {
        match &request.kind {
            TerminalKind::Shell { .. } => self.open_shell(request).await,
            TerminalKind::MainProcess { .. } => self.open_main(request).await,
        }
    }

    async fn open_shell(&self, request: &TerminalRequest) -> Result<TerminalSession, GatewayError> {
        let TerminalKind::Shell { workdir, env } = &request.kind else {
            return Err(GatewayError::Protocol {
                message: "not a shell terminal".to_owned(),
            });
        };
        let environment = env
            .iter()
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect();
        let mut start = exec_start(
            self.scope(),
            &request.sandbox,
            vec![SHELL_PROGRAM.to_owned()],
            Some(workdir),
            environment,
        );
        start.tty = true;
        start.cols = u32::from(request.size.cols.get());
        start.rows = u32::from(request.size.rows.get());
        let (input_tx, input_rx) = mpsc::channel(REQUEST_CAPACITY);
        input_tx
            .send(ExecSandboxInput {
                payload: Some(exec_sandbox_input::Payload::Start(start)),
            })
            .await
            .map_err(|_| GatewayError::Protocol {
                message: "the exec request channel closed".to_owned(),
            })?;
        let mut grpc = self.grpc();
        let opened = tokio::time::timeout(
            self.call_timeout,
            grpc.exec_sandbox_interactive(ReceiverStream::new(input_rx)),
        )
        .await;
        let stream = match opened {
            Err(_) => {
                return Err(GatewayError::Timeout {
                    op: GatewayOp::Terminal,
                    after: self.call_timeout,
                    group: None,
                });
            }
            Ok(Err(status)) => {
                let missing = Some(Missing::Sandbox(request.sandbox.clone()));
                let error = self.map_status(GatewayOp::Terminal, missing, &status);
                return Err(self.refine_not_ready(&request.sandbox, error).await);
            }
            Ok(Ok(response)) => response.into_inner(),
        };
        let (session, driver) = TerminalSession::pair(SESSION_CAPACITY, request.kind.clone());
        tokio::spawn(shell_task(stream, input_tx, driver));
        Ok(session)
    }
}

async fn shell_task(
    mut stream: Streaming<ExecSandboxEvent>,
    input: mpsc::Sender<ExecSandboxInput>,
    driver: TerminalDriver,
) {
    let TerminalDriver {
        mut commands,
        events,
    } = driver;
    let mut queue: VecDeque<TerminalEvent> = VecDeque::from([TerminalEvent::Attached]);
    let mut exited = false;
    let mut ended = false;
    loop {
        if ended && queue.is_empty() {
            break;
        }
        tokio::select! {
            permit = events.reserve(), if !queue.is_empty() => {
                let Ok(permit) = permit else { break };
                if let Some(event) = queue.pop_front() {
                    permit.send(event);
                }
            }
            command = commands.recv(), if !ended => match command {
                None => break,
                Some(TerminalCommand::Input(bytes)) => {
                    for chunk in bytes.chunks(INPUT_CHUNK) {
                        let message = ExecSandboxInput {
                            payload: Some(exec_sandbox_input::Payload::Stdin(chunk.to_vec())),
                        };
                        if input.send(message).await.is_err() {
                            break;
                        }
                    }
                }
                Some(TerminalCommand::Resize(size)) => {
                    if input.send(resize_input(size)).await.is_err() {
                        break;
                    }
                }
                Some(TerminalCommand::Detach) => {
                    queue.push_back(TerminalEvent::Ended(EndReason::Detached));
                    ended = true;
                }
            },
            item = stream.message(), if queue.is_empty() && !ended => match item {
                Ok(Some(event)) => match event.payload {
                    Some(
                        exec_sandbox_event::Payload::Stdout(chunk),
                    ) => queue.push_back(TerminalEvent::Output(chunk.data)),
                    Some(
                        exec_sandbox_event::Payload::Stderr(chunk),
                    ) => queue.push_back(TerminalEvent::Output(chunk.data)),
                    Some(exec_sandbox_event::Payload::Exit(event)) => {
                        exited = true;
                        queue.push_back(TerminalEvent::Exited(Some(event.exit_code)));
                    }
                    None => {}
                },
                Ok(None) => {
                    queue.push_back(TerminalEvent::Ended(if exited {
                        EndReason::StreamClosed
                    } else {
                        EndReason::Failed("the stream ended without an exit event".to_owned())
                    }));
                    ended = true;
                }
                Err(status) => {
                    queue.push_back(TerminalEvent::Ended(EndReason::Failed(
                        status.message().to_owned(),
                    )));
                    ended = true;
                }
            },
        }
    }
}
