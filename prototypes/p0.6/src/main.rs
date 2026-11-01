// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use openshell_sdk::raw::proto::{
    self, CreateSshSessionRequest, ExecSandboxInput, ExecSandboxRequest, ExecSandboxWindowResize,
    RevokeSshSessionRequest,
    SshRelayTarget, TcpForwardFrame, TcpForwardInit, exec_sandbox_event, exec_sandbox_input,
    tcp_forward_frame, tcp_forward_init,
};
use openshell_sdk::{ClientConfig, ClientIdentity, OpenShellClient};
use russh::client::{self, Handle, Msg};
use russh::keys::HashAlg;
use russh::{Channel, ChannelMsg};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

struct Ctx {
    client: OpenShellClient,
    workspace: String,
    sandbox: String,
    started: Instant,
}

fn show(bytes: &[u8]) -> String {
    bytes.escape_ascii().to_string()
}

fn check(name: &str, ok: bool) {
    println!("CHECK {name}: {}", if ok { "PASS" } else { "FAIL" });
}

impl Ctx {
    fn log(&self, who: &str, what: impl AsRef<str>) {
        println!(
            "[+{:6.2}s {who}] {}",
            self.started.elapsed().as_secs_f32(),
            what.as_ref()
        );
    }
}

async fn connect(gateway: &str) -> Result<OpenShellClient> {
    let dir = std::path::PathBuf::from(std::env::var("HOME")?)
        .join(".config/openshell/gateways")
        .join(gateway);
    let metadata: serde_json::Value = serde_json::from_slice(&std::fs::read(
        dir.join("metadata.json"),
    )?)?;
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

fn exec_start(sandbox: &str, workspace: &str, command: &[&str], cols: u32, rows: u32) -> ExecSandboxInput {
    ExecSandboxInput {
        payload: Some(exec_sandbox_input::Payload::Start(ExecSandboxRequest {
            sandbox: sandbox.to_string(),
            workspace_scope: Some(proto::workspace_selector(workspace)),
            command: command.iter().map(ToString::to_string).collect(),
            tty: true,
            cols,
            rows,
            ..Default::default()
        })),
    }
}

fn stdin(data: &[u8]) -> ExecSandboxInput {
    ExecSandboxInput {
        payload: Some(exec_sandbox_input::Payload::Stdin(data.to_vec())),
    }
}

fn resize(cols: u32, rows: u32) -> ExecSandboxInput {
    ExecSandboxInput {
        payload: Some(exec_sandbox_input::Payload::Resize(ExecSandboxWindowResize {
            cols,
            rows,
        })),
    }
}

async fn shell(ctx: &Ctx) -> Result<()> {
    let (tx, rx) = mpsc::channel::<ExecSandboxInput>(64);
    let mut start = exec_start(&ctx.sandbox, &ctx.workspace, &["bash"], 100, 30);
    if let Some(exec_sandbox_input::Payload::Start(request)) = start.payload.as_mut() {
        request.workdir = "/sandbox".to_string();
        request
            .environment
            .insert("JACKIN_PANE".to_string(), "shell-1".to_string());
    }
    tx.send(start).await?;
    let mut grpc = ctx.client.raw_grpc();
    let mut events = grpc
        .exec_sandbox_interactive(ReceiverStream::new(rx))
        .await
        .context("exec_sandbox_interactive")?
        .into_inner();
    ctx.log("shell", "stream open (start message sent)");

    let script = tokio::spawn({
        let tx = tx.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(800)).await;
            tx.send(stdin(b"echo hi\n")).await.ok();
            tokio::time::sleep(Duration::from_millis(500)).await;
            tx.send(stdin(b"stty size\n")).await.ok();
            tokio::time::sleep(Duration::from_millis(500)).await;
            tx.send(resize(120, 40)).await.ok();
            tokio::time::sleep(Duration::from_millis(500)).await;
            tx.send(stdin(b"stty size\n")).await.ok();
            tokio::time::sleep(Duration::from_millis(500)).await;
            tx.send(stdin(b"echo $TERM; id -un; pwd; echo pane=$JACKIN_PANE; exit 3\n")).await.ok();
        }
    });

    let mut out = Vec::new();
    let mut exit = None;
    let stream_end;
    loop {
        match tokio::time::timeout(Duration::from_secs(20), events.next()).await {
            Err(_) => bail!("no event for 20s"),
            Ok(None) => {
                stream_end = Some("clean end of stream".to_string());
                break;
            }
            Ok(Some(Err(status))) => {
                stream_end = Some(format!("status {status}"));
                break;
            }
            Ok(Some(Ok(event))) => match event.payload {
                Some(exec_sandbox_event::Payload::Stdout(chunk)) => {
                    ctx.log("shell", format!("stdout {}", show(&chunk.data)));
                    out.extend_from_slice(&chunk.data);
                }
                Some(exec_sandbox_event::Payload::Stderr(chunk)) => {
                    ctx.log("shell", format!("stderr {}", show(&chunk.data)));
                }
                Some(exec_sandbox_event::Payload::Exit(exit_event)) => {
                    ctx.log("shell", format!("exit {}", exit_event.exit_code));
                    exit = Some(exit_event.exit_code);
                }
                None => ctx.log("shell", "empty event"),
            },
        }
    }
    script.await?;
    ctx.log("shell", format!("stream ended: {}", stream_end.unwrap_or_default()));
    let text = String::from_utf8_lossy(&out).to_string();
    check("echo hi output", text.contains("hi\r\n"));
    check("initial pty size 30 100", text.contains("30 100"));
    check("resize applied 40 120", text.contains("40 120"));
    check("TERM is xterm-256color", text.contains("xterm-256color"));
    check("workdir applied", text.contains("/sandbox\r\n"));
    check("environment applied", text.contains("pane=shell-1"));
    check("exit status 3", exit == Some(3));
    Ok(())
}

async fn shell_drop(ctx: &Ctx) -> Result<()> {
    let (tx, rx) = mpsc::channel::<ExecSandboxInput>(64);
    tx.send(exec_start(
        &ctx.sandbox,
        &ctx.workspace,
        &["sh", "-c", "exec sleep 4242"],
        80,
        24,
    ))
    .await?;
    let mut grpc = ctx.client.raw_grpc();
    let events = grpc
        .exec_sandbox_interactive(ReceiverStream::new(rx))
        .await?
        .into_inner();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let before = ctx
        .client
        .exec(
            &ctx.sandbox,
            &["sh".into(), "-c".into(), "ps -eo args= | grep -c '^sleep 4242$'".into()],
            Default::default(),
        )
        .await?;
    ctx.log("drop", format!("sleep processes before drop: {}", show(&before.stdout)));
    drop(events);
    drop(tx);
    tokio::time::sleep(Duration::from_secs(3)).await;
    let after = ctx
        .client
        .exec(
            &ctx.sandbox,
            &["sh".into(), "-c".into(), "ps -eo args= | grep -c '^sleep 4242$'".into()],
            Default::default(),
        )
        .await?;
    ctx.log("drop", format!("sleep processes after drop: {}", show(&after.stdout)));
    check(
        "dropping the response stream kills the remote process",
        String::from_utf8_lossy(&before.stdout).trim() == "1"
            && String::from_utf8_lossy(&after.stdout).trim() == "0",
    );
    Ok(())
}

struct Verifier {
    expected: String,
    seen: Arc<std::sync::Mutex<String>>,
}

impl client::Handler for Verifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = match key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => {
                key.fingerprint(HashAlg::Sha256).to_string()
            }
            other => format!("unsupported key kind {other:?}"),
        };
        *self.seen.lock().unwrap() = fingerprint.clone();
        Ok(self.expected.is_empty() || fingerprint == self.expected)
    }
}

struct Attach {
    name: &'static str,
    channel: Channel<Msg>,
    handle: Handle<Verifier>,
    fingerprint_ok: bool,
}

async fn session_token(ctx: &Ctx) -> Result<proto::CreateSshSessionResponse> {
    let mut grpc = ctx.client.raw_grpc();
    Ok(grpc
        .create_ssh_session(CreateSshSessionRequest {
            sandbox: ctx.sandbox.clone(),
            workspace_scope: Some(proto::workspace_selector(&ctx.workspace)),
        })
        .await
        .context("create_ssh_session")?
        .into_inner())
}

async fn open_relay(
    ctx: &Ctx,
    token: &str,
) -> Result<(
    mpsc::Sender<TcpForwardFrame>,
    tonic::Streaming<TcpForwardFrame>,
)> {
    let (tx, rx) = mpsc::channel::<TcpForwardFrame>(16);
    tx.send(TcpForwardFrame {
        payload: Some(tcp_forward_frame::Payload::Init(TcpForwardInit {
            sandbox: ctx.sandbox.clone(),
            workspace: ctx.workspace.clone(),
            service_id: format!("p06-proof:{}", ctx.sandbox),
            target: Some(tcp_forward_init::Target::Ssh(SshRelayTarget {})),
            authorization_token: token.to_string(),
        })),
    })
    .await?;
    let mut grpc = ctx.client.raw_grpc();
    let response = grpc
        .forward_tcp(ReceiverStream::new(rx))
        .await
        .context("forward_tcp")?
        .into_inner();
    Ok((tx, response))
}

async fn attach(
    ctx: &Ctx,
    name: &'static str,
    session: &proto::CreateSshSessionResponse,
    cols: u32,
    rows: u32,
    read_only: bool,
) -> Result<Attach> {
    let (tx, mut response) = open_relay(ctx, &session.token).await?;
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
                    if tx.send(frame).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    tokio::spawn(async move {
        while let Ok(Some(frame)) = response.message().await {
            if let Some(tcp_forward_frame::Payload::Data(data)) = frame.payload {
                if far_write.write_all(&data).await.is_err() {
                    break;
                }
            }
        }
        let _ = far_write.shutdown().await;
    });

    let seen = Arc::new(std::sync::Mutex::new(String::new()));
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
            seen: seen.clone(),
        },
    )
    .await
    .context("ssh handshake")?;
    let auth = handle.authenticate_none("sandbox").await?;
    if !auth.success() {
        bail!("ssh auth_none rejected");
    }
    let channel = handle.channel_open_session().await?;
    if read_only {
        channel.set_env(false, "OPENSHELL_MAIN_READ_ONLY", "1").await?;
    }
    channel
        .request_pty(false, "xterm-256color", cols, rows, 0, 0, &[])
        .await?;
    channel.request_subsystem(true, "openshell-main").await?;
    let fingerprint = seen.lock().unwrap().clone();
    ctx.log(
        name,
        format!(
            "attached: pty {cols}x{rows}, host key {fingerprint}, pinned={}",
            session.host_key_fingerprint
        ),
    );
    Ok(Attach {
        name,
        channel,
        handle,
        fingerprint_ok: !fingerprint.is_empty() && fingerprint == session.host_key_fingerprint,
    })
}

#[derive(Default)]
struct Collected {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit: Option<u32>,
    closed: bool,
}

impl Collected {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).replace('\r', "")
    }
    fn warn(&self) -> String {
        String::from_utf8_lossy(&self.stderr).replace('\r', "")
    }
    fn lines(&self) -> Vec<String> {
        self.text().lines().map(ToString::to_string).collect()
    }
}

impl Attach {
    async fn collect(&mut self, ctx: &Ctx, window: Duration) -> Collected {
        let mut got = Collected::default();
        let end = tokio::time::Instant::now() + window;
        loop {
            match tokio::time::timeout_at(end, self.channel.wait()).await {
                Err(_) => break,
                Ok(None) => {
                    got.closed = true;
                    break;
                }
                Ok(Some(ChannelMsg::Data { data })) => got.stdout.extend_from_slice(&data),
                Ok(Some(ChannelMsg::ExtendedData { data, .. })) => {
                    got.stderr.extend_from_slice(&data)
                }
                Ok(Some(ChannelMsg::ExitStatus { exit_status })) => got.exit = Some(exit_status),
                Ok(Some(ChannelMsg::Close)) => {
                    got.closed = true;
                    break;
                }
                Ok(Some(_)) => {}
            }
        }
        let lines = got.lines();
        let head: Vec<_> = lines.iter().take(3).cloned().collect();
        let tail: Vec<_> = lines.iter().rev().take(3).rev().cloned().collect();
        ctx.log(
            self.name,
            format!(
                "collected {} stdout bytes, {} lines, stderr={:?}, exit={:?}, closed={}\n    head={head:?}\n    tail={tail:?}",
                got.stdout.len(),
                lines.len(),
                got.warn(),
                got.exit,
                got.closed
            ),
        );
        got
    }

    async fn send(&self, ctx: &Ctx, data: &[u8]) -> Result<()> {
        ctx.log(self.name, format!("send {}", show(data)));
        self.channel.data(data).await?;
        Ok(())
    }

    async fn resize(&self, ctx: &Ctx, cols: u32, rows: u32) -> Result<()> {
        ctx.log(self.name, format!("window_change {cols}x{rows}"));
        self.channel.window_change(cols, rows, 0, 0).await?;
        Ok(())
    }

    async fn close(self, ctx: &Ctx) -> Result<()> {
        ctx.log(self.name, "eof + close + disconnect");
        let _ = self.channel.eof().await;
        let _ = self.channel.close().await;
        let _ = self
            .handle
            .disconnect(russh::Disconnect::ByApplication, "done", "en")
            .await;
        Ok(())
    }
}

fn ticks(lines: &[String]) -> Vec<u32> {
    lines
        .iter()
        .filter_map(|line| line.strip_prefix("tick ")?.trim().parse().ok())
        .collect()
}

fn gapless(ticks: &[u32]) -> bool {
    !ticks.is_empty() && ticks.windows(2).all(|pair| pair[1] == pair[0] + 1)
}

async fn main_flow(ctx: &Ctx) -> Result<()> {
    let session = session_token(ctx).await?;
    ctx.log(
        "session",
        format!(
            "sandbox_id={} gateway={}://{}:{} fingerprint={} expires={:?}",
            session.sandbox_id,
            session.gateway_scheme,
            session.gateway_host,
            session.gateway_port,
            session.host_key_fingerprint,
            session.expiration_time.as_ref().map(|t| t.seconds)
        ),
    );

    let mut a1 = attach(ctx, "attach1", &session, 100, 30, false).await?;
    check("host key matches CreateSshSession fingerprint", a1.fingerprint_ok);
    let first = a1.collect(ctx, Duration::from_secs(3)).await;
    check("attach1 sees main banner", first.text().contains("main pid="));
    a1.send(ctx, b"hello\r").await?;
    let echoed = a1.collect(ctx, Duration::from_millis(2500)).await;
    check("attach1 typed line reached main", echoed.text().contains("got: hello"));
    a1.resize(ctx, 120, 40).await?;
    let resized = a1.collect(ctx, Duration::from_millis(2500)).await;
    check(
        "attach1 resize reached main PTY",
        resized.text().contains("WINCH size=40 120"),
    );
    a1.send(ctx, &[0x10, 0x11]).await?;
    let detached = a1.collect(ctx, Duration::from_secs(5)).await;
    check("Ctrl-P Ctrl-Q closes the channel", detached.closed);
    println!("note: exit status on detach = {:?}", detached.exit);
    a1.close(ctx).await?;

    tokio::time::sleep(Duration::from_secs(4)).await;

    let session2 = session_token(ctx).await?;
    let mut a2 = attach(ctx, "attach2", &session2, 120, 40, false).await?;
    let replay = a2.collect(ctx, Duration::from_secs(3)).await;
    let lines = replay.lines();
    let t = ticks(&lines);
    check("attach2 replay starts at the banner", lines.first().is_some_and(|l| l.contains("main pid=")));
    check("attach2 replay has tick 1", t.first() == Some(&1));
    check("attach2 replay is gapless", gapless(&t));
    check("attach2 replay holds the typed line", replay.text().contains("got: hello"));
    check("attach2 replay holds the resize", replay.text().contains("WINCH size=40 120"));
    check("attach2 replay covers detached time (more than 8 ticks)", t.len() > 8);
    println!("replay ticks: 1..{} ({} ticks)", t.last().unwrap_or(&0), t.len());
    check("attach2 got input ownership (no warning)", replay.warn().is_empty());
    a2.close(ctx).await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    let session3 = session_token(ctx).await?;
    let mut a3 = attach(ctx, "attach3", &session3, 100, 30, false).await?;
    let again = a3.collect(ctx, Duration::from_secs(2)).await;
    check(
        "attach3 after close-without-detach-keys owns input",
        again.warn().is_empty(),
    );
    a3.send(ctx, b"third\r").await?;
    let third = a3.collect(ctx, Duration::from_secs(2)).await;
    check("attach3 typed line reached main", third.text().contains("got: third"));
    a3.send(ctx, &[0x04]).await?;
    let ctrl_d = a3.collect(ctx, Duration::from_secs(5)).await;
    check("Ctrl-D closes the channel", ctrl_d.closed);
    a3.close(ctx).await?;
    Ok(())
}

async fn concurrent(ctx: &Ctx) -> Result<()> {
    let s_a = session_token(ctx).await?;
    let s_b = session_token(ctx).await?;
    let mut a = attach(ctx, "A", &s_a, 100, 30, false).await?;
    let a0 = a.collect(ctx, Duration::from_secs(2)).await;
    check("A owns input", a0.warn().is_empty());
    let a_last = ticks(&a0.lines()).last().copied().unwrap_or(0);

    let mut b = attach(ctx, "B", &s_b, 80, 24, false).await?;
    let b0 = b.collect(ctx, Duration::from_millis(1500)).await;
    check(
        "B attached read-only with owner warning",
        b0.warn().contains("already has an input owner"),
    );
    check("B replay has banner", b0.text().contains("main pid="));
    let a1 = a.collect(ctx, Duration::from_secs(2)).await;
    let a_ticks = ticks(&a1.lines());
    check(
        "A kept receiving output after B attached (not kicked)",
        a_ticks.first().is_some_and(|first| *first > a_last),
    );
    check(
        "B attach pty size 80x24 resized the shared main PTY (A sees WINCH 24 80)",
        a1.text().contains("WINCH size=24 80"),
    );
    b.send(ctx, b"fromB\r").await?;
    let a2 = a.collect(ctx, Duration::from_secs(2)).await;
    let b1 = b.collect(ctx, Duration::from_millis(500)).await;
    check("B input did not reach main while A owns input", !a2.text().contains("got: fromB") && !b1.text().contains("got: fromB"));
    b.resize(ctx, 90, 20).await?;
    let a3 = a.collect(ctx, Duration::from_secs(2)).await;
    check(
        "viewer B resize also resizes the shared PTY (A sees WINCH 20 90)",
        a3.text().contains("WINCH size=20 90"),
    );

    a.send(ctx, &[0x10, 0x11]).await?;
    let _ = a.collect(ctx, Duration::from_secs(3)).await;
    a.close(ctx).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    b.send(ctx, b"afterA\r").await?;
    let b2 = b.collect(ctx, Duration::from_secs(3)).await;
    check("B acquires input after A detached", b2.warn().contains("input enabled"));
    check("B typed line reached main", b2.text().contains("got: afterA"));
    b.send(ctx, &[0x10, 0x11]).await?;
    let _ = b.collect(ctx, Duration::from_secs(3)).await;
    b.close(ctx).await?;

    let s_c = session_token(ctx).await?;
    let mut c = attach(ctx, "C-viewer", &s_c, 100, 30, true).await?;
    let c0 = c.collect(ctx, Duration::from_secs(2)).await;
    check("explicit read-only viewer gets no warning text", c0.warn().is_empty());
    c.send(ctx, b"fromC\r").await?;
    let c1 = c.collect(ctx, Duration::from_secs(2)).await;
    check("explicit read-only input ignored", !c1.text().contains("got: fromC"));
    c.send(ctx, &[0x03]).await?;
    let c2 = c.collect(ctx, Duration::from_secs(3)).await;
    check("viewer Ctrl-C closes only the viewer", c2.closed);
    c.close(ctx).await?;
    Ok(())
}

async fn limit(ctx: &Ctx) -> Result<()> {
    let session = session_token(ctx).await?;
    let mut held = Vec::new();
    for index in 1..=4 {
        match open_relay(ctx, &session.token).await {
            Ok(pair) => {
                ctx.log("limit", format!("forward {index}: opened"));
                held.push(pair);
            }
            Err(error) => ctx.log("limit", format!("forward {index}: {error:#}")),
        }
    }
    check("fourth ForwardTcp on one token refused", held.len() == 3);
    Ok(())
}

async fn exit_flow(ctx: &Ctx) -> Result<()> {
    let session = session_token(ctx).await?;
    let mut a = attach(ctx, "live", &session, 100, 30, false).await?;
    let first = a.collect(ctx, Duration::from_secs(2)).await;
    check("live attach replays the banner", first.text().contains("bye"));
    let end = a.collect(ctx, Duration::from_secs(40)).await;
    check("main output delivered before exit", end.text().contains("leaving"));
    check("main exit code 7 delivered as SSH exit status", end.exit == Some(7));
    check("channel closed after exit", end.closed);
    a.close(ctx).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let session = session_token(ctx).await?;
    let mut late = attach(ctx, "late", &session, 100, 30, false).await?;
    let got = late.collect(ctx, Duration::from_secs(5)).await;
    ctx.log("late", format!("late attach text={:?} warn={:?}", got.text(), got.warn()));
    late.close(ctx).await?;
    Ok(())
}

async fn revoke(ctx: &Ctx) -> Result<()> {
    let session = session_token(ctx).await?;
    let mut a = attach(ctx, "rev", &session, 100, 30, false).await?;
    let first = a.collect(ctx, Duration::from_secs(1)).await;
    check("attached before revoke", first.text().contains("main pid="));
    let mut grpc = ctx.client.raw_grpc();
    let outcome = grpc
        .revoke_ssh_session(RevokeSshSessionRequest {
            token: session.token.clone(),
            allow_missing: false,
        })
        .await?
        .into_inner();
    ctx.log("rev", format!("revoke outcome={}", outcome.outcome));
    let later = a.collect(ctx, Duration::from_secs(3)).await;
    check("established attachment survives revoke", !ticks(&later.lines()).is_empty());
    match open_relay(ctx, &session.token).await {
        Ok(_) => check("revoked token refused for a new ForwardTcp", false),
        Err(error) => {
            ctx.log("rev", format!("new forward: {error:#}"));
            check("revoked token refused for a new ForwardTcp", true);
        }
    }
    a.send(ctx, &[0x04]).await?;
    let _ = a.collect(ctx, Duration::from_secs(2)).await;
    a.close(ctx).await?;
    Ok(())
}

async fn hold(ctx: &Ctx) -> Result<()> {
    let session = session_token(ctx).await?;
    let mut a = attach(ctx, "hold", &session, 100, 30, false).await?;
    let first = a.collect(ctx, Duration::from_secs(1)).await;
    check("hold owns input", first.warn().is_empty());
    let _ = a.collect(ctx, Duration::from_secs(120)).await;
    Ok(())
}

async fn probe(ctx: &Ctx) -> Result<()> {
    let session = session_token(ctx).await?;
    let mut a = attach(ctx, "probe", &session, 100, 30, false).await?;
    let first = a.collect(ctx, Duration::from_secs(1)).await;
    check("probe owns input right after the holder was killed", first.warn().is_empty());
    a.send(ctx, &[0x04]).await?;
    let _ = a.collect(ctx, Duration::from_secs(2)).await;
    a.close(ctx).await?;
    Ok(())
}

async fn nudge(ctx: &Ctx) -> Result<()> {
    let session = session_token(ctx).await?;
    let mut a = attach(ctx, "nudge", &session, 100, 30, false).await?;
    let _ = a.collect(ctx, Duration::from_secs(1)).await;
    a.resize(ctx, 100, 30).await?;
    let same = a.collect(ctx, Duration::from_millis(2500)).await;
    check("same-size window_change sends no SIGWINCH", !same.text().contains("WINCH size=30 100"));
    a.resize(ctx, 100, 29).await?;
    a.resize(ctx, 100, 30).await?;
    let nudged = a.collect(ctx, Duration::from_secs(3)).await;
    let count = nudged.text().matches("WINCH size=").count();
    ctx.log("nudge", format!("WINCH count after 29/30 nudge: {count}"));
    check("two-step resize reaches main as SIGWINCH", count >= 1);
    a.send(ctx, &[0x04]).await?;
    let _ = a.collect(ctx, Duration::from_secs(2)).await;
    a.close(ctx).await?;
    Ok(())
}

async fn ctrl_keys(ctx: &Ctx) -> Result<()> {
    let session = session_token(ctx).await?;
    let mut a = attach(ctx, "keys", &session, 100, 30, false).await?;
    let _ = a.collect(ctx, Duration::from_secs(1)).await;
    a.send(ctx, &[0x10]).await?;
    let held = a.collect(ctx, Duration::from_millis(2500)).await;
    check("lone Ctrl-P (0x10) is not echoed by main while held", !held.stdout.contains(&0x10) && !held.text().contains("^P"));
    a.send(ctx, b"x").await?;
    let released = a.collect(ctx, Duration::from_millis(1500)).await;
    check("held Ctrl-P is released with the next byte", released.text().contains("^Px"));
    a.send(ctx, &[0x01]).await?;
    a.send(ctx, &[0x02]).await?;
    a.send(ctx, &[0x1b, b'[', b'A']).await?;
    let other = a.collect(ctx, Duration::from_millis(1500)).await;
    check("other control bytes (Ctrl-A, Ctrl-B, Up arrow) reach main", other.text().contains("^A^B^[[A"));
    a.send(ctx, &[0x04]).await?;
    let ctrl_d = a.collect(ctx, Duration::from_secs(3)).await;
    check("Ctrl-D (0x04) detaches instead of reaching main", ctrl_d.closed && ctrl_d.exit == Some(0));
    a.close(ctx).await?;
    Ok(())
}

async fn big(ctx: &Ctx) -> Result<()> {
    let session = session_token(ctx).await?;
    let mut a = attach(ctx, "big", &session, 100, 30, false).await?;
    let got = a.collect(ctx, Duration::from_secs(8)).await;
    let lines = got.lines();
    ctx.log(
        "big",
        format!(
            "replay bytes={} first line={:?} last line={:?}",
            got.stdout.len(),
            lines.first(),
            lines.last()
        ),
    );
    check("replay is capped near 1 MiB", got.stdout.len() <= 1024 * 1024 + 8192 && got.stdout.len() > 900_000);
    check("replay ends with END", got.text().contains("END"));
    check("replay does not start at line 1", lines.first().is_some_and(|l| l.trim() != "1"));
    a.send(ctx, &[0x04]).await?;
    let _ = a.collect(ctx, Duration::from_secs(3)).await;
    a.close(ctx).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, gateway, workspace, sandbox, scenario] = &args[..] else {
        bail!("usage: p06-proof <gateway> <workspace> <sandbox> <shell|shell-drop|main|concurrent|limit|exit|big|revoke|hold|probe|nudge|keys>");
    };
    let ctx = Ctx {
        client: connect(gateway).await?,
        workspace: workspace.clone(),
        sandbox: sandbox.clone(),
        started: Instant::now(),
    };
    println!("gateway={gateway} workspace={workspace} sandbox={sandbox} scenario={scenario}");
    match scenario.as_str() {
        "shell" => shell(&ctx).await,
        "shell-drop" => shell_drop(&ctx).await,
        "main" => main_flow(&ctx).await,
        "concurrent" => concurrent(&ctx).await,
        "limit" => limit(&ctx).await,
        "exit" => exit_flow(&ctx).await,
        "big" => big(&ctx).await,
        "revoke" => revoke(&ctx).await,
        "keys" => ctrl_keys(&ctx).await,
        "hold" => hold(&ctx).await,
        "probe" => probe(&ctx).await,
        "nudge" => nudge(&ctx).await,
        other => bail!("unknown scenario {other}"),
    }
}
