// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::fmt::Debug;
use std::io::{self, Write};
use std::num::NonZeroU16;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use jackin::{AppEnv, Deps, ExitClass, GrpcConnector, RawMode, Terminal, TerminalInput};
use jackin_core::{
    Clock, Config, ConfigStore, EnvSnapshot, FileSystem, GatewayEntry, GatewayName, HostPath,
    ProcessRunner, Provider,
};
use jackin_image::ImageEngine;
use jackin_openshell::{Gateway, GatewayError, TermSize};
use jackin_session::{ConnectError, GatewayConnector, HostPorts};
use jackin_testkit::{ProcessReply, ScriptedProgram, World, default_limits};
use jackin_usage::ProviderEndpoints;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

use super::console::ScriptedConsole;

pub(crate) type Failure = Box<dyn std::error::Error>;

#[derive(Clone, Debug, Default)]
pub(crate) struct Shared(Arc<Mutex<Vec<u8>>>);

impl Shared {
    pub(crate) fn text(&self) -> String {
        match self.0.lock() {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(poisoned) => String::from_utf8_lossy(&poisoned.into_inner()).into_owned(),
        }
    }
}

impl Write for Shared {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self.0.lock() {
            Ok(mut inner) => inner.extend_from_slice(bytes),
            Err(poisoned) => poisoned.into_inner().extend_from_slice(bytes),
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct Ran {
    pub(crate) exit: ExitClass,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl Ran {
    pub(crate) fn value(&self) -> Result<Value, Failure> {
        let body = self
            .stdout
            .strip_suffix('\n')
            .ok_or("stdout has no newline")?;
        Ok(serde_json::from_str(body)?)
    }

    pub(crate) fn json<T: DeserializeOwned>(&self) -> Result<T, Failure> {
        Ok(serde_json::from_value(self.value()?)?)
    }

    pub(crate) fn ok(&self) -> Result<(), Failure> {
        if self.exit == ExitClass::Ok {
            Ok(())
        } else {
            Err(format!("exit {:?}, stderr: {}", self.exit, self.stderr).into())
        }
    }
}

pub(crate) fn failed(ran: &Ran, class: ExitClass, names: &[&str]) -> Result<(), Failure> {
    if ran.exit != class {
        return Err(format!(
            "exit {:?}, wanted {class:?}; stderr: {}",
            ran.exit, ran.stderr
        )
        .into());
    }
    if !ran.stdout.is_empty() {
        return Err(format!("stdout is not empty: {}", ran.stdout).into());
    }
    for name in names {
        if !ran.stderr.contains(name) {
            return Err(format!("stderr lacks {name}: {}", ran.stderr).into());
        }
    }
    Ok(())
}

pub(crate) fn keys(value: &Value) -> Result<Vec<String>, Failure> {
    let mut names: Vec<String> = value
        .as_object()
        .ok_or("not an object")?
        .keys()
        .cloned()
        .collect();
    names.sort();
    Ok(names)
}

pub(crate) fn names(wanted: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = wanted.iter().map(|name| (*name).to_owned()).collect();
    names.sort();
    names
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FixedConnector {
    gateways: BTreeMap<GatewayName, Arc<dyn Gateway>>,
}

impl FixedConnector {
    pub(crate) fn empty() -> FixedConnector {
        FixedConnector::default()
    }

    pub(crate) fn with(mut self, gateway: Arc<dyn Gateway>) -> FixedConnector {
        self.gateways.insert(gateway.name().clone(), gateway);
        self
    }
}

#[async_trait]
impl GatewayConnector for FixedConnector {
    async fn connect(
        &self,
        name: &GatewayName,
        _entry: &GatewayEntry,
        _cancel: &CancellationToken,
    ) -> Result<Arc<dyn Gateway>, ConnectError> {
        self.gateways.get(name).cloned().ok_or_else(|| {
            ConnectError::Gateway(GatewayError::Unreachable {
                endpoint: name.to_string(),
                message: "the test connector knows no such gateway".to_owned(),
            })
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Step {
    Line(String),
    Bytes(Vec<u8>),
    Resize(TermSize),
    Close,
    Gate(Arc<Notify>),
}

#[derive(Clone, Debug)]
pub(crate) struct RawProbe {
    raw: Arc<AtomicBool>,
    reading: Arc<watch::Sender<bool>>,
}

impl RawProbe {
    pub(crate) fn raw(&self) -> bool {
        self.raw.load(Ordering::SeqCst)
    }

    pub(crate) async fn pending_read(&self) {
        drop(self.reading.subscribe().wait_for(|reading| *reading).await);
    }
}

struct Leave(Arc<AtomicBool>);

impl Drop for Leave {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug)]
pub(crate) struct ScriptedTerminal {
    script: VecDeque<Step>,
    size: TermSize,
    raw: Arc<AtomicBool>,
    reading: Arc<watch::Sender<bool>>,
}

pub(crate) fn term_size(cols: u16, rows: u16) -> Result<TermSize, Failure> {
    Ok(TermSize {
        cols: NonZeroU16::new(cols).ok_or("zero columns")?,
        rows: NonZeroU16::new(rows).ok_or("zero rows")?,
    })
}

impl ScriptedTerminal {
    pub(crate) fn new(steps: Vec<Step>) -> Result<(ScriptedTerminal, RawProbe), Failure> {
        let raw = Arc::new(AtomicBool::new(false));
        let reading = Arc::new(watch::channel(false).0);
        Ok((
            ScriptedTerminal {
                script: steps.into(),
                size: term_size(100, 30)?,
                raw: Arc::clone(&raw),
                reading: Arc::clone(&reading),
            },
            RawProbe { raw, reading },
        ))
    }
}

#[async_trait]
impl Terminal for ScriptedTerminal {
    fn size(&self) -> TermSize {
        self.size
    }

    fn enter_raw(&mut self) -> io::Result<RawMode> {
        self.raw.store(true, Ordering::SeqCst);
        Ok(RawMode::new(Leave(Arc::clone(&self.raw))))
    }

    async fn read_line(&mut self) -> io::Result<Option<String>> {
        match self.script.front() {
            Some(Step::Line(_)) => match self.script.pop_front() {
                Some(Step::Line(line)) => Ok(Some(line)),
                _ => std::future::pending().await,
            },
            Some(Step::Close) => {
                self.script.pop_front();
                Ok(None)
            }
            _ => {
                self.reading.send_replace(true);
                std::future::pending().await
            }
        }
    }

    async fn next_input(&mut self) -> TerminalInput {
        loop {
            match self.script.front() {
                Some(Step::Gate(gate)) => {
                    Arc::clone(gate).notified().await;
                    self.script.pop_front();
                }
                Some(Step::Bytes(_) | Step::Resize(_) | Step::Close) => {
                    return match self.script.pop_front() {
                        Some(Step::Bytes(bytes)) => TerminalInput::Bytes(bytes),
                        Some(Step::Resize(size)) => {
                            self.size = size;
                            TerminalInput::Resized(size)
                        }
                        _ => TerminalInput::Closed,
                    };
                }
                _ => std::future::pending().await,
            }
        }
    }
}

pub(crate) const CLOSED_ORIGIN: &str = "http://127.0.0.1:1";

pub(crate) fn closed_endpoints() -> ProviderEndpoints {
    Provider::ALL
        .into_iter()
        .fold(ProviderEndpoints::real(), |endpoints, provider| {
            endpoints.with(provider, CLOSED_ORIGIN.to_owned())
        })
}

pub(crate) async fn until(mut done: impl FnMut() -> bool) -> Result<(), Failure> {
    for _ in 0..400 {
        if done() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("the condition never held".into())
}

pub(crate) struct Harness {
    pub(crate) connector: FixedConnector,
    pub(crate) custom: Option<Arc<dyn GatewayConnector>>,
    pub(crate) cwd: HostPath,
    pub(crate) config_path: HostPath,
    pub(crate) cache_dir: HostPath,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) usage_endpoints: ProviderEndpoints,
    pub(crate) console: Arc<ScriptedConsole>,
    host: HostPorts,
    engine: Arc<dyn ImageEngine>,
}

pub(crate) struct Invocation<'a> {
    harness: &'a Harness,
    args: Vec<String>,
    terminal: Option<ScriptedTerminal>,
    cancel: CancellationToken,
}

impl Harness {
    pub(crate) fn new(world: &World) -> Harness {
        let clock: Arc<dyn Clock> = Arc::<jackin_testkit::FakeClock>::clone(&world.clock);
        let runner: Arc<dyn ProcessRunner> =
            Arc::<jackin_testkit::TestRunner>::clone(&world.runner);
        let engine: Arc<dyn ImageEngine> = Arc::<jackin_testkit::FakeEngine>::clone(&world.engine);
        let gateway: Arc<dyn Gateway> = Arc::<jackin_testkit::FakeGateway>::clone(&world.gateway);
        Harness {
            connector: FixedConnector::empty().with(gateway),
            custom: None,
            cwd: world.home().clone(),
            config_path: ConfigStore::default_path(world.home()),
            cache_dir: world.cache_dir().clone(),
            env: vec![(
                "HOME".to_owned(),
                world.home().as_path().display().to_string(),
            )],
            usage_endpoints: closed_endpoints(),
            console: Arc::new(ScriptedConsole::default()),
            host: HostPorts {
                clock,
                fs: Arc::clone(&world.fs),
                runner,
                home: world.home().clone(),
            },
            engine,
        }
    }

    pub(crate) fn fs(&self, fs: Arc<dyn FileSystem>) -> Harness {
        Harness {
            connector: self.connector.clone(),
            custom: self.custom.clone(),
            cwd: self.cwd.clone(),
            config_path: self.config_path.clone(),
            cache_dir: self.cache_dir.clone(),
            env: self.env.clone(),
            usage_endpoints: self.usage_endpoints.clone(),
            console: Arc::clone(&self.console),
            host: HostPorts {
                clock: Arc::clone(&self.host.clock),
                fs,
                runner: Arc::clone(&self.host.runner),
                home: self.host.home.clone(),
            },
            engine: Arc::clone(&self.engine),
        }
    }

    pub(crate) fn runner(&self, runner: Arc<dyn ProcessRunner>) -> Harness {
        Harness {
            connector: self.connector.clone(),
            custom: self.custom.clone(),
            cwd: self.cwd.clone(),
            config_path: self.config_path.clone(),
            cache_dir: self.cache_dir.clone(),
            env: self.env.clone(),
            usage_endpoints: self.usage_endpoints.clone(),
            console: Arc::clone(&self.console),
            host: HostPorts {
                clock: Arc::clone(&self.host.clock),
                fs: Arc::clone(&self.host.fs),
                runner,
                home: self.host.home.clone(),
            },
            engine: Arc::clone(&self.engine),
        }
    }

    pub(crate) fn write_config(&self, text: &str) -> Result<(), Failure> {
        let path = self.config_path.as_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, text)?;
        Ok(())
    }

    pub(crate) fn store(&self) -> ConfigStore {
        ConfigStore::new(
            Arc::clone(&self.host.fs),
            Arc::clone(&self.host.clock),
            self.config_path.clone(),
        )
    }

    pub(crate) async fn stored(&self) -> Result<Config, Failure> {
        Ok(self.store().load().await?)
    }

    pub(crate) fn config_bytes(&self) -> Result<Option<String>, Failure> {
        match std::fs::read(self.config_path.as_path()) {
            Ok(bytes) => Ok(Some(String::from_utf8(bytes)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn command(&self, args: &[&str]) -> Invocation<'_> {
        let mut owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        if args.contains(&"load") {
            let explicit_provider = args.iter().any(|arg| {
                *arg == "--provider"
                    || arg.starts_with("--provider=")
                    || arg.starts_with("--provider-for=codex=")
            }) || args
                .windows(2)
                .any(|pair| pair[0] == "--provider-for" && pair[1].starts_with("codex="));
            if !explicit_provider {
                owned.extend([
                    "--provider".to_owned(),
                    "zai".to_owned(),
                    "--provider-for".to_owned(),
                    "codex=zai".to_owned(),
                    "--model".to_owned(),
                    "glm-5.3".to_owned(),
                    "--model-for".to_owned(),
                    "codex=glm-5.3".to_owned(),
                ]);
            }
        }
        Invocation {
            harness: self,
            args: owned,
            terminal: None,
            cancel: CancellationToken::new(),
        }
    }

    pub(crate) async fn run(&self, args: &[&str]) -> Result<Ran, Failure> {
        self.command(args).run().await
    }
}

impl Invocation<'_> {
    pub(crate) fn terminal(mut self, terminal: ScriptedTerminal) -> Self {
        self.terminal = Some(terminal);
        self
    }

    pub(crate) fn cancel(mut self, cancel: CancellationToken) -> Self {
        self.cancel = cancel;
        self
    }

    pub(crate) async fn run(self) -> Result<Ran, Failure> {
        let harness = self.harness;
        let stdout = Shared::default();
        let stderr = Shared::default();
        let mut args: Vec<OsString> = vec![OsString::from("jackin")];
        args.extend(self.args.iter().map(OsString::from));
        let terminal: Option<Box<dyn Terminal>> = match self.terminal {
            Some(terminal) => Some(Box::new(terminal)),
            None => None,
        };
        let env = AppEnv {
            args,
            stdout: Box::new(stdout.clone()),
            stderr: Box::new(stderr.clone()),
            terminal,
            cwd: harness.cwd.clone(),
            config_path: harness.config_path.clone(),
            cache_dir: harness.cache_dir.clone(),
            env: EnvSnapshot::from_pairs(harness.env.iter().cloned()),
            cancel: self.cancel,
        };
        let connect: Arc<dyn GatewayConnector> = match &harness.custom {
            Some(custom) => Arc::clone(custom),
            None => Arc::new(harness.connector.clone()),
        };
        let deps = Deps {
            host: harness.host.clone(),
            engine: Arc::clone(&harness.engine),
            connect,
            limits: default_limits(),
            usage_endpoints: harness.usage_endpoints.clone(),
            console: Arc::<ScriptedConsole>::clone(&harness.console),
        };
        let exit = jackin::run(env, deps).await;
        Ok(Ran {
            exit,
            stdout: stdout.text(),
            stderr: stderr.text(),
        })
    }
}

pub(crate) const GATEWAY_CONFIG: &str = "default_gateway = \"fake-gateway\"

[gateways.fake-gateway]
openshell_workspace = \"default\"
builder = \"b\"
platform = \"linux/arm64\"
";

pub(crate) fn connecting(
    world: &World,
    reply: ProcessReply,
) -> Result<(Harness, GrpcConnector), Failure> {
    let mut harness = Harness::new(world);
    harness.write_config(GATEWAY_CONFIG)?;
    world.runner.scripted().expect(
        ScriptedProgram::Openshell,
        &["gateway", "list", "-o", "json"],
        reply,
    );
    let runner: Arc<dyn ProcessRunner> = Arc::<jackin_testkit::TestRunner>::clone(&world.runner);
    let fs: Arc<dyn FileSystem> = Arc::clone(&world.fs);
    let connector = GrpcConnector::new(runner, fs, world.home().clone());
    harness.custom = Some(Arc::new(connector.clone()));
    Ok((harness, connector))
}

pub(crate) fn host_path(path: &Path) -> Result<HostPath, Failure> {
    Ok(HostPath::parse(path)?)
}
