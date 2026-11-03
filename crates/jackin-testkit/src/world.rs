// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU8;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Clock, ConfigStore, FileSystem, GatewayName, HostPath, OpenShellWorkspace, ProcessRunner,
    StdFileSystem,
};
use jackin_image::ImageEngine;
use jackin_openshell::Gateway;
use jackin_session::{
    DEFAULT_BUNDLE_BYTES, DEFAULT_EXPORT_ENTRY_BYTES, DEFAULT_EXPORT_OBJECTS,
    DEFAULT_EXPORT_TOTAL_BYTES, HostPorts, LaunchLimits, RoleCatalog, Session, SessionDeps,
};
use tempfile::TempDir;

use crate::engine::FakeEngine;
use crate::fixtures::FixtureError;
use crate::gateway::FakeGateway;
use crate::host::{FakeClock, TestRunner};
use crate::usage::LazyUsageServer;

#[derive(Debug)]
pub struct World {
    pub gateway: Arc<FakeGateway>,
    pub engine: Arc<FakeEngine>,
    pub clock: Arc<FakeClock>,
    pub fs: Arc<dyn FileSystem>,
    pub runner: Arc<TestRunner>,
    pub session: Session,
    pub roles: Arc<RoleCatalog>,
    pub config: ConfigStore,
    pub usage: LazyUsageServer,
    cache_dir: HostPath,
    home: HostPath,
    _dir: TempDir,
}

#[derive(Debug)]
pub struct WorldBuilder {
    limits: LaunchLimits,
}

pub fn default_limits() -> LaunchLimits {
    LaunchLimits {
        ready: Duration::from_secs(30),
        supervisor: Duration::from_secs(20),
        attempt: Duration::from_secs(20),
        hook: Duration::from_secs(600),
        upload: Duration::from_secs(600),
        create_attempts: NonZeroU8::MIN.saturating_add(2),
        bundle_bytes: DEFAULT_BUNDLE_BYTES,
        export_entry_bytes: DEFAULT_EXPORT_ENTRY_BYTES,
        export_objects: DEFAULT_EXPORT_OBJECTS,
        export_total_bytes: DEFAULT_EXPORT_TOTAL_BYTES,
    }
}

impl World {
    pub fn builder() -> WorldBuilder {
        WorldBuilder {
            limits: default_limits(),
        }
    }

    pub fn cache_dir(&self) -> &HostPath {
        &self.cache_dir
    }

    pub fn home(&self) -> &HostPath {
        &self.home
    }
}

impl WorldBuilder {
    #[must_use]
    pub fn limits(mut self, limits: LaunchLimits) -> WorldBuilder {
        self.limits = limits;
        self
    }

    pub fn build(self) -> Result<World, FixtureError> {
        let dir = tempfile::Builder::new().prefix("jackin-world-").tempdir()?;
        let canonical = std::fs::canonicalize(dir.path())?;
        let cache_dir = HostPath::parse(&canonical.join("cache"))?;
        let config_path = HostPath::parse(&canonical.join("config.toml"))?;
        let home_path = canonical.join("home");
        std::fs::create_dir(&home_path)?;
        let home = HostPath::parse(&home_path)?;
        let gateway = Arc::new(FakeGateway::new(
            GatewayName::parse("fake-gateway")?,
            OpenShellWorkspace::parse("default")?,
        ));
        let engine = Arc::new(FakeEngine::new());
        gateway.link_engine(Arc::clone(&engine));
        gateway.use_host_home(&home);
        let clock = Arc::new(FakeClock::new());
        gateway.link_clock(Arc::clone(&clock));
        let runner = Arc::new(TestRunner::with_home(&home));
        let fs: Arc<dyn FileSystem> = Arc::new(StdFileSystem::default());
        let gateway_port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&gateway);
        let engine_port: Arc<dyn ImageEngine> = Arc::<FakeEngine>::clone(&engine);
        let clock_port: Arc<dyn Clock> = Arc::<FakeClock>::clone(&clock);
        let runner_port: Arc<dyn ProcessRunner> = Arc::<TestRunner>::clone(&runner);
        let session = Session::new(SessionDeps {
            gateway: gateway_port,
            engine: engine_port,
            host: HostPorts {
                clock: Arc::clone(&clock_port),
                fs: Arc::clone(&fs),
                runner: Arc::clone(&runner_port),
            },
            cache_dir: cache_dir.clone(),
            limits: self.limits,
        });
        Ok(World {
            roles: Arc::new(RoleCatalog::new(
                Arc::clone(&fs),
                runner_port,
                cache_dir.clone(),
            )),
            config: ConfigStore::new(Arc::clone(&fs), clock_port, config_path),
            usage: LazyUsageServer::default(),
            gateway,
            engine,
            clock,
            fs,
            runner,
            session,
            cache_dir,
            home,
            _dir: dir,
        })
    }
}
