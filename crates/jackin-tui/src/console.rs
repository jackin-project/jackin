// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use jackin_console::app::{App, AppOptions, ClipboardMode, Entry, EntryPage, assemble};
use jackin_console::arbiter::Arbiter;
use jackin_console::world::{DaemonHealth, Snapshot, WallSource, World, WorldParts};
use jackin_core::{ConfigError, ConfigStore, EnvName, EnvSnapshot, HostPath};
use jackin_session::{HostPorts, RoleCatalog};
use junie_tui::clock::SimClock;
use junie_tui::core::event::Input;
use junie_tui::motion::Motion;
use junie_tui::runtime::{Application, ExternalReceiver, external_channel};
use junie_tui::theme::{Theme, Tone};
use thiserror::Error;

use crate::backend::JackinBackend;
use crate::bridge::{Bridge, InFlight};
use crate::clipboard::HostClipboard;
use crate::observe::{Inputs, InputsParts};
use crate::sessions::SessionSet;
use crate::unbuilt::SignedInOnly;

const NO_MOTION: &str = "JACKIN_NO_MOTION";

#[derive(Clone, Copy, Debug)]
pub struct ConsoleLimits {
    pub connect_deadline: Duration,
    pub work_reads: NonZeroUsize,
}

#[derive(Debug)]
pub struct ConsoleDeps {
    pub config: ConfigStore,
    pub sessions: SessionSet,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleExit {
    Quit,
}

pub struct Console {
    app: App,
    inflight: InFlight,
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
}

impl Console {
    pub fn app(&mut self) -> &mut App {
        &mut self.app
    }

    pub fn inflight(&self) -> usize {
        self.inflight.count()
    }

    pub async fn drain(&mut self) {
        let mut changes = self.inflight.subscribe();
        loop {
            let idle = *changes.borrow_and_update() == 0;
            self.deliver();
            if idle || changes.changed().await.is_err() {
                break;
            }
        }
    }

    fn deliver(&mut self) {
        let pending = Application::external(&self.app)
            .map(ExternalReceiver::drain)
            .unwrap_or_default();
        for message in pending {
            Application::handle(&mut self.app, Input::External(message));
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
        sessions,
        roles,
        host,
        wall,
        env,
        cwd,
        home,
        engine_socket: _,
        version,
        theme,
        handle,
        limits,
    } = deps;
    let config_path = config.path().as_path().display().to_string();
    let inputs = Arc::new(Inputs::new(InputsParts {
        config,
        sessions,
        roles,
        clock: Arc::clone(&host.clock),
        version,
        home: home.as_path().display().to_string(),
        config_path,
        limits,
    }));
    let first = inputs.observe().await?;
    let (sender, receiver) = external_channel();
    let inflight = InFlight::new();
    let backend = Arc::new(JackinBackend::new(
        Arc::clone(&inputs),
        Bridge::new(sender, inflight.clone(), handle),
    ));
    let status = first.status();
    let healthy = first.healthy;
    let Snapshot {
        workspaces,
        roles,
        instances,
        global,
        accounts,
        surfaces,
        github,
        index_error,
    } = first.snapshot;
    let mut parts = WorldParts::new(
        SimClock::new(0, 0),
        home.as_path().display().to_string(),
        cwd.as_path().display().to_string(),
        Box::new(SignedInOnly),
        backend,
        surfaces,
    )
    .wall(wall);
    parts.workspaces = workspaces;
    parts.roles = roles;
    parts.instances = instances;
    parts.global = global;
    parts.accounts = accounts;
    parts.github = github;
    parts.arbiter = Arbiter::new(0);
    if let Some(error) = index_error {
        parts.arbiter.discovery = Err(error);
    }
    let mut world = World::new(parts);
    if !healthy {
        world.daemon_health = DaemonHealth::Stale;
    }
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
            clipboard: ClipboardMode::Host(Arc::new(HostClipboard)),
        },
    )
    .with_external(receiver);
    if index_error.is_none()
        && let Some(text) = status
    {
        app.set_status(&text, Tone::Warning);
    }
    Ok(Console { app, inflight })
}
