// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use jackin_console::domain::fixed::Fixed;
use jackin_console::world::WallSource;
use jackin_core::{Clock, ConfigStore, EnvSnapshot, GatewayName, HostPath, ProcessRunner};
use jackin_image::ImageEngine;
use jackin_openshell::Gateway;
use jackin_session::{GatewayConnector, HostPorts, Session, SessionDeps};
use jackin_tui::{
    Console, ConsoleDeps, ConsoleLimits, DefaultGateway, GatewaySessions, GatewayState,
    SessionConnector, SessionSet, build_console,
};
use jackin_usage::{HostUsage, UsageDeps};
use junie_tui::core::event::{Input, Key, Mouse, MouseKind};
use junie_tui::runtime::Application;
use junie_tui::shell::NativeClipboard;
use junie_tui::theme::{ColorLevel, Theme, Tone};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Position;

use crate::connector::ScriptedConnector;
use crate::doctor_host::DoctorHost;
use crate::fixtures::FixtureError;
use crate::gateway::FakeGateway;
use crate::host::FakeClock;
use crate::world::World;

const USAGE_DIR: &str = "usage";
const SETTLE_STALL: Duration = Duration::from_secs(5);
const WAIT_STEPS: usize = 1000;
const WAIT_STEP: Duration = Duration::from_millis(5);
const DEFAULT_SIZE: (u16, u16) = (120, 40);

#[derive(Debug)]
pub struct FakeWall {
    clock: Arc<FakeClock>,
    extra_ms: AtomicI64,
}

impl FakeWall {
    pub fn new(clock: Arc<FakeClock>) -> FakeWall {
        FakeWall {
            clock,
            extra_ms: AtomicI64::new(0),
        }
    }

    pub fn advance(&self, ms: i64) {
        let total = self.extra_ms.fetch_add(ms, Ordering::SeqCst) + ms;
        let whole = total.div_euclid(1000);
        self.extra_ms.fetch_sub(whole * 1000, Ordering::SeqCst);
        self.clock
            .advance(Duration::from_secs(whole.unsigned_abs()));
    }
}

impl WallSource for FakeWall {
    fn epoch_millis(&self) -> i64 {
        self.clock.now().0 * 1000 + self.extra_ms.load(Ordering::SeqCst)
    }

    fn utc_offset_secs(&self) -> i64 {
        i64::from(self.clock.utc_offset_secs())
    }
}

#[derive(Debug)]
pub struct RecordingClipboard {
    accepting: AtomicBool,
    writes: Mutex<Vec<String>>,
}

impl RecordingClipboard {
    pub fn new(accepting: bool) -> Arc<RecordingClipboard> {
        Arc::new(RecordingClipboard {
            accepting: AtomicBool::new(accepting),
            writes: Mutex::new(Vec::new()),
        })
    }

    pub fn writes(&self) -> Vec<String> {
        crate::sync::locked(&self.writes).clone()
    }
}

impl NativeClipboard for RecordingClipboard {
    fn write(&self, text: &str) -> bool {
        crate::sync::locked(&self.writes).push(text.to_owned());
        self.accepting.load(Ordering::SeqCst)
    }
}

#[derive(Debug)]
pub struct ConsoleRigBuilder<'a> {
    world: &'a World,
    extra: BTreeMap<GatewayName, GatewayState>,
    connector: Option<Arc<dyn SessionConnector>>,
    host: DoctorHost,
    engine_socket: Option<HostPath>,
    cwd: Option<HostPath>,
    deadline: Duration,
    env: Vec<(String, String)>,
    size: (u16, u16),
    clipboard: Arc<RecordingClipboard>,
    pane_inbox_bytes: usize,
    unchosen: bool,
    pinned: Option<GatewayName>,
}

pub struct ConsoleRig {
    console: Console,
    wall: Arc<FakeWall>,
    config: ConfigStore,
    gateway: Arc<FakeGateway>,
    size: (u16, u16),
    theme: Theme,
    clipboard: Arc<RecordingClipboard>,
}

impl fmt::Debug for ConsoleRig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsoleRig").finish_non_exhaustive()
    }
}

pub fn session_over(world: &World, gateway: Arc<dyn Gateway>) -> Session {
    session_maker(world)(gateway)
}

pub fn session_maker(world: &World) -> impl Fn(Arc<dyn Gateway>) -> Session + Send + Sync + use<> {
    let engine: Arc<dyn ImageEngine> = Arc::<crate::engine::FakeEngine>::clone(&world.engine);
    let clock: Arc<dyn Clock> = Arc::<FakeClock>::clone(&world.clock);
    let fs = Arc::clone(&world.fs);
    let runner: Arc<dyn ProcessRunner> = Arc::<crate::host::TestRunner>::clone(&world.runner);
    let cache_dir = world.cache_dir().clone();
    let config = world.config.clone();
    move |gateway| {
        Session::new(SessionDeps {
            gateway,
            engine: Arc::clone(&engine),
            host: HostPorts {
                clock: Arc::clone(&clock),
                fs: Arc::clone(&fs),
                runner: Arc::clone(&runner),
            },
            cache_dir: cache_dir.clone(),
            config: config.clone(),
            limits: crate::world::default_limits(),
        })
    }
}

pub fn connector_over(
    world: &World,
    gateways: Arc<dyn GatewayConnector>,
) -> Arc<dyn SessionConnector> {
    Arc::new(GatewaySessions::new(gateways, session_maker(world)))
}

impl ConsoleRig {
    pub fn builder(world: &World) -> ConsoleRigBuilder<'_> {
        ConsoleRigBuilder {
            world,
            extra: BTreeMap::new(),
            connector: None,
            host: DoctorHost::healthy(),
            engine_socket: None,
            cwd: None,
            deadline: Duration::from_secs(10),
            env: Vec::new(),
            size: DEFAULT_SIZE,
            clipboard: RecordingClipboard::new(true),
            pane_inbox_bytes: 1024 * 1024,
            unchosen: false,
            pinned: None,
        }
    }

    pub async fn new(world: &World) -> Result<ConsoleRig, FixtureError> {
        ConsoleRig::builder(world).build().await
    }

    pub async fn seeded(rig: &crate::Rig) -> Result<ConsoleRig, FixtureError> {
        rig.world.seed_all(&rig.config).await?;
        ConsoleRig::new(&rig.world).await
    }

    pub fn gateway(&self) -> &Arc<FakeGateway> {
        &self.gateway
    }

    pub fn clipboard(&self) -> &Arc<RecordingClipboard> {
        &self.clipboard
    }

    pub async fn wait_for_text(&mut self, text: &str) -> Result<String, FixtureError> {
        for _ in 0..WAIT_STEPS {
            let screen = self.screen();
            if screen.contains(text) {
                return Ok(screen);
            }
            self.tick(1);
            tokio::time::sleep(WAIT_STEP).await;
        }
        Err(format!("{text:?} never appeared:\n{}", self.screen()).into())
    }

    pub async fn pump(&mut self, ticks: usize) {
        for _ in 0..ticks {
            self.tick(1);
            tokio::time::sleep(WAIT_STEP).await;
        }
    }

    pub fn config(&self) -> &ConfigStore {
        &self.config
    }

    pub fn console(&mut self) -> &mut Console {
        &mut self.console
    }

    pub fn reasons(&self) -> Fixed {
        Fixed::openshell()
    }

    pub async fn config_text(&self) -> Result<String, FixtureError> {
        Ok(self.config.load().await?.render())
    }

    fn input(&mut self, input: Input) {
        Application::handle(self.console.app(), input);
        self.render();
    }

    pub fn keys(&mut self, spec: &str) -> Result<(), FixtureError> {
        for word in spec.split_whitespace() {
            let key = parse_key(word).ok_or_else(|| format!("unknown key {word}"))?;
            self.input(Input::Key(key));
        }
        Ok(())
    }

    pub fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.input(Input::Key(Key {
                code: KeyCode::Char(c),
                mods: KeyModifiers::NONE,
            }));
        }
    }

    pub fn skip_intro(&mut self) {
        for _ in 0..4 {
            if self.console.app().route() != jackin_console::app::Route::Intro {
                return;
            }
            self.tick(4);
            self.input(Input::Key(Key {
                code: KeyCode::Enter,
                mods: KeyModifiers::NONE,
            }));
        }
    }

    pub fn paste(&mut self, text: &str) {
        self.input(Input::Paste(text.to_owned()));
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.size = (width, height);
        self.input(Input::Resize(width, height));
    }

    pub fn click(&mut self, x: u16, y: u16) {
        let at = Position::new(x, y);
        self.input(Input::Mouse(Mouse::new(MouseKind::Down, at)));
        self.input(Input::Mouse(Mouse::new(MouseKind::Up, at)));
    }

    pub fn click_text(&mut self, text: &str) -> Result<(), FixtureError> {
        let (x, y) = self
            .find(text)
            .ok_or_else(|| format!("{text:?} is not on the screen:\n{}", self.screen()))?;
        self.click(x, y);
        Ok(())
    }

    fn find(&mut self, text: &str) -> Option<(u16, u16)> {
        let screen = self.screen();
        screen.lines().enumerate().find_map(|(row, line)| {
            let byte = line.find(text)?;
            let column = line[..byte].chars().count();
            Some((u16::try_from(column).ok()?, u16::try_from(row).ok()?))
        })
    }

    pub fn tick(&mut self, count: usize) {
        for _ in 0..count {
            let interval = Application::tick_interval(self.console.app());
            self.wall
                .advance(i64::try_from(interval.as_millis()).unwrap_or(i64::MAX));
            self.input(Input::Tick);
        }
    }

    pub fn elapse(&mut self, secs: u64) {
        self.wall
            .advance(i64::try_from(secs.saturating_mul(1000)).unwrap_or(i64::MAX));
    }

    pub async fn settle(&mut self) -> Result<(), FixtureError> {
        let clock = Arc::clone(&self.wall.clock);
        tokio::select! {
            () = self.console.drain() => {}
            () = clock.until_stalled(SETTLE_STALL) => {
                return Err("the console made no progress for 5 s".into());
            }
        }
        self.render();
        Ok(())
    }

    pub fn screen(&mut self) -> String {
        self.screen_at(self.size.0, self.size.1)
    }

    pub fn screen_at(&mut self, width: u16, height: u16) -> String {
        let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
            Ok(terminal) => terminal,
            Err(error) => return error.to_string(),
        };
        let app = self.console.app();
        if let Err(error) = terminal.draw(|frame| Application::render(app, frame)) {
            return error.to_string();
        }
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .chunks(usize::from(buffer.area.width))
            .map(|row| {
                row.iter()
                    .map(ratatui::buffer::Cell::symbol)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render(&mut self) {
        self.screen();
    }

    pub fn status_line(&mut self) -> String {
        self.console
            .app()
            .shell
            .status
            .get()
            .map(|(text, _)| text.to_owned())
            .unwrap_or_default()
    }

    pub fn status_is_neutral(&mut self) -> bool {
        self.console
            .app()
            .shell
            .status
            .get()
            .is_some_and(|(_, tone)| tone != Tone::Error && tone != Tone::Warning)
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }
}

impl ConsoleRigBuilder<'_> {
    #[must_use]
    pub fn gateway(mut self, name: GatewayName, state: GatewayState) -> Self {
        self.extra.insert(name, state);
        self
    }

    #[must_use]
    pub fn connector(mut self, connector: Arc<dyn SessionConnector>) -> Self {
        self.connector = Some(connector);
        self
    }

    #[must_use]
    pub fn host(mut self, host: DoctorHost) -> Self {
        self.host = host;
        self
    }

    #[must_use]
    pub fn engine_socket(mut self, path: HostPath) -> Self {
        self.engine_socket = Some(path);
        self
    }

    #[must_use]
    pub fn cwd(mut self, path: HostPath) -> Self {
        self.cwd = Some(path);
        self
    }

    #[must_use]
    pub fn unchosen(mut self) -> Self {
        self.unchosen = true;
        self
    }

    #[must_use]
    pub fn pinned(mut self, name: GatewayName) -> Self {
        self.pinned = Some(name);
        self
    }

    #[must_use]
    pub fn deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    #[must_use]
    pub fn env(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.to_owned(), value.to_owned()));
        self
    }

    #[must_use]
    pub fn clipboard(mut self, clipboard: Arc<RecordingClipboard>) -> Self {
        self.clipboard = clipboard;
        self
    }

    #[must_use]
    pub fn pane_inbox_bytes(mut self, bytes: usize) -> Self {
        self.pane_inbox_bytes = bytes;
        self
    }

    #[must_use]
    pub fn size(mut self, width: u16, height: u16) -> Self {
        self.size = (width, height);
        self
    }

    pub async fn build(self) -> Result<ConsoleRig, FixtureError> {
        let Prepared {
            deps,
            wall,
            config,
            gateway,
            size,
            theme,
            clipboard,
        } = self.prepare().await?;
        let console = build_console(deps).await?;
        let mut rig = ConsoleRig {
            console,
            wall,
            config,
            gateway,
            size,
            theme,
            clipboard,
        };
        rig.render();
        Ok(rig)
    }

    pub async fn deps(self) -> Result<ConsoleDeps, FixtureError> {
        Ok(self.prepare().await?.deps)
    }

    async fn prepare(self) -> Result<Prepared, FixtureError> {
        let world = self.world;
        let config = world.config.load().await?;
        let default =
            match self.pinned {
                Some(name) => DefaultGateway::Pinned(name),
                None => DefaultGateway::Followed((!self.unchosen).then(|| {
                    match config.resolve_gateway(None) {
                        Ok((name, _)) => name.clone(),
                        Err(_) => world.gateway.name().clone(),
                    }
                })),
            };
        let mut entries = self.extra;
        if !entries.contains_key(world.gateway.name()) {
            let gateway: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&world.gateway);
            entries.insert(
                world.gateway.name().clone(),
                GatewayState::Connected(Box::new(session_over(world, gateway))),
            );
        }
        self.host.apply(world)?;
        let server = world.usage.get().await?;
        let usage = Arc::new(HostUsage::new(UsageDeps {
            clock: Arc::<FakeClock>::clone(&world.clock),
            fs: Arc::clone(&world.fs),
            runner: Arc::<crate::host::TestRunner>::clone(&world.runner),
            env: EnvSnapshot::from_pairs(self.env.clone()),
            endpoints: server.endpoints(),
            state_dir: HostPath::resolve(Path::new(USAGE_DIR), world.cache_dir())?,
        }));
        let connect = self.connector.unwrap_or_else(|| {
            let gateway: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&world.gateway);
            connector_over(
                world,
                Arc::new(ScriptedConnector::new().gateway(gateway, Vec::new())),
            )
        });
        let wall = Arc::new(FakeWall::new(Arc::clone(&world.clock)));
        let wall_port: Arc<dyn WallSource> = Arc::<FakeWall>::clone(&wall);
        let clock: Arc<dyn Clock> = Arc::<FakeClock>::clone(&world.clock);
        let theme = Theme::for_level(ColorLevel::TrueColor);
        let sink: Arc<dyn NativeClipboard> = Arc::<RecordingClipboard>::clone(&self.clipboard);
        let deps = ConsoleDeps {
            config: world.config.clone(),
            desk: world.desk(),
            usage,
            sessions: SessionSet::new(default, entries),
            connect,
            roles: Arc::clone(&world.roles),
            host: HostPorts {
                clock,
                fs: Arc::clone(&world.fs),
                runner: Arc::<crate::host::TestRunner>::clone(&world.runner),
            },
            wall: wall_port,
            env: EnvSnapshot::from_pairs(self.env),
            cwd: self.cwd.unwrap_or_else(|| world.home().clone()),
            home: world.home().clone(),
            engine_socket: self.engine_socket.unwrap_or_else(|| world.home().clone()),
            version: "0.0.0-test".to_owned(),
            theme,
            handle: tokio::runtime::Handle::current(),
            limits: ConsoleLimits {
                connect_deadline: self.deadline,
                work_reads: NonZeroUsize::MIN.saturating_add(3),
                pane_inbox_bytes: NonZeroUsize::new(self.pane_inbox_bytes)
                    .unwrap_or(NonZeroUsize::MIN),
                supervisor_poll: Duration::from_millis(20),
            },
            clipboard: sink,
        };
        Ok(Prepared {
            deps,
            wall,
            config: world.config.clone(),
            gateway: Arc::clone(&world.gateway),
            size: self.size,
            theme,
            clipboard: self.clipboard,
        })
    }
}

struct Prepared {
    deps: ConsoleDeps,
    wall: Arc<FakeWall>,
    config: ConfigStore,
    gateway: Arc<FakeGateway>,
    size: (u16, u16),
    theme: Theme,
    clipboard: Arc<RecordingClipboard>,
}

fn parse_key(word: &str) -> Option<Key> {
    let mut mods = KeyModifiers::NONE;
    let mut rest = word;
    loop {
        if let Some(tail) = rest.strip_prefix("ctrl+") {
            mods |= KeyModifiers::CONTROL;
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("alt+") {
            mods |= KeyModifiers::ALT;
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("shift+") {
            mods |= KeyModifiers::SHIFT;
            rest = tail;
        } else {
            break;
        }
    }
    let code = match rest {
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" if mods.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        function if function.starts_with('F') && function.len() > 1 => {
            KeyCode::F(function[1..].parse().ok()?)
        }
        single => {
            let mut chars = single.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            if c.is_uppercase() {
                mods |= KeyModifiers::SHIFT;
            }
            KeyCode::Char(c)
        }
    };
    Some(Key { code, mods })
}
