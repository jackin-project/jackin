// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU8;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Clock, Config, ConfigEdit, ConfigError, ConfigStore, EnvScope, FileSystem, GatewayName,
    HostPath, OpenShellWorkspace, ProcessRunner, Provider, StdFileSystem,
};
use jackin_image::ImageEngine;
use jackin_openshell::Gateway;
use jackin_session::{
    AccountDesk, DEFAULT_BUNDLE_BYTES, DEFAULT_EXPORT_ENTRY_BYTES, DEFAULT_EXPORT_OBJECTS,
    DEFAULT_EXPORT_TOTAL_BYTES, HostPorts, LaunchLimits, RoleCatalog, Session, SessionDeps,
};
use tempfile::TempDir;

use crate::engine::FakeEngine;
use crate::fixtures::FixtureError;
use crate::gateway::FakeGateway;
use crate::host::{FakeClock, FsFault, TestRunner, WorkingFs};
use crate::usage::LazyUsageServer;

#[derive(Debug)]
pub struct World {
    pub gateway: Arc<FakeGateway>,
    pub engine: Arc<FakeEngine>,
    pub clock: Arc<FakeClock>,
    pub fs: Arc<dyn FileSystem>,
    working_fs: Arc<WorkingFs>,
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

    pub fn fail_fs(&self, fault: FsFault) {
        self.working_fs.fail_next(fault);
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
                home: self.home.clone(),
            },
            self.cache_dir.clone(),
            store,
        )
    }

    pub fn seed_omp_agent_db(&self, provider: &str, key: Option<&str>) -> Result<(), FixtureError> {
        let directory = self.home.as_path().join(".omp/agent");
        std::fs::create_dir_all(&directory)?;
        let connection = rusqlite::Connection::open(directory.join("agent.db"))?;
        let data = key.map_or_else(
            || serde_json::json!({}),
            |key| serde_json::json!({ "key": key }),
        );
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS auth_credentials (id INTEGER PRIMARY KEY AUTOINCREMENT, provider TEXT NOT NULL, credential_type TEXT NOT NULL, data TEXT NOT NULL, disabled_cause TEXT DEFAULT NULL, identity_key TEXT DEFAULT NULL, created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0);",
        )?;
        connection.execute(
            "INSERT INTO auth_credentials (provider, credential_type, data) VALUES (?1, 'api_key', ?2)",
            rusqlite::params![provider, data.to_string()],
        )?;
        Ok(())
    }

    pub fn isolated_omp_ambient(
        test_name: &str,
        provider: &str,
        key: &str,
    ) -> Result<bool, FixtureError> {
        const MARKER: &str = "JACKIN_TEST_OMP_AMBIENT_CHILD";
        if std::env::var_os(MARKER).is_some() {
            return Ok(false);
        }
        let ambient = World::builder().build()?;
        ambient.seed_omp_agent_db(provider, Some(key))?;
        let status = std::process::Command::new("/usr/bin/env")
            .arg(std::env::current_exe()?)
            .arg("--exact")
            .arg(test_name)
            .arg("--nocapture")
            .env("HOME", ambient.home().as_path())
            .env(MARKER, "1")
            .status()?;
        if !status.success() {
            return Err(format!("the isolated OmpHost test failed with {status}").into());
        }
        Ok(true)
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
        self.config.edit(edits).await?;
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
        self.config.edit(edits).await?;
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
            for (agent, account) in &role.bindings {
                edits.push(ConfigEdit::BindRoleAccount {
                    role: name.clone(),
                    agent: *agent,
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
        self.config.edit(edits).await?;
        Ok(())
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
        let working_fs = Arc::new(WorkingFs::new(Arc::new(StdFileSystem::default()), &clock));
        let fs: Arc<dyn FileSystem> = Arc::<WorkingFs>::clone(&working_fs);
        let gateway_port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&gateway);
        let engine_port: Arc<dyn ImageEngine> = Arc::<FakeEngine>::clone(&engine);
        let clock_port: Arc<dyn Clock> = Arc::<FakeClock>::clone(&clock);
        let runner_port: Arc<dyn ProcessRunner> = Arc::<TestRunner>::clone(&runner);
        let config = ConfigStore::new(Arc::clone(&fs), Arc::clone(&clock_port), config_path);
        let session = Session::new(SessionDeps {
            gateway: gateway_port,
            engine: engine_port,
            host: HostPorts {
                clock: Arc::clone(&clock_port),
                fs: Arc::clone(&fs),
                runner: Arc::clone(&runner_port),
                home: home.clone(),
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
            clock,
            fs,
            working_fs,
            runner,
            session,
            cache_dir,
            home,
            _dir: dir,
        })
    }
}
