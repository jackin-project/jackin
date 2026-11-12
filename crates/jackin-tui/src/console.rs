// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jackin_console::app::{App, AppOptions, ClipboardMode, Entry, EntryPage, assemble};
use jackin_console::arbiter::Arbiter;
use jackin_console::panes::PanePolicy;
use jackin_console::world::{Snapshot, WallSource, World, WorldParts};
use jackin_core::{
    Compatibility, CompatibilityError, ConfigError, ConfigStore, EnvName, EnvSnapshot, HostPath,
};
use jackin_session::{AccountDesk, HostPorts, RoleCatalog};
use jackin_usage::HostUsage;
use junie_tui::clock::SimClock;
use junie_tui::core::event::Input;
use junie_tui::motion::Motion;
use junie_tui::runtime::{Application, ExternalReceiver, external_channel};
use junie_tui::shell::NativeClipboard;
use junie_tui::shell::StopHandle;
use junie_tui::theme::{ColorLevel, Theme, Tone};
use thiserror::Error;
use tokio::sync::oneshot;
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::backend::JackinBackend;
use crate::bridge::{Bridge, InFlight};
use crate::observe::{Inputs, InputsParts};
use crate::onepassword::OpSnapshot;
use crate::panes::PaneHub;
use crate::sessions::{SessionConnector, SessionSet};

const NO_MOTION: &str = "JACKIN_NO_MOTION";
const CLOSE_DEADLINE: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug)]
pub struct ConsoleLimits {
    pub connect_deadline: Duration,
    pub work_reads: NonZeroUsize,
    pub pane_inbox_bytes: NonZeroUsize,
    pub supervisor_poll: Duration,
}

#[derive(Debug)]
pub struct ConsoleDeps {
    pub config: ConfigStore,
    pub desk: AccountDesk,
    pub usage: Arc<HostUsage>,
    pub sessions: SessionSet,
    pub connect: Arc<dyn SessionConnector>,
    pub roles: Arc<RoleCatalog>,
    pub host: HostPorts,
    pub wall: Arc<dyn WallSource>,
    pub env: EnvSnapshot,
    pub cwd: HostPath,
    pub home: HostPath,
    pub engine_socket: HostPath,
    pub version: String,
    pub theme: Theme,
    pub handle: tokio::runtime::Handle,
    pub limits: ConsoleLimits,
    pub clipboard: Arc<dyn NativeClipboard>,
    pub cancel: CancellationToken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleExit {
    Quit,
}

pub struct Console {
    app: Box<App>,
    inflight: InFlight,
    panes: PaneHub,
    _cancel: DropGuard,
}

impl fmt::Debug for Console {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Console").finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum ConsoleError {
    #[error("terminal error: {0}")]
    Terminal(std::io::Error),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Pinned(#[from] CompatibilityError),
    #[error("the console was cancelled")]
    Cancelled,
    #[error("the console stopped before it could quit")]
    Panicked,
}

#[async_trait]
pub trait ConsoleHost: Send + Sync + fmt::Debug {
    async fn run(&self, deps: ConsoleDeps) -> Result<ConsoleExit, ConsoleError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TerminalConsole;

#[async_trait]
impl ConsoleHost for TerminalConsole {
    async fn run(&self, deps: ConsoleDeps) -> Result<ConsoleExit, ConsoleError> {
        run_console(deps).await
    }
}

pub async fn run_console(deps: ConsoleDeps) -> Result<ConsoleExit, ConsoleError> {
    run_console_with(deps, junie_tui::shell::run).await
}

pub async fn run_console_with<F>(
    deps: ConsoleDeps,
    terminal: F,
) -> Result<ConsoleExit, ConsoleError>
where
    F: FnOnce(&mut App) -> std::io::Result<()> + Send + 'static,
{
    let handle = deps.handle.clone();
    let cancelled = deps.cancel.clone();
    let watched = cancelled.clone();
    let building = cancelled.clone();
    let (sender, receiver) = oneshot::channel::<StopHandle>();
    let watcher = tokio::spawn(async move {
        if let Ok(stop) = receiver.await {
            watched.cancelled().await;
            stop.stop();
        }
    });
    let (panes, cancel, outcome) = tokio::task::spawn_blocking(move || {
        let Console {
            mut app,
            panes,
            _cancel: cancel,
            ..
        } = handle.block_on(async {
            tokio::select! {
                biased;
                () = building.cancelled() => Err(ConsoleError::Cancelled),
                built = build_console(deps) => built,
            }
        })?;
        let _sent = sender.send(app.shell.stop_handle());
        let outcome = terminal(&mut app);
        drop(app);
        Ok::<_, ConsoleError>((panes, cancel, outcome))
    })
    .await
    .map_err(|_| ConsoleError::Panicked)??;
    watcher.abort();
    panes.finished(CLOSE_DEADLINE).await;
    drop(cancel);
    outcome.map_err(ConsoleError::Terminal)?;
    if cancelled.is_cancelled() {
        return Err(ConsoleError::Cancelled);
    }
    Ok(ConsoleExit::Quit)
}

pub fn system_theme() -> Theme {
    Theme::for_level(ColorLevel::detect())
}

impl Console {
    pub fn app(&mut self) -> &mut App {
        &mut self.app
    }

    pub fn inflight(&self) -> usize {
        self.inflight.count()
    }

    pub fn pane_tasks(&self) -> usize {
        self.panes.live()
    }

    pub async fn close_panes(&mut self, deadline: Duration) -> bool {
        self.app.world.daemons.clear();
        self.panes.finished(deadline).await
    }

    pub async fn drain(&mut self) {
        let mut changes = self.inflight.subscribe();
        loop {
            let idle = *changes.borrow_and_update() == 0;
            self.deliver();
            if idle && *changes.borrow() == 0 {
                break;
            }
            if !idle && changes.changed().await.is_err() {
                break;
            }
        }
    }

    fn deliver(&mut self) {
        let pending = Application::external(self.app.as_ref())
            .map(ExternalReceiver::drain)
            .unwrap_or_default();
        for message in pending {
            Application::handle(self.app.as_mut(), Input::External(message));
        }
    }
}

fn reduced_motion(env: &EnvSnapshot) -> bool {
    EnvName::parse(NO_MOTION)
        .ok()
        .and_then(|name| env.get(&name).map(str::to_owned))
        .is_some_and(|value| !value.trim_start_matches('0').is_empty())
}

pub async fn build_console(deps: ConsoleDeps) -> Result<Console, ConsoleError> {
    let ConsoleDeps {
        config,
        desk,
        usage,
        sessions,
        connect,
        roles,
        host,
        wall,
        env,
        cwd,
        home,
        engine_socket,
        version,
        theme,
        handle,
        limits,
        clipboard,
        cancel: external,
    } = deps;
    let config_path = config.path().as_path().display().to_string();
    let cancel = external.child_token();
    let op = OpSnapshot::default();
    let inputs = Arc::new(Inputs::new(InputsParts {
        config,
        desk,
        usage,
        op: op.clone(),
        env: env.clone(),
        sessions,
        roles,
        host,
        connect,
        wall: Arc::clone(&wall),
        engine_socket,
        pinned: Compatibility::pinned()?,
        cancel: cancel.clone(),
        version,
        home: home.as_path().display().to_string(),
        config_path,
        limits,
    }));
    inputs.doctor_at_start().await;
    let first = inputs.observe().await?;
    Ok(assemble_console(ConsoleParts {
        inputs,
        first,
        op,
        cancel,
        env,
        wall,
        home,
        cwd,
        theme,
        handle,
        limits,
        clipboard,
    }))
}

struct ConsoleParts {
    inputs: Arc<Inputs>,
    first: crate::observe::Observation,
    op: OpSnapshot,
    cancel: CancellationToken,
    env: EnvSnapshot,
    wall: Arc<dyn WallSource>,
    home: HostPath,
    cwd: HostPath,
    theme: Theme,
    handle: tokio::runtime::Handle,
    limits: ConsoleLimits,
    clipboard: Arc<dyn NativeClipboard>,
}

fn assemble_console(parts: ConsoleParts) -> Console {
    let ConsoleParts {
        inputs,
        first,
        op,
        cancel,
        env,
        wall,
        home,
        cwd,
        theme,
        handle,
        limits,
        clipboard,
    } = parts;
    let (sender, receiver) = external_channel();
    let inflight = InFlight::new();
    let bridge = Bridge::new(sender, inflight.clone(), handle.clone());
    let panes = PaneHub::new(
        Arc::clone(&inputs),
        handle,
        bridge.clone(),
        limits.pane_inbox_bytes.get(),
        limits.supervisor_poll,
    );
    let backend = Arc::new(JackinBackend::new(
        Arc::clone(&inputs),
        bridge,
        first.snapshot.surfaces.vocabulary.clone(),
        panes.clone(),
    ));
    let status = first.status(&first.snapshot.surfaces.vocabulary);
    let Snapshot {
        workspaces,
        roles,
        instances,
        global,
        accounts,
        surfaces,
        github,
        index_error,
        daemon_health,
    } = first.snapshot;
    let mut world_parts = WorldParts::new(
        SimClock::new(0, 0),
        home.as_path().display().to_string(),
        cwd.as_path().display().to_string(),
        Box::new(op),
        backend,
        surfaces,
    )
    .wall(wall);
    world_parts.workspaces = workspaces;
    world_parts.roles = roles;
    world_parts.instances = instances;
    world_parts.global = global;
    world_parts.accounts = accounts;
    world_parts.github = github;
    world_parts.arbiter = Arbiter::new(0);
    if let Some(error) = index_error {
        world_parts.arbiter.discovery = Err(error);
    }
    let mut world = World::new(world_parts);
    world.daemon_health = daemon_health;
    let mut app = assemble(
        world,
        Entry {
            page: EntryPage::Manager,
            outro_when_framed: false,
        },
        AppOptions {
            motion: Motion::resolve(None, reduced_motion(&env)),
            frame: 0,
            theme,
            clipboard: ClipboardMode::Host(clipboard),
            panes: PanePolicy::default(),
        },
    )
    .with_external(receiver);
    if index_error.is_none()
        && let Some(text) = status
    {
        app.set_status(&text, Tone::Warning);
    }
    Console {
        app: Box::new(app),
        inflight,
        panes,
        _cancel: cancel.drop_guard(),
    }
}
