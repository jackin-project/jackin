// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Pane I/O tasks. Each pane runs one task that owns its remote session and talks to the app
//! over channels: `PaneCmd` in, `PaneEvent` out. The transport code is the proof from main.rs.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use openshell_sdk::raw::proto::{
    self, CreateSshSessionRequest, ExecSandboxInput, ExecSandboxRequest, ExecSandboxWindowResize,
    SshRelayTarget, TcpForwardFrame, TcpForwardInit, exec_sandbox_event, exec_sandbox_input,
    tcp_forward_frame, tcp_forward_init,
};
use openshell_sdk::{ClientConfig, ClientIdentity, OpenShellClient};
use russh::ChannelMsg;
use russh::client;
use russh::keys::HashAlg;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Debug)]
pub enum PaneCmd {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Detach,
}

#[derive(Debug)]
pub enum PaneEvent {
    Attached,
    Output(Vec<u8>),
    Notice(String),
    Exited(Option<i32>),
    Ended(String),
}

#[derive(Clone)]
pub struct Remote {
    pub client: OpenShellClient,
    pub workspace: String,
    pub sandbox: String,
}

pub async fn connect(gateway: &str) -> Result<OpenShellClient> {
    let dir = std::path::PathBuf::from(std::env::var("HOME")?)
        .join(".config/openshell/gateways")
        .join(gateway);
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("metadata.json"))?)?;
    let endpoint = metadata["gateway_endpoint"]
        .as_str()
        .context("gateway_endpoint missing")?;
    let mtls = dir.join("mtls");
    let mut config = ClientConfig::new(endpoint);
    config.ca_cert = Some(std::fs::read(mtls.join("ca.crt"))?);
    config.client_identity = Some(ClientIdentity::new(
        std::fs::read(mtls.join("tls.crt"))?,
        std::fs::read(mtls.join("tls.key"))?,
    ));
    Ok(OpenShellClient::connect(config).await?)
}

struct Verifier {
    expected: String,
}

impl client::Handler for Verifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(match key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => {
                key.fingerprint(HashAlg::Sha256).to_string() == self.expected
            }
            _ => false,
        })
    }
}

pub fn spawn_main(
    remote: Remote,
    cols: u16,
    rows: u16,
    nudge: bool,
    events: mpsc::UnboundedSender<PaneEvent>,
) -> mpsc::UnboundedSender<PaneCmd> {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let end = match run_main(&remote, cols, rows, nudge, cmd_rx, &events).await {
            Ok(end) => end,
            Err(error) => format!("error: {error:#}"),
        };
        let _ = events.send(PaneEvent::Ended(end));
    });
    cmd_tx
}

async fn run_main(
    remote: &Remote,
    cols: u16,
    rows: u16,
    nudge: bool,
    mut cmds: mpsc::UnboundedReceiver<PaneCmd>,
    events: &mpsc::UnboundedSender<PaneEvent>,
) -> Result<String> {
    let mut grpc = remote.client.raw_grpc();
    let session = grpc
        .create_ssh_session(CreateSshSessionRequest {
            sandbox: remote.sandbox.clone(),
            workspace_scope: Some(proto::workspace_selector(&remote.workspace)),
        })
        .await
        .context("create_ssh_session")?
        .into_inner();

    let (frame_tx, frame_rx) = mpsc::channel::<TcpForwardFrame>(16);
    frame_tx
        .send(TcpForwardFrame {
            payload: Some(tcp_forward_frame::Payload::Init(TcpForwardInit {
                sandbox: remote.sandbox.clone(),
                workspace: remote.workspace.clone(),
                service_id: format!("p06-panes:{}", remote.sandbox),
                target: Some(tcp_forward_init::Target::Ssh(SshRelayTarget {})),
                authorization_token: session.token.clone(),
            })),
        })
        .await?;
    let mut response = grpc
        .forward_tcp(ReceiverStream::new(frame_rx))
        .await
        .context("forward_tcp")?
        .into_inner();

    let (near, far) = tokio::io::duplex(64 * 1024);
    let (mut far_read, mut far_write) = tokio::io::split(far);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            match far_read.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let frame = TcpForwardFrame {
                        payload: Some(tcp_forward_frame::Payload::Data(buf[..n].to_vec())),
                    };
                    if frame_tx.send(frame).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    tokio::spawn(async move {
        while let Ok(Some(frame)) = response.message().await {
            if let Some(tcp_forward_frame::Payload::Data(data)) = frame.payload
                && far_write.write_all(&data).await.is_err()
            {
                break;
            }
        }
        let _ = far_write.shutdown().await;
    });

    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        ..Default::default()
    });
    let mut handle = client::connect_stream(
        config,
        near,
        Verifier {
            expected: session.host_key_fingerprint.clone(),
        },
    )
    .await
    .context("ssh handshake (host key must match CreateSshSession)")?;
    if !handle.authenticate_none("sandbox").await?.success() {
        bail!("ssh auth_none rejected");
    }
    let mut channel = handle.channel_open_session().await?;
    channel
        .request_pty(
            false,
            "xterm-256color",
            u32::from(cols),
            u32::from(rows),
            0,
            0,
            &[],
        )
        .await?;
    channel.request_subsystem(true, "openshell-main").await?;
    let _ = events.send(PaneEvent::Attached);

    let mut size = (cols, rows);
    let mut exit_code = None;
    let nudge_timer = tokio::time::sleep(Duration::from_secs(3600));
    tokio::pin!(nudge_timer);
    if nudge {
        nudge_timer
            .as_mut()
            .reset(tokio::time::Instant::now() + Duration::from_millis(600));
    }
    let mut nudge_pending = nudge;
    let end = loop {
        tokio::select! {
            message = channel.wait() => match message {
                Some(ChannelMsg::Data { data }) => {
                    let _ = events.send(PaneEvent::Output(data.to_vec()));
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    let _ = events.send(PaneEvent::Notice(
                        String::from_utf8_lossy(&data).trim().to_string(),
                    ));
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = Some(i32::try_from(exit_status).unwrap_or(i32::MAX));
                }
                Some(ChannelMsg::Close) | None => {
                    let _ = events.send(PaneEvent::Exited(exit_code));
                    break "channel closed".to_string();
                }
                Some(_) => {}
            },
            command = cmds.recv() => match command {
                Some(PaneCmd::Input(bytes)) => channel.data(&bytes[..]).await?,
                Some(PaneCmd::Resize { cols, rows }) => {
                    size = (cols, rows);
                    channel.window_change(u32::from(cols), u32::from(rows), 0, 0).await?;
                }
                Some(PaneCmd::Detach) | None => {
                    let _ = channel.eof().await;
                    let _ = channel.close().await;
                    let _ = handle
                        .disconnect(russh::Disconnect::ByApplication, "detach", "en")
                        .await;
                    break "detached".to_string();
                }
            },
            () = &mut nudge_timer, if nudge_pending => {
                nudge_pending = false;
                let (cols, rows) = size;
                channel
                    .window_change(u32::from(cols), u32::from(rows.saturating_sub(1).max(1)), 0, 0)
                    .await?;
                channel.window_change(u32::from(cols), u32::from(rows), 0, 0).await?;
            }
        }
    };
    Ok(end)
}

pub fn spawn_shell(
    remote: Remote,
    cols: u16,
    rows: u16,
    events: mpsc::UnboundedSender<PaneEvent>,
) -> mpsc::UnboundedSender<PaneCmd> {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let end = match run_shell(&remote, cols, rows, cmd_rx, &events).await {
            Ok(end) => end,
            Err(error) => format!("error: {error:#}"),
        };
        let _ = events.send(PaneEvent::Ended(end));
    });
    cmd_tx
}

async fn run_shell(
    remote: &Remote,
    cols: u16,
    rows: u16,
    mut cmds: mpsc::UnboundedReceiver<PaneCmd>,
    events: &mpsc::UnboundedSender<PaneEvent>,
) -> Result<String> {
    let (tx, rx) = mpsc::channel::<ExecSandboxInput>(64);
    tx.send(ExecSandboxInput {
        payload: Some(exec_sandbox_input::Payload::Start(ExecSandboxRequest {
            sandbox: remote.sandbox.clone(),
            workspace_scope: Some(proto::workspace_selector(&remote.workspace)),
            command: vec!["bash".to_string()],
            tty: true,
            cols: u32::from(cols),
            rows: u32::from(rows),
            workdir: "/sandbox".to_string(),
            environment: [("JACKIN_PANE".to_string(), "shell-1".to_string())].into(),
            ..Default::default()
        })),
    })
    .await?;
    let mut grpc = remote.client.raw_grpc();
    let mut stream = grpc
        .exec_sandbox_interactive(ReceiverStream::new(rx))
        .await
        .context("exec_sandbox_interactive")?
        .into_inner();
    let _ = events.send(PaneEvent::Attached);

    let end = loop {
        tokio::select! {
            event = stream.next() => match event {
                None => break "stream ended".to_string(),
                Some(Err(status)) => break format!("status {status}"),
                Some(Ok(event)) => match event.payload {
                    Some(exec_sandbox_event::Payload::Stdout(chunk)) => {
                        let _ = events.send(PaneEvent::Output(chunk.data));
                    }
                    Some(exec_sandbox_event::Payload::Stderr(chunk)) => {
                        let _ = events.send(PaneEvent::Output(chunk.data));
                    }
                    Some(exec_sandbox_event::Payload::Exit(exit)) => {
                        let _ = events.send(PaneEvent::Exited(Some(exit.exit_code)));
                    }
                    None => {}
                },
            },
            command = cmds.recv() => match command {
                Some(PaneCmd::Input(bytes)) => {
                    for chunk in bytes.chunks(4096) {
                        tx.send(ExecSandboxInput {
                            payload: Some(exec_sandbox_input::Payload::Stdin(chunk.to_vec())),
                        })
                        .await?;
                    }
                }
                Some(PaneCmd::Resize { cols, rows }) => {
                    tx.send(ExecSandboxInput {
                        payload: Some(exec_sandbox_input::Payload::Resize(ExecSandboxWindowResize {
                            cols: u32::from(cols),
                            rows: u32::from(rows),
                        })),
                    })
                    .await?;
                }
                Some(PaneCmd::Detach) | None => break "closed (remote process killed)".to_string(),
            },
        }
    };
    Ok(end)
}
