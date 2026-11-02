// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::Debug;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use jackin::{AppEnv, Deps, ExitClass, GatewayConnector};
use jackin_core::{
    Clock, Config, ConfigStore, EnvSnapshot, FileSystem, GatewayEntry, GatewayName, HostPath,
    ProcessRunner,
};
use jackin_image::ImageEngine;
use jackin_openshell::{Gateway, GatewayError};
use jackin_session::HostPorts;
use jackin_testkit::{World, default_limits};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

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
    ) -> Result<Arc<dyn Gateway>, GatewayError> {
        self.gateways
            .get(name)
            .cloned()
            .ok_or_else(|| GatewayError::Unreachable {
                endpoint: name.to_string(),
                message: "the test connector knows no such gateway".to_owned(),
            })
    }
}

pub(crate) struct Harness {
    pub(crate) connector: FixedConnector,
    pub(crate) cwd: HostPath,
    pub(crate) config_path: HostPath,
    pub(crate) cache_dir: HostPath,
    pub(crate) env: Vec<(String, String)>,
    host: HostPorts,
    engine: Arc<dyn ImageEngine>,
}

pub(crate) struct Invocation<'a> {
    harness: &'a Harness,
    args: Vec<String>,
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
            cwd: world.home().clone(),
            config_path: ConfigStore::default_path(world.home()),
            cache_dir: world.cache_dir().clone(),
            env: vec![(
                "HOME".to_owned(),
                world.home().as_path().display().to_string(),
            )],
            host: HostPorts {
                clock,
                fs: Arc::clone(&world.fs),
                runner,
            },
            engine,
        }
    }

    pub(crate) fn fs(&self, fs: Arc<dyn FileSystem>) -> Harness {
        Harness {
            connector: self.connector.clone(),
            cwd: self.cwd.clone(),
            config_path: self.config_path.clone(),
            cache_dir: self.cache_dir.clone(),
            env: self.env.clone(),
            host: HostPorts {
                clock: Arc::clone(&self.host.clock),
                fs,
                runner: Arc::clone(&self.host.runner),
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

    pub(crate) async fn stored(&self) -> Result<Config, Failure> {
        let store = ConfigStore::new(
            Arc::clone(&self.host.fs),
            Arc::clone(&self.host.clock),
            self.config_path.clone(),
        );
        Ok(store.load().await?)
    }

    pub(crate) fn config_bytes(&self) -> Result<Option<String>, Failure> {
        match std::fs::read(self.config_path.as_path()) {
            Ok(bytes) => Ok(Some(String::from_utf8(bytes)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn command(&self, args: &[&str]) -> Invocation<'_> {
        Invocation {
            harness: self,
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            cancel: CancellationToken::new(),
        }
    }

    pub(crate) async fn run(&self, args: &[&str]) -> Result<Ran, Failure> {
        self.command(args).run().await
    }
}

impl Invocation<'_> {
    pub(crate) async fn run(self) -> Result<Ran, Failure> {
        let harness = self.harness;
        let stdout = Shared::default();
        let stderr = Shared::default();
        let mut args: Vec<OsString> = vec![OsString::from("jackin")];
        args.extend(self.args.iter().map(OsString::from));
        let env = AppEnv {
            args,
            stdout: Box::new(stdout.clone()),
            stderr: Box::new(stderr.clone()),
            terminal: None,
            cwd: harness.cwd.clone(),
            config_path: harness.config_path.clone(),
            cache_dir: harness.cache_dir.clone(),
            env: EnvSnapshot::from_pairs(harness.env.iter().cloned()),
            cancel: self.cancel,
        };
        let deps = Deps {
            host: harness.host.clone(),
            engine: Arc::clone(&harness.engine),
            connect: Arc::new(harness.connector.clone()),
            limits: default_limits(),
        };
        let exit = jackin::run(env, deps).await;
        Ok(Ran {
            exit,
            stdout: stdout.text(),
            stderr: stderr.text(),
        })
    }
}

pub(crate) fn host_path(path: &Path) -> Result<HostPath, Failure> {
    Ok(HostPath::parse(path)?)
}
