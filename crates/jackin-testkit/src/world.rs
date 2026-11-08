// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU8;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use jackin_core::{
    Clock, Config, ConfigEdit, ConfigError, ConfigStore, EnvScope, FileSystem, GatewayName,
    HostPath, OpenShellWorkspace, ProcessRunner, Provider, StdFileSystem,
};
use jackin_image::BuildxEngine;
use jackin_openshell::Gateway;
use jackin_session::{
    AccountDesk, DEFAULT_BUNDLE_BYTES, DEFAULT_EXPORT_ENTRY_BYTES, DEFAULT_EXPORT_OBJECTS,
    DEFAULT_EXPORT_TOTAL_BYTES, HostPorts, LaunchLimits, RoleCatalog, Session, SessionDeps,
};
use tempfile::TempDir;

use crate::docker::DockerBoundary;
use crate::fixtures::FixtureError;
use crate::gateway::FakeGateway;
use crate::host::{FakeClock, TestRunner, WorkingFs};
use crate::usage::LazyUsageServer;

#[derive(Debug)]
pub struct World {
    pub gateway: Arc<FakeGateway>,
    pub engine: Arc<BuildxEngine>,
    pub docker: Arc<DockerBoundary>,
    pub clock: Arc<FakeClock>,
    pub fs: Arc<dyn FileSystem>,
    pub runner: Arc<TestRunner>,
    pub cancel: CancellationToken,
    pub session: Session,
    pub roles: Arc<RoleCatalog>,
    pub config: ConfigStore,
    pub usage: LazyUsageServer,
    cache_dir: HostPath,
    image_work_dir: HostPath,
    home: HostPath,
    _dir: TempDir,
}

#[derive(Debug)]
pub struct WorldBuilder {
    limits: LaunchLimits,
    fs: Option<Arc<dyn FileSystem>>,
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
            fs: None,
        }
    }

    pub fn cache_dir(&self) -> &HostPath {
        &self.cache_dir
    }

    pub fn home(&self) -> &HostPath {
        &self.home
    }

    pub fn image_work_dir(&self) -> &HostPath {
        &self.image_work_dir
    }

    pub fn desk(&self) -> AccountDesk {
        self.desk_over(self.config.clone())
    }

    pub fn desk_over(&self, store: ConfigStore) -> AccountDesk {
        AccountDesk::new(
            HostPorts {
                clock: Arc::<FakeClock>::clone(&self.clock),
                fs: Arc::clone(&self.fs),
                runner: Arc::<TestRunner>::clone(&self.runner),
            },
            self.cache_dir.clone(),
            store,
        )
    }

    pub async fn seed_config(&self, config: &Config) -> Result<(), ConfigError> {
        let mut edits: Vec<ConfigEdit> = config
            .gateways()
            .map(|(name, entry)| ConfigEdit::AddGateway {
                name: name.clone(),
                entry: entry.clone(),
            })
            .collect();
        edits.extend(config.accounts().cloned().map(ConfigEdit::AddAccount));
        self.config.edit(edits, &self.cancel).await?;
        Ok(())
    }

    pub async fn seed_workspaces(&self, config: &Config) -> Result<(), ConfigError> {
        let edits: Vec<ConfigEdit> = config
            .workspaces()
            .map(|(name, workspace)| ConfigEdit::AddWorkspace {
                name: name.clone(),
                workspace: workspace.clone(),
            })
            .collect();
        self.config.edit(edits, &self.cancel).await?;
        Ok(())
    }

    pub async fn seed_all(&self, config: &Config) -> Result<(), ConfigError> {
        let stored = self.config.load().await?;
        let mut edits = Vec::new();
        for (name, entry) in config.gateways() {
            if stored.gateway(name).is_none() {
                edits.push(ConfigEdit::AddGateway {
                    name: name.clone(),
                    entry: entry.clone(),
                });
            }
        }
        if let Some(name) = config
            .default_gateway()
            .filter(|_| stored.default_gateway().is_none())
        {
            edits.push(ConfigEdit::SetDefaultGateway(Some(name.clone())));
        }
        for account in config.accounts() {
            if stored.account(account.name()).is_none() {
                edits.push(ConfigEdit::AddAccount(account.clone()));
            }
        }
        for provider in Provider::ALL {
            if let Some(account) = config
                .default_account(provider)
                .filter(|_| stored.default_account(provider).is_none())
            {
                edits.push(ConfigEdit::SetDefaultAccount {
                    account: account.clone(),
                });
            }
        }
        for (name, role) in config
            .roles()
            .filter(|(name, _)| stored.role(name).is_none())
        {
            edits.push(ConfigEdit::AddRole {
                name: name.clone(),
                source: role.source.clone(),
            });
            for (provider, account) in &role.bindings {
                let agent = jackin_core::TUPLES
                    .iter()
                    .find(|tuple| tuple.provider == *provider)
                    .map_or(jackin_core::Agent::Codex, |tuple| tuple.agent);
                edits.push(ConfigEdit::BindRoleAccount {
                    role: name.clone(),
                    agent,
                    account: account.clone(),
                });
            }
        }
        for (name, workspace) in config
            .workspaces()
            .filter(|(name, _)| stored.workspace(name).is_none())
        {
            edits.push(ConfigEdit::AddWorkspace {
                name: name.clone(),
                workspace: workspace.clone(),
            });
        }
        for (name, value) in config
            .env()
            .iter()
            .filter(|(name, _)| !stored.env().contains_key(*name))
        {
            edits.push(ConfigEdit::SetEnv {
                scope: EnvScope::Global,
                name: name.name().clone(),
                value: Some(value.clone()),
            });
        }
        self.config.edit(edits, &self.cancel).await?;
        Ok(())
    }
}

impl WorldBuilder {
    #[must_use]
    pub fn filesystem(mut self, fs: Arc<dyn FileSystem>) -> WorldBuilder {
        self.fs = Some(fs);
        self
    }

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
        let clock = Arc::new(FakeClock::new());
        let docker = Arc::new(DockerBoundary::with_fake_clock(Arc::clone(&clock)));
        gateway.link_image_registry(docker.registry());
        gateway.use_host_home(&home);
        gateway.link_clock(Arc::clone(&clock));
        let runner = Arc::new(TestRunner::with_home(&home));
        let cancel = CancellationToken::new();
        let base_fs: Arc<dyn FileSystem> = self
            .fs
            .unwrap_or_else(|| Arc::new(StdFileSystem::default()));
        let fs: Arc<dyn FileSystem> = Arc::new(WorkingFs::new(base_fs, &clock));
        let gateway_port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&gateway);
        let docker_runner: Arc<dyn ProcessRunner> = Arc::<DockerBoundary>::clone(&docker);
        let work_dir = HostPath::parse(&canonical.join("image-work"))?;
        let engine = Arc::new(BuildxEngine::new(
            docker_runner,
            Arc::clone(&fs),
            work_dir.clone(),
        ));
        let clock_port: Arc<dyn Clock> = Arc::<FakeClock>::clone(&clock);
        let runner_port: Arc<dyn ProcessRunner> = Arc::<TestRunner>::clone(&runner);
        let engine_port: Arc<dyn jackin_image::ImageEngine> = Arc::<BuildxEngine>::clone(&engine);
        let config = ConfigStore::new(Arc::clone(&fs), Arc::clone(&clock_port), config_path);
        let session = Session::new(SessionDeps {
            gateway: gateway_port,
            engine: engine_port,
            host: HostPorts {
                clock: Arc::clone(&clock_port),
                fs: Arc::clone(&fs),
                runner: Arc::clone(&runner_port),
            },
            cache_dir: cache_dir.clone(),
            config: config.clone(),
            limits: self.limits,
        });
        Ok(World {
            roles: Arc::new(RoleCatalog::new(
                Arc::clone(&fs),
                runner_port,
                Arc::clone(&clock_port),
                cache_dir.clone(),
            )),
            config,
            usage: LazyUsageServer::default(),
            gateway,
            engine,
            docker,
            clock,
            fs,
            runner,
            cancel,
            session,
            cache_dir,
            image_work_dir: work_dir,
            home,
            _dir: dir,
        })
    }
}
