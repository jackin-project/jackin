// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use jackin_core::{SandboxName, TerminalText};
use openshell_sdk::raw::proto::{
    CreateSshSessionRequest, CreateSshSessionResponse, RevokeSshSessionRequest, SshRelayTarget,
    TcpForwardFrame, TcpForwardInit, tcp_forward_frame, tcp_forward_init,
};
use russh::client::{self, Handle, Msg};
use russh::keys::HashAlg;
use russh::{Channel, ChannelMsg, Disconnect};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use super::Inner;
use super::shell::SESSION_CAPACITY;
use crate::error::{GatewayError, GatewayOp, Missing};
use crate::terminal::{
    EndReason, InputLease, InputMode, RepaintPolicy, TermSize, TerminalCommand, TerminalDriver,
    TerminalEvent, TerminalKind, TerminalRequest, TerminalSession,
};

const SSH_USER: &str = "sandbox";
const SSH_TERM: &str = "xterm-256color";
const SUBSYSTEM: &str = "openshell-main";
const READ_ONLY_ENV: &str = "OPENSHELL_MAIN_READ_ONLY";
const KEEPALIVE: Duration = Duration::from_secs(15);
const KEEPALIVE_MAX: usize = 3;
const LEASE_WINDOW: Duration = Duration::from_millis(250);
const REVOKE_TIMEOUT: Duration = Duration::from_secs(5);
const DUPLEX_BYTES: usize = 64 * 1024;
const FRAME_BYTES: usize = 16 * 1024;
const FRAME_CAPACITY: usize = 16;
const READ_ONLY_NOTICE: &str = "attached read-only";
const INPUT_ENABLED_NOTICE: &str = "input enabled";
const REFUSED_NOTICE: &str = "cannot connect to the canonical main process";
const CTRL_C: u8 = 0x03;
const CTRL_D: u8 = 0x04;
const DETACH_PREFIX: u8 = 0x10;
const DETACH_KEY: u8 = 0x11;

struct PinnedKey {
    expected: String,
    mismatch: Arc<AtomicBool>,
}

impl client::Handler for PinnedKey {
    type Error = russh::Error;

    fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        let matches = match key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => {
                !self.expected.is_empty()
                    && key.fingerprint(HashAlg::Sha256).to_string() == self.expected
            }
            russh::keys::PublicKeyOrCertificate::Certificate(_) => false,
        };
        if !matches {
            self.mismatch.store(true, Ordering::SeqCst);
        }
        std::future::ready(Ok(matches))
    }
}

struct RevokeOnDrop {
    inner: Arc<Inner>,
    token: Option<String>,
}

impl RevokeOnDrop {
    fn new(inner: &Arc<Inner>, token: &str) -> RevokeOnDrop {
        RevokeOnDrop {
            inner: Arc::clone(inner),
            token: Some(token.to_owned()),
        }
    }

    fn disarm(&mut self) {
        self.token = None;
    }
}

impl Drop for RevokeOnDrop {
    fn drop(&mut self) {
        let (Some(token), Ok(runtime)) = (self.token.take(), tokio::runtime::Handle::try_current())
        else {
            return;
        };
        let inner = Arc::clone(&self.inner);
        drop(runtime.spawn(async move { inner.revoke(&token).await }));
    }
}

struct Attach {
    channel: Channel<Msg>,
    handle: Handle<PinnedKey>,
    bridge: JoinSet<()>,
}

fn ssh(error: &russh::Error) -> GatewayError {
    GatewayError::Protocol {
        message: format!("ssh: {error}"),
    }
}

fn missing(sandbox: &SandboxName) -> Option<Missing> {
    Some(Missing::Sandbox(sandbox.clone()))
}

fn dims(size: TermSize) -> (u32, u32) {
    (u32::from(size.cols.get()), u32::from(size.rows.get()))
}

fn notice_text(data: &[u8]) -> String {
    TerminalText::sanitize(data).as_str().trim().to_owned()
}

enum Lease {
    Granted(InputLease),
    Refused(String),
}

fn decide_lease(mode: InputMode, early: &[ChannelMsg]) -> Lease {
    let notices: Vec<String> = early
        .iter()
        .filter_map(|message| match message {
            ChannelMsg::ExtendedData { data, .. } => Some(notice_text(data)),
            _ => None,
        })
        .collect();
    if let Some(text) = notices.iter().find(|text| text.contains(REFUSED_NOTICE)) {
        return Lease::Refused(text.clone());
    }
    let read_only = notices.iter().any(|text| text.contains(READ_ONLY_NOTICE));
    if mode == InputMode::Viewer || read_only {
        Lease::Granted(InputLease::ReadOnly)
    } else {
        Lease::Granted(InputLease::Owner)
    }
}

struct Watch {
    lease: InputLease,
    prefix_pending: bool,
    closing: Option<EndReason>,
}

impl Watch {
    fn new(lease: InputLease) -> Watch {
        Watch {
            lease,
            prefix_pending: false,
            closing: None,
        }
    }

    fn grant(&mut self, lease: InputLease, queue: &mut VecDeque<TerminalEvent>) {
        if self.lease != lease {
            self.lease = lease;
            queue.push_back(TerminalEvent::Lease(lease));
        }
    }

    fn sent(&mut self, bytes: &[u8]) {
        if self.closing.is_some() {
            return;
        }
        if self.lease == InputLease::ReadOnly && bytes.contains(&CTRL_C) {
            self.closing = Some(EndReason::StreamClosed);
            return;
        }
        for &byte in bytes {
            if self.prefix_pending {
                self.prefix_pending = false;
                if byte == DETACH_KEY {
                    self.closing = Some(EndReason::Detached);
                    return;
                }
            }
            if byte == CTRL_D {
                self.closing = Some(EndReason::Detached);
                return;
            }
            self.prefix_pending = byte == DETACH_PREFIX;
        }
    }
}

fn translate(
    message: Option<ChannelMsg>,
    queue: &mut VecDeque<TerminalEvent>,
    watch: &mut Watch,
) -> bool {
    match message {
        Some(ChannelMsg::Data { data }) => {
            queue.push_back(TerminalEvent::Output(data.to_vec()));
            false
        }
        Some(ChannelMsg::ExtendedData { data, .. }) => {
            let text = notice_text(&data);
            if text.contains(INPUT_ENABLED_NOTICE) {
                watch.grant(InputLease::Owner, queue);
            } else if text.contains(READ_ONLY_NOTICE) {
                watch.grant(InputLease::ReadOnly, queue);
            }
            queue.push_back(TerminalEvent::Notice(text));
            false
        }
        Some(ChannelMsg::ExitStatus { exit_status: 0 }) if watch.closing.is_some() => false,
        Some(ChannelMsg::ExitStatus { exit_status }) => {
            queue.push_back(TerminalEvent::Exited(Some(
                i32::try_from(exit_status).unwrap_or(i32::MAX),
            )));
            false
        }
        Some(ChannelMsg::ExitSignal { .. }) => {
            queue.push_back(TerminalEvent::Exited(None));
            false
        }
        Some(ChannelMsg::Close) | None => {
            queue.push_back(TerminalEvent::Ended(
                watch.closing.take().unwrap_or(EndReason::StreamClosed),
            ));
            true
        }
        Some(_) => false,
    }
}

impl Inner {
    pub(super) async fn open_main(
        self: &Arc<Self>,
        request: &TerminalRequest,
    ) -> Result<TerminalSession, GatewayError> {
        let TerminalKind::MainProcess { input, repaint } = &request.kind else {
            return Err(GatewayError::Protocol {
                message: "not a main-process terminal".to_owned(),
            });
        };
        let session = self.create_ssh_session(&request.sandbox).await?;
        let mut guard = RevokeOnDrop::new(self, &session.token);
        let opened =
            tokio::time::timeout(self.call_timeout, self.handshake(request, &session, *input))
                .await;
        let result = match opened {
            Ok(result) => result,
            Err(_) => Err(GatewayError::Timeout {
                op: GatewayOp::Terminal,
                after: self.call_timeout,
                group: None,
            }),
        };
        let (attach, early) = match result {
            Ok(opened) => opened,
            Err(error) => {
                self.revoke(&session.token).await;
                guard.disarm();
                return Err(self.refine_not_ready(&request.sandbox, error).await);
            }
        };
        guard.disarm();
        let mut queue = VecDeque::from([TerminalEvent::Attached]);
        let mut ended = false;
        let mut watch = Watch::new(InputLease::ReadOnly);
        match decide_lease(*input, &early) {
            Lease::Refused(text) => {
                queue.push_back(TerminalEvent::Ended(EndReason::Refused(text)));
                ended = true;
            }
            Lease::Granted(lease) => {
                watch.lease = lease;
                queue.push_back(TerminalEvent::Lease(lease));
                for message in early {
                    if translate(Some(message), &mut queue, &mut watch) {
                        ended = true;
                        break;
                    }
                }
            }
        }
        let (terminal, driver) = TerminalSession::pair(SESSION_CAPACITY, request.kind.clone());
        let pump = Pump {
            inner: Arc::clone(self),
            token: session.token,
            attach,
            driver,
            queue,
            watch,
            ended,
            size: request.size,
            input: *input,
            repaint: *repaint,
        };
        tokio::spawn(pump.run());
        Ok(terminal)
    }

    async fn create_ssh_session(
        &self,
        sandbox: &SandboxName,
    ) -> Result<CreateSshSessionResponse, GatewayError> {
        let result = self
            .unary(
                GatewayOp::Terminal,
                missing(sandbox),
                self.grpc().create_ssh_session(CreateSshSessionRequest {
                    workspace_scope: Some(self.scope()),
                    sandbox: sandbox.to_string(),
                }),
            )
            .await;
        match result {
            Ok(session) => Ok(session),
            Err(error) => Err(self.refine_not_ready(sandbox, error).await),
        }
    }

    async fn revoke(&self, token: &str) {
        let mut grpc = self.grpc();
        let call = grpc.revoke_ssh_session(RevokeSshSessionRequest {
            token: token.to_owned(),
            allow_missing: true,
        });
        drop(tokio::time::timeout(REVOKE_TIMEOUT, call).await);
    }

    async fn handshake(
        &self,
        request: &TerminalRequest,
        session: &CreateSshSessionResponse,
        mode: InputMode,
    ) -> Result<(Attach, Vec<ChannelMsg>), GatewayError> {
        let mut bridge = JoinSet::new();
        let (stream, mut fault) = self
            .open_relay(&request.sandbox, &session.token, &mut bridge)
            .await?;
        let sandbox = &request.sandbox;
        let mismatch = Arc::new(AtomicBool::new(false));
        let config = Arc::new(client::Config {
            keepalive_interval: Some(KEEPALIVE),
            keepalive_max: KEEPALIVE_MAX,
            ..client::Config::default()
        });
        let handler = PinnedKey {
            expected: session.host_key_fingerprint.clone(),
            mismatch: Arc::clone(&mismatch),
        };
        let mut handle = match client::connect_stream(config, stream, handler).await {
            Ok(handle) => handle,
            Err(error) => {
                return Err(if mismatch.load(Ordering::SeqCst) {
                    GatewayError::HostKeyMismatch
                } else {
                    self.relay_cause(&mut fault, sandbox, ssh(&error))
                });
            }
        };
        let auth = handle
            .authenticate_none(SSH_USER)
            .await
            .map_err(|error| self.relay_cause(&mut fault, sandbox, ssh(&error)))?;
        if !auth.success() {
            return Err(self.relay_cause(
                &mut fault,
                sandbox,
                GatewayError::TerminalRefused {
                    reason: "the sandbox SSH server rejected the session".to_owned(),
                },
            ));
        }
        let mut channel = handle
            .channel_open_session()
            .await
            .map_err(|error| self.relay_cause(&mut fault, sandbox, ssh(&error)))?;
        if mode == InputMode::Viewer {
            channel
                .set_env(false, READ_ONLY_ENV, "1")
                .await
                .map_err(|error| self.relay_cause(&mut fault, sandbox, ssh(&error)))?;
        }
        let (cols, rows) = dims(request.size);
        channel
            .request_pty(false, SSH_TERM, cols, rows, 0, 0, &[])
            .await
            .map_err(|error| self.relay_cause(&mut fault, sandbox, ssh(&error)))?;
        channel
            .request_subsystem(true, SUBSYSTEM)
            .await
            .map_err(|error| self.relay_cause(&mut fault, sandbox, ssh(&error)))?;
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Success) => break,
                Some(ChannelMsg::Failure) | None => {
                    return Err(self.relay_cause(
                        &mut fault,
                        sandbox,
                        GatewayError::TerminalRefused {
                            reason: "the sandbox refused the main-process subsystem".to_owned(),
                        },
                    ));
                }
                Some(_) => {}
            }
        }
        let deadline = Instant::now() + LEASE_WINDOW;
        let mut early = Vec::new();
        while let Ok(Some(message)) = tokio::time::timeout_at(deadline, channel.wait()).await {
            let significant = !matches!(
                message,
                ChannelMsg::WindowAdjusted { .. } | ChannelMsg::Success
            );
            early.push(message);
            if significant {
                break;
            }
        }
        Ok((
            Attach {
                channel,
                handle,
                bridge,
            },
            early,
        ))
    }

    async fn open_relay(
        &self,
        sandbox: &SandboxName,
        token: &str,
        bridge: &mut JoinSet<()>,
    ) -> Result<(tokio::io::DuplexStream, oneshot::Receiver<Status>), GatewayError> {
        let (frame_tx, frame_rx) = mpsc::channel::<TcpForwardFrame>(FRAME_CAPACITY);
        frame_tx
            .send(TcpForwardFrame {
                payload: Some(tcp_forward_frame::Payload::Init(TcpForwardInit {
                    sandbox: sandbox.to_string(),
                    workspace: self.workspace.to_string(),
                    service_id: format!("jackin:{sandbox}"),
                    target: Some(tcp_forward_init::Target::Ssh(SshRelayTarget {})),
                    authorization_token: token.to_owned(),
                })),
            })
            .await
            .map_err(|_| GatewayError::Protocol {
                message: "the forward request channel closed".to_owned(),
            })?;
        let response = self
            .grpc()
            .forward_tcp(ReceiverStream::new(frame_rx))
            .await
            .map_err(|status| self.map_status(GatewayOp::Terminal, missing(sandbox), &status))?
            .into_inner();
        let (fault_tx, fault_rx) = oneshot::channel();
        let (near, far) = tokio::io::duplex(DUPLEX_BYTES);
        let (far_read, far_write) = tokio::io::split(far);
        bridge.spawn(forward_up(far_read, frame_tx));
        bridge.spawn(forward_down(response, far_write, fault_tx));
        Ok((near, fault_rx))
    }

    fn relay_cause(
        &self,
        fault: &mut oneshot::Receiver<Status>,
        sandbox: &SandboxName,
        otherwise: GatewayError,
    ) -> GatewayError {
        match fault.try_recv() {
            Ok(status) => self.map_status(GatewayOp::Terminal, missing(sandbox), &status),
            Err(_) => otherwise,
        }
    }
}

async fn forward_down(
    mut response: tonic::Streaming<TcpForwardFrame>,
    mut sink: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    fault: oneshot::Sender<Status>,
) {
    loop {
        match response.message().await {
            Ok(Some(TcpForwardFrame {
                payload: Some(tcp_forward_frame::Payload::Data(data)),
            })) => {
                if sink.write_all(&data).await.is_err() {
                    break;
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(status) => {
                drop(fault.send(status));
                break;
            }
        }
    }
    drop(sink.shutdown().await);
}

async fn forward_up(
    mut source: tokio::io::ReadHalf<tokio::io::DuplexStream>,
    frames: mpsc::Sender<TcpForwardFrame>,
) {
    let mut buffer = vec![0_u8; FRAME_BYTES];
    while let Ok(count) = source.read(&mut buffer).await {
        if count == 0 {
            break;
        }
        let frame = TcpForwardFrame {
            payload: Some(tcp_forward_frame::Payload::Data(buffer[..count].to_vec())),
        };
        if frames.send(frame).await.is_err() {
            break;
        }
    }
}

struct Pump {
    inner: Arc<Inner>,
    token: String,
    attach: Attach,
    driver: TerminalDriver,
    queue: VecDeque<TerminalEvent>,
    watch: Watch,
    ended: bool,
    size: TermSize,
    input: InputMode,
    repaint: RepaintPolicy,
}

impl Pump {
    async fn run(self) {
        let Pump {
            inner,
            token,
            mut attach,
            driver,
            mut queue,
            mut watch,
            mut ended,
            mut size,
            input,
            repaint,
        } = self;
        let TerminalDriver {
            mut commands,
            events,
        } = driver;
        let mut nudge_at = match repaint {
            RepaintPolicy::Never => None,
            RepaintPolicy::NudgeAfter(delay) => Some(Instant::now() + delay),
        };
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
                    None => {
                        close_channel(&attach.channel).await;
                        break;
                    }
                    Some(TerminalCommand::Input(bytes)) => {
                        watch.sent(&bytes);
                        if input == InputMode::Viewer {
                            if let Some(reason) = watch.closing.take() {
                                close_channel(&attach.channel).await;
                                queue.push_back(TerminalEvent::Ended(reason));
                                ended = true;
                            }
                        } else if let Err(error) = attach.channel.data_bytes(bytes).await {
                            queue.push_back(TerminalEvent::Ended(EndReason::Failed(error.to_string())));
                            ended = true;
                        }
                    }
                    Some(TerminalCommand::Resize(new_size)) => {
                        size = new_size;
                        let (cols, rows) = dims(size);
                        if let Err(error) = attach.channel.window_change(cols, rows, 0, 0).await {
                            queue.push_back(TerminalEvent::Ended(EndReason::Failed(error.to_string())));
                            ended = true;
                        }
                    }
                    Some(TerminalCommand::Detach) => {
                        close_channel(&attach.channel).await;
                        queue.push_back(TerminalEvent::Ended(EndReason::Detached));
                        ended = true;
                    }
                },
                () = tokio::time::sleep_until(nudge_at.unwrap_or_else(Instant::now)), if nudge_at.is_some() && !ended => {
                    nudge_at = None;
                    let (cols, rows) = dims(size);
                    if rows > 1 {
                        for step in [rows - 1, rows] {
                            if attach.channel.window_change(cols, step, 0, 0).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                message = attach.channel.wait(), if queue.is_empty() && !ended => {
                    if translate(message, &mut queue, &mut watch) {
                        ended = true;
                    }
                }
            }
        }
        drop(
            attach
                .handle
                .disconnect(Disconnect::ByApplication, "done", "en")
                .await,
        );
        attach.bridge.abort_all();
        inner.revoke(&token).await;
    }
}

async fn close_channel(channel: &Channel<Msg>) {
    drop(channel.eof().await);
    drop(channel.close().await);
}
