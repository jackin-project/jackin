// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Config, EnvName, FileSystem, GatewayName, HostPath, ImageId, LaunchId, PlainEnvValue,
    ProjectRoot, RoleName, Sha256, StdFileSystem,
};
use jackin_image::{BuiltImage, ImageSource};
use jackin_session::{LaunchPlan, LaunchRequest, LoadedRole, PlanError, RoleCatalog, plan_launch};
use jackin_testkit::{FakeClock, RepoFixture, RoleFixture, TestRunner};

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const ACCOUNT_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f";
const GUARD: Duration = Duration::from_secs(170);

pub(crate) async fn guard(work: impl Future<Output = TestResult>) -> TestResult {
    tokio::time::timeout(GUARD, work)
        .await
        .map_err(|_| format!("the test hung for {} s", GUARD.as_secs()))?
}

pub(crate) fn launch_id(byte: u8) -> LaunchId {
    LaunchId::from_entropy(&[byte; 26])
}

pub(crate) fn catalog(cache: &HostPath) -> (RoleCatalog, Arc<TestRunner>) {
    let runner = Arc::new(TestRunner::new());
    let fs: Arc<dyn FileSystem> = Arc::new(StdFileSystem::default());
    let catalog = RoleCatalog::new(
        fs,
        Arc::<TestRunner>::clone(&runner),
        Arc::new(FakeClock::new()),
        cache.clone(),
    );
    (catalog, runner)
}

pub(crate) fn login_account(name: &str) -> String {
    format!(
        "[accounts.{name}]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"/tmp/codex-home\", chatgpt_account_id = \"{ACCOUNT_ID}\" }}\n"
    )
}

pub(crate) struct Spec {
    pub(crate) role: RoleFixture,
    pub(crate) engine: Option<u16>,
    pub(crate) workspace_docker: bool,
    pub(crate) accounts: String,
    pub(crate) defaults: String,
    pub(crate) workspace_bindings: String,
    pub(crate) role_bindings: String,
    pub(crate) gateways: Option<String>,
    pub(crate) workspace_keys: String,
    pub(crate) tables: String,
}

pub(crate) struct Setup {
    pub(crate) config: Config,
    pub(crate) role: LoadedRole,
    pub(crate) role_fixture: RoleFixture,
    _repo: RepoFixture,
    pub(crate) project: ProjectRoot,
    pub(crate) gateway: GatewayName,
    pub(crate) cache: HostPath,
    _cache_dir: tempfile::TempDir,
}

impl Spec {
    pub(crate) fn new(role: RoleFixture) -> Spec {
        Spec {
            role,
            engine: None,
            workspace_docker: false,
            accounts: login_account("work"),
            defaults: String::new(),
            workspace_bindings: String::new(),
            role_bindings: String::new(),
            gateways: None,
            workspace_keys: String::new(),
            tables: String::new(),
        }
    }

    pub(crate) fn text(&self, repo: &HostPath, role: &HostPath) -> String {
        let gateways = self.gateways.clone().unwrap_or_else(|| {
            let mut text = "default_gateway = \"jackin-local\"\n\n[gateways.jackin-local]\nopenshell_workspace = \"default\"\nbuilder = \"jackin-builder\"\nplatform = \"linux/arm64\"\n".to_owned();
            if let Some(port) = self.engine {
                text.push_str(&format!(
                    "\n[gateways.jackin-local.docker_engine]\nport = {port}\nclient_certs = \"/tmp/engine-certs\"\n"
                ));
            }
            text
        });
        let defaults = if self.defaults.is_empty() {
            String::new()
        } else {
            format!("\n[defaults.accounts]\n{}", self.defaults)
        };
        format!(
            "{gateways}{defaults}\n{}\n[roles.sentinel]\nsource = {{ path = \"{}\" }}\nbindings = {{ {} }}\n\n[workspaces.platform-backend]\npath = \"{}\"\ndocker = {}\n{}bindings = {{ {} }}\n{}",
            self.accounts,
            role.as_path().display(),
            self.role_bindings,
            repo.as_path().display(),
            self.workspace_docker,
            self.workspace_keys,
            self.workspace_bindings,
            self.tables,
        )
    }

    pub(crate) async fn build(self) -> Result<Setup, Box<dyn Error>> {
        let repo = RepoFixture::new().await?;
        repo.file("src/main.rs", b"fn main() {}\n").await?;
        repo.file("README.md", b"readme\n").await?;
        repo.ignore("target/\n").await?;
        repo.commit().await?;
        let role_fixture = self.role.clone();
        role_fixture.commit().await?;
        let cache_dir = tempfile::tempdir()?;
        let cache = HostPath::parse(&std::fs::canonicalize(cache_dir.path())?)?;
        let (catalog, _runner) = catalog(&cache);
        let role = catalog
            .load_path(
                role_fixture.root(),
                &tokio_util::sync::CancellationToken::new(),
            )
            .await?;
        let config = Config::parse(&self.text(repo.root(), role_fixture.root()))?;
        let project = repo.project().await?;
        Ok(Setup {
            config,
            role,
            role_fixture,
            _repo: repo,
            project,
            gateway: GatewayName::parse("jackin-local")?,
            cache,
            _cache_dir: cache_dir,
        })
    }
}

impl Setup {
    pub(crate) fn request(&self, id: u8) -> Result<LaunchRequest, Box<dyn Error>> {
        Ok(LaunchRequest::new(
            self.gateway.clone(),
            self.project.clone(),
            RoleName::parse("sentinel")?,
            launch_id(id),
        ))
    }

    pub(crate) fn plan(&self, request: LaunchRequest) -> Result<LaunchPlan, PlanError> {
        plan_launch(&self.config, &self.role, request)
    }
}

pub(crate) fn built_image(role: &LoadedRole, fill: char) -> Result<BuiltImage, Box<dyn Error>> {
    let key = Sha256::of(b"image key");
    Ok(BuiltImage {
        tag: jackin_core::ImageTag::for_role(role.name(), &key),
        id: ImageId::parse(&format!("sha256:{}", fill.to_string().repeat(64)))?,
        source: ImageSource::Built,
    })
}

pub(crate) fn plain_values(
    pairs: &[(&str, &str)],
) -> Result<BTreeMap<EnvName, PlainEnvValue>, Box<dyn Error>> {
    pairs
        .iter()
        .map(|(name, value)| Ok((EnvName::parse(name)?, PlainEnvValue::parse(value)?)))
        .collect()
}

pub(crate) fn env_values(
    pairs: &[(&str, &str)],
) -> Result<BTreeMap<EnvName, String>, Box<dyn Error>> {
    pairs
        .iter()
        .map(|(name, value)| Ok((EnvName::parse(name)?, (*value).to_owned())))
        .collect()
}
