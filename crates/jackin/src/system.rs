// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::{self, Read, Write};
use std::num::{NonZeroU8, NonZeroU16, NonZeroUsize};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    ConfigStore, EnvName, EnvSnapshot, FileSystem, GatewayEntry, GatewayName, HostPath,
    ProcessRunner, settle_critical_writes,
};
use jackin_openshell::{Gateway, GrpcGateway, GrpcGatewayConfig, TermSize, resolve_gateway};
use jackin_session::{
    ConnectError, DEFAULT_BUNDLE_BYTES, DEFAULT_EXPORT_ENTRY_BYTES, DEFAULT_EXPORT_OBJECTS,
    DEFAULT_EXPORT_TOTAL_BYTES, GatewayConnector, LaunchLimits,
};
use jackin_tui::ConsoleLimits;
use rustix::termios::{self, OptionalActions, Termios};
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::error::CommandError;
use crate::terminal::{RawMode, Terminal, TerminalInput};

const TRANSFER_TIMEOUT: Duration = Duration::from_secs(600);
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const CRITICAL_WRITE_LIMIT: Duration = Duration::from_secs(120);
const CRITICAL_WRITE_NOTICE: Duration = Duration::from_secs(1);
const READ_CHUNK: usize = 4096;
const FALLBACK_SIZE: (u16, u16) = (80, 24);
const DOCKER_SOCKET: &str = "/var/run/docker.sock";
const PANE_INBOX_BYTES: usize = 1024 * 1024;
const UNIX_SCHEME: &str = "unix://";

pub fn drive<F: Future>(future: F) -> io::Result<F::Output> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let output = runtime.block_on(future);
    runtime.block_on(finish_critical_writes(
        &mut io::stderr(),
        CRITICAL_WRITE_NOTICE,
        CRITICAL_WRITE_LIMIT,
        interrupted(),
    ));
    runtime.shutdown_timeout(SHUTDOWN_GRACE);
    Ok(output)
}

async fn interrupted() {
    let (Ok(mut interrupt), Ok(mut terminate)) = (
        signal(SignalKind::interrupt()),
        signal(SignalKind::terminate()),
    ) else {
        return std::future::pending().await;
    };
    tokio::select! {
        _ = interrupt.recv() => {}
        _ = terminate.recv() => {}
    }
}

async fn settle(limit: Duration) -> bool {
    tokio::task::spawn_blocking(move || settle_critical_writes(limit))
        .await
        .unwrap_or(false)
}

async fn finish_critical_writes(
    out: &mut impl Write,
    notice_after: Duration,
    limit: Duration,
    interrupt: impl Future<Output = ()>,
) {
    if settle_critical_writes(Duration::ZERO) {
        return;
    }
    tokio::pin!(interrupt);
    let first = tokio::select! {
        settled = settle(notice_after) => Some(settled),
        () = &mut interrupt => None,
    };
    if first == Some(true) {
        return;
    }
    if first.is_some() {
        drop(out.write_all(b"jackin: finishing a credential write (Ctrl-C abandons it)...\n"));
        let rest = tokio::select! {
            settled = settle(limit.saturating_sub(notice_after)) => Some(settled),
            () = &mut interrupt => None,
        };
        match rest {
            Some(true) => return,
            Some(false) => {
                drop(out.write_all(b"jackin: gave up waiting for a credential write\n"));
                return;
            }
            None => {}
        }
    }
    drop(out.write_all(b"jackin: abandoned the credential write\n"));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locations {
    pub config_path: HostPath,
    pub cache_dir: HostPath,
    pub openshell_config: HostPath,
    pub engine_socket: HostPath,
    pub home: HostPath,
}

fn variable(env: &EnvSnapshot, name: &str) -> Option<String> {
    EnvName::parse(name)
        .ok()
        .and_then(|name| env.get(&name).map(str::to_owned))
        .filter(|value| !value.is_empty())
}

fn absolute(env: &EnvSnapshot, name: &str) -> Result<Option<HostPath>, CommandError> {
    variable(env, name)
        .map(|value| HostPath::parse(Path::new(&value)))
        .transpose()
        .map_err(|_| CommandError::Environment(format!("{name} must be an absolute path")))
}

fn below(base: &HostPath, relative: &str) -> Result<HostPath, CommandError> {
    Ok(HostPath::resolve(Path::new(relative), base)?)
}

pub(crate) fn engine_socket(env: &EnvSnapshot) -> Result<HostPath, CommandError> {
    let socket = variable(env, "DOCKER_HOST")
        .and_then(|host| host.strip_prefix(UNIX_SCHEME).map(str::to_owned))
        .unwrap_or_else(|| DOCKER_SOCKET.to_owned());
    Ok(HostPath::parse(Path::new(&socket))?)
}

pub fn locations(env: &EnvSnapshot) -> Result<Locations, CommandError> {
    let home = home(env)?;
    let config_path = match absolute(env, "XDG_CONFIG_HOME")? {
        Some(base) => below(&base, "jackin/config.toml")?,
        None => ConfigStore::default_path(&home),
    };
    let openshell_config = match absolute(env, "XDG_CONFIG_HOME")? {
        Some(base) => below(&base, "openshell")?,
        None => below(&home, ".config/openshell")?,
    };
    let cache_dir = match absolute(env, "XDG_CACHE_HOME")? {
        Some(base) => below(&base, "jackin")?,
        None => below(&home, ".cache/jackin")?,
    };
    Ok(Locations {
        config_path,
        cache_dir,
        openshell_config,
        engine_socket: engine_socket(env)?,
        home,
    })
}

pub fn launch_limits() -> LaunchLimits {
    LaunchLimits {
        ready: Duration::from_secs(120),
        supervisor: Duration::from_secs(30),
        attempt: Duration::from_secs(30),
        hook: Duration::from_secs(600),
        upload: Duration::from_secs(600),
        create_attempts: NonZeroU8::MIN.saturating_add(2),
        bundle_bytes: DEFAULT_BUNDLE_BYTES,
        export_entry_bytes: DEFAULT_EXPORT_ENTRY_BYTES,
        export_objects: DEFAULT_EXPORT_OBJECTS,
        export_total_bytes: DEFAULT_EXPORT_TOTAL_BYTES,
    }
}

pub(crate) fn home(env: &EnvSnapshot) -> Result<HostPath, CommandError> {
    absolute(env, "HOME")?.ok_or_else(|| CommandError::Environment("HOME is not set".to_owned()))
}

pub(crate) fn console_limits() -> ConsoleLimits {
    ConsoleLimits {
        connect_deadline: Duration::from_secs(5),
        work_reads: NonZeroUsize::MIN.saturating_add(3),
        pane_inbox_bytes: NonZeroUsize::new(PANE_INBOX_BYTES).unwrap_or(NonZeroUsize::MIN),
        supervisor_poll: Duration::from_millis(250),
    }
}

#[derive(Clone, Debug)]
pub struct GrpcConnector {
    runner: Arc<dyn ProcessRunner>,
    fs: Arc<dyn FileSystem>,
    openshell_config: HostPath,
}

impl GrpcConnector {
    pub fn new(
        runner: Arc<dyn ProcessRunner>,
        fs: Arc<dyn FileSystem>,
        openshell_config: HostPath,
    ) -> GrpcConnector {
        GrpcConnector {
            runner,
            fs,
            openshell_config,
        }
    }
}

#[async_trait]
impl GatewayConnector for GrpcConnector {
    async fn connect(
        &self,
        name: &GatewayName,
        entry: &GatewayEntry,
        cancel: &CancellationToken,
    ) -> Result<Arc<dyn Gateway>, ConnectError> {
        let resolved = resolve_gateway(
            self.runner.as_ref(),
            self.fs.as_ref(),
            &self.openshell_config,
            name,
            cancel,
        )
        .await
        .map_err(|source| ConnectError::Discovery {
            gateway: name.clone(),
            source,
        })?;
        let gateway = GrpcGateway::connect(GrpcGatewayConfig {
            gateway: resolved,
            workspace: entry.openshell_workspace.clone(),
            runner: Arc::clone(&self.runner),
            transfer_timeout: TRANSFER_TIMEOUT,
            call_timeout: CALL_TIMEOUT,
            cancel: cancel.clone(),
        })
        .await?;
        Ok(Arc::new(gateway))
    }
}

#[derive(Debug)]
struct Restore(Termios);

impl Drop for Restore {
    fn drop(&mut self) {
        let outcome = termios::tcsetattr(io::stdin(), OptionalActions::Now, &self.0);
        if outcome.is_err() {
            drop(io::stderr().write_all(b"jackin: cannot restore the terminal\n"));
        }
    }
}

fn pump_stdin(sender: &mpsc::UnboundedSender<Vec<u8>>) {
    let mut stdin = io::stdin().lock();
    let mut buffer = [0_u8; READ_CHUNK];
    while let Ok(count) = stdin.read(&mut buffer) {
        if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
            break;
        }
    }
}

type Chunks = mpsc::UnboundedReceiver<Vec<u8>>;

fn reading(slot: &mut Option<Chunks>) -> &mut Chunks {
    slot.get_or_insert_with(|| {
        let (sender, chunks) = mpsc::unbounded_channel();
        std::thread::spawn(move || pump_stdin(&sender));
        chunks
    })
}

#[derive(Debug)]
pub struct SystemTerminal {
    chunks: Option<Chunks>,
    resizes: Signal,
    pending: Vec<u8>,
}

impl SystemTerminal {
    pub fn open() -> io::Result<Option<SystemTerminal>> {
        if !termios::isatty(io::stdin()) || !termios::isatty(io::stdout()) {
            return Ok(None);
        }
        let resizes = signal(SignalKind::window_change())?;
        Ok(Some(SystemTerminal {
            chunks: None,
            resizes,
            pending: Vec::new(),
        }))
    }
}

#[async_trait]
impl Terminal for SystemTerminal {
    fn size(&self) -> TermSize {
        let (cols, rows) = termios::tcgetwinsize(io::stdout())
            .map_or(FALLBACK_SIZE, |size| (size.ws_col, size.ws_row));
        let fallback = |value: u16, default: u16| {
            NonZeroU16::new(value)
                .or_else(|| NonZeroU16::new(default))
                .unwrap_or(NonZeroU16::MIN)
        };
        TermSize {
            cols: fallback(cols, FALLBACK_SIZE.0),
            rows: fallback(rows, FALLBACK_SIZE.1),
        }
    }

    fn enter_raw(&mut self) -> io::Result<RawMode> {
        let saved = termios::tcgetattr(io::stdin())?;
        let mut raw = saved.clone();
        raw.make_raw();
        termios::tcsetattr(io::stdin(), OptionalActions::Now, &raw)?;
        Ok(RawMode::new(Restore(saved)))
    }

    async fn read_line(&mut self) -> io::Result<Option<String>> {
        loop {
            if let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=end).collect();
                let text = String::from_utf8_lossy(&line);
                return Ok(Some(text.trim_end_matches(['\r', '\n']).to_owned()));
            }
            match reading(&mut self.chunks).recv().await {
                Some(chunk) => self.pending.extend(chunk),
                None if self.pending.is_empty() => return Ok(None),
                None => {
                    let rest = std::mem::take(&mut self.pending);
                    return Ok(Some(String::from_utf8_lossy(&rest).into_owned()));
                }
            }
        }
    }

    async fn next_input(&mut self) -> TerminalInput {
        if !self.pending.is_empty() {
            return TerminalInput::Bytes(std::mem::take(&mut self.pending));
        }
        tokio::select! {
            chunk = reading(&mut self.chunks).recv() => match chunk {
                Some(bytes) => TerminalInput::Bytes(bytes),
                None => TerminalInput::Closed,
            },
            _ = self.resizes.recv() => TerminalInput::Resized(self.size()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Instant;

    use jackin_core::CriticalWrite;

    use super::*;

    const NOTICE: &str = "jackin: finishing a credential write (Ctrl-C abandons it)...\n";

    fn pause(length: Duration) {
        let (_hold, wait) = mpsc::channel::<()>();
        let _timed_out = wait.recv_timeout(length).is_err();
    }

    async fn report(
        notice_after: Duration,
        limit: Duration,
        interrupt: impl Future<Output = ()>,
    ) -> String {
        let mut out = Vec::new();
        finish_critical_writes(&mut out, notice_after, limit, interrupt).await;
        String::from_utf8_lossy(&out).into_owned()
    }

    #[tokio::test]
    async fn nothing_is_printed_when_no_write_is_in_flight() {
        let text = report(
            Duration::from_millis(50),
            Duration::from_secs(5),
            std::future::pending(),
        )
        .await;
        assert_eq!(text, "");
    }

    #[tokio::test]
    async fn a_slow_write_prints_one_notice_and_no_give_up_line() {
        let write = CriticalWrite::begin();
        let releaser = std::thread::spawn(move || {
            pause(Duration::from_millis(300));
            drop(write);
        });
        let text = report(
            Duration::from_millis(50),
            Duration::from_secs(10),
            std::future::pending(),
        )
        .await;
        assert_eq!(text, NOTICE);
        assert_eq!(releaser.join().ok(), Some(()));
    }

    #[tokio::test]
    async fn a_write_that_never_ends_is_reported_as_given_up() {
        let _stuck = CriticalWrite::begin();
        let text = report(
            Duration::from_millis(20),
            Duration::from_millis(200),
            std::future::pending(),
        )
        .await;
        assert_eq!(
            text,
            format!("{NOTICE}jackin: gave up waiting for a credential write\n")
        );
    }

    #[tokio::test]
    async fn an_interrupt_during_the_wait_abandons_the_write_promptly() {
        let _stuck = CriticalWrite::begin();
        let started = Instant::now();
        let text = report(
            Duration::from_millis(20),
            Duration::from_secs(60),
            tokio::time::sleep(Duration::from_millis(150)),
        )
        .await;
        assert_eq!(
            text,
            format!("{NOTICE}jackin: abandoned the credential write\n")
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn an_interrupt_before_the_notice_still_abandons_the_write() {
        let _stuck = CriticalWrite::begin();
        let text = report(
            Duration::from_secs(60),
            Duration::from_secs(120),
            std::future::ready(()),
        )
        .await;
        assert_eq!(text, "jackin: abandoned the credential write\n");
    }

    #[tokio::test]
    async fn a_real_sigint_during_the_wait_abandons_the_write() {
        let _stuck = CriticalWrite::begin();
        let pid = std::process::id().to_string();
        let sender = std::thread::spawn(move || {
            pause(Duration::from_millis(300));
            std::process::Command::new("kill")
                .args(["-INT", &pid])
                .status()
                .map(|status| status.success())
                .ok()
        });
        let started = Instant::now();
        let text = tokio::time::timeout(
            Duration::from_secs(5),
            report(
                Duration::from_millis(20),
                Duration::from_secs(60),
                interrupted(),
            ),
        )
        .await
        .unwrap_or_default();
        assert_eq!(
            text,
            format!("{NOTICE}jackin: abandoned the credential write\n")
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(sender.join().ok().flatten(), Some(true));
    }
}
