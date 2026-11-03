// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU8;
use std::time::Duration;

use jackin_core::{
    Account, AccountName, AccountQuery, AccountSelection, Agent, ChatgptAccountId, CodexAuthMode,
    Config, ConfigError, DockerCapability, DockerError, DockerLabel, DockerLayer, EnvFile,
    EnvFileError, EnvName, EnvTable, FileMode, GatewayName, LaunchId, NameSuffix, PathError,
    PlannedLabels, PolicyLabel, ProjectRoot, Provider, ProviderName, RoleManifest, RoleName,
    SandboxName, SandboxPath, SelectionError, Workspace, WorkspaceName, plain_list, sandbox_layout,
};
use jackin_image::{ImageError, ImageRequest, Rebuild};
use jackin_openshell::{
    ClosedPolicy, ClosedPolicyError, MainProcess, NetworkRule, PolicyError, SandboxCreate,
};
use thiserror::Error;

use crate::files::render_codex_config;
use crate::launch::LaunchWarning;
use crate::roles::LoadedRole;

const JACKIN_START: SandboxPath = SandboxPath::from_static("/usr/local/bin/jackin-start");
const ENV_FILE_MODE: FileMode = FileMode(0o600);

#[derive(Clone, Debug)]
pub struct LaunchRequest {
    gateway: GatewayName,
    project: ProjectRoot,
    role: RoleName,
    launch: LaunchId,
    account: Option<AccountName>,
    rebuild: Rebuild,
    env: BTreeMap<EnvName, String>,
    prompting: Prompting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvQuestion {
    pub name: EnvName,
    pub prompt: Option<String>,
    pub default: Option<String>,
}

impl fmt::Display for EnvQuestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)
    }
}

pub const DEFAULT_BUNDLE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const DEFAULT_EXPORT_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
pub const DEFAULT_EXPORT_OBJECTS: u32 = 5_000_000;
pub const DEFAULT_EXPORT_TOTAL_BYTES: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchLimits {
    pub ready: Duration,
    pub supervisor: Duration,
    pub attempt: Duration,
    pub hook: Duration,
    pub upload: Duration,
    pub create_attempts: NonZeroU8,
    pub bundle_bytes: u64,
    pub export_entry_bytes: u64,
    pub export_objects: u32,
    pub export_total_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prompting {
    Never,
    Interactive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedFile {
    pub path: SandboxPath,
    pub mode: FileMode,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedFiles {
    pub codex_config: PlannedFile,
    pub env: PlannedFile,
}

#[derive(Clone, Debug)]
pub struct LaunchPlan {
    launch_id: LaunchId,
    gateway: GatewayName,
    workspace: WorkspaceName,
    role: RoleName,
    selection: AccountSelection,
    account: Account,
    chatgpt_account: ChatgptAccountId,
    project: ProjectRoot,
    docker: DockerCapability,
    warnings: Vec<LaunchWarning>,
    image_request: ImageRequest,
    draft: SandboxDraft,
    files: PlannedFiles,
}

#[derive(Clone, Debug)]
pub struct SandboxDraft {
    name: SandboxName,
    labels: PlannedLabels,
    provider: ProviderName,
    policy: ClosedPolicy,
    main: MainProcess,
}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Selection(#[from] SelectionError),
    #[error(transparent)]
    Project(#[from] PathError),
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error(transparent)]
    Closure(#[from] ClosedPolicyError),
    #[error(transparent)]
    Docker(#[from] DockerError),
    #[error("environment values are needed: {}", plain_list(.0))]
    EnvNeeded(Vec<EnvQuestion>),
    #[error("environment names are not declared by the role: {}", plain_list(.0))]
    EnvUndeclared(Vec<EnvName>),
    #[error(transparent)]
    EnvFile(#[from] EnvFileError),
    #[error("account {account} uses {mode}, which is not supported yet")]
    Unsupported {
        account: AccountName,
        mode: CodexAuthMode,
    },
    #[error(transparent)]
    CodexConfig(#[from] toml::ser::Error),
    #[error("the request names role {requested} but the loaded role is {loaded}")]
    RoleMismatch {
        requested: RoleName,
        loaded: RoleName,
    },
    #[error("workspace {workspace} does not allow role {role}")]
    RoleNotAllowed {
        workspace: WorkspaceName,
        role: RoleName,
    },
}

impl Prompting {
    pub fn for_terminal(is_tty: bool) -> Prompting {
        if is_tty {
            Prompting::Interactive
        } else {
            Prompting::Never
        }
    }
}

impl LaunchRequest {
    pub fn new(
        gateway: GatewayName,
        project: ProjectRoot,
        role: RoleName,
        launch: LaunchId,
    ) -> LaunchRequest {
        LaunchRequest {
            gateway,
            project,
            role,
            launch,
            account: None,
            rebuild: Rebuild::No,
            env: BTreeMap::new(),
            prompting: Prompting::Never,
        }
    }

    #[must_use]
    pub fn account(mut self, account: Option<AccountName>) -> LaunchRequest {
        self.account = account;
        self
    }

    #[must_use]
    pub fn rebuild(mut self, rebuild: Rebuild) -> LaunchRequest {
        self.rebuild = rebuild;
        self
    }

    #[must_use]
    pub fn env(mut self, values: BTreeMap<EnvName, String>) -> LaunchRequest {
        self.env = values;
        self
    }

    #[must_use]
    pub fn prompting(mut self, prompting: Prompting) -> LaunchRequest {
        self.prompting = prompting;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnvLayers<'a> {
    pub global: &'a EnvTable,
    pub workspace: &'a EnvTable,
    pub workspace_role: Option<&'a EnvTable>,
}

impl<'a> EnvLayers<'a> {
    pub fn of(config: &'a Config, workspace: &'a Workspace, role: &RoleName) -> EnvLayers<'a> {
        EnvLayers {
            global: config.env(),
            workspace: &workspace.env,
            workspace_role: workspace.role_env.get(role),
        }
    }
}

pub fn resolve_env(
    layers: &EnvLayers<'_>,
    manifest: &RoleManifest,
    answers: &BTreeMap<EnvName, String>,
    prompting: Prompting,
) -> Result<BTreeMap<EnvName, String>, PlanError> {
    let undeclared: Vec<EnvName> = answers
        .keys()
        .filter(|name| !manifest.env.contains_key(*name))
        .cloned()
        .collect();
    if !undeclared.is_empty() {
        return Err(PlanError::EnvUndeclared(undeclared));
    }
    let mut values: BTreeMap<EnvName, String> = BTreeMap::new();
    for table in [
        Some(layers.global),
        Some(layers.workspace),
        layers.workspace_role,
    ]
    .into_iter()
    .flatten()
    {
        values.extend(
            table
                .iter()
                .map(|(name, value)| (name.name().clone(), value.as_str().to_owned())),
        );
    }
    let mut questions = Vec::new();
    for (name, spec) in &manifest.env {
        if let Some(value) = answers.get(name) {
            values.insert(name.clone(), value.clone());
            continue;
        }
        if values.contains_key(name) {
            continue;
        }
        match (prompting, &spec.default) {
            (Prompting::Never, Some(default)) => {
                values.insert(name.clone(), default.clone());
            }
            (Prompting::Never | Prompting::Interactive, _) => questions.push(EnvQuestion {
                name: name.clone(),
                prompt: spec.prompt.clone(),
                default: spec.default.clone(),
            }),
        }
    }
    if questions.is_empty() {
        Ok(values)
    } else {
        Err(PlanError::EnvNeeded(questions))
    }
}

pub fn plan_image(
    config: &Config,
    role: &LoadedRole,
    gateway: &GatewayName,
    workspace: Option<&Workspace>,
    rebuild: Rebuild,
) -> Result<ImageRequest, PlanError> {
    let entry = config
        .gateway(gateway)
        .ok_or_else(|| ConfigError::UnknownGateway(gateway.clone()))?;
    let layer = DockerLayer::from_flags(
        role.manifest().docker.enabled,
        workspace.is_some_and(|found| found.docker),
    );
    Ok(ImageRequest {
        role: role.name().clone(),
        context_files: role.files().to_vec(),
        dockerfile: role.dockerfile().clone(),
        hooks: role.hooks().clone(),
        revision: role.revision().clone(),
        agent: Agent::Codex,
        platform: entry.platform,
        builder: entry.builder.clone(),
        docker: layer,
        rebuild,
    })
}

fn launch_policy(
    role: &LoadedRole,
    docker: &DockerCapability,
) -> Result<(ClosedPolicy, Vec<LaunchWarning>), PlanError> {
    let (document, warnings) = match docker {
        DockerCapability::Enabled(access) => (
            role.policy()
                .with_rule(NetworkRule::docker_engine(access.engine.port))?,
            vec![LaunchWarning::DockerCapability],
        ),
        DockerCapability::Disabled => (role.policy().clone(), Vec::new()),
    };
    Ok((ClosedPolicy::parse(&document.render()?)?, warnings))
}

pub fn plan_launch(
    config: &Config,
    role: &LoadedRole,
    request: LaunchRequest,
) -> Result<LaunchPlan, PlanError> {
    let (gateway, entry) = config.resolve_gateway(Some(&request.gateway))?;
    let (workspace_name, workspace) = config.workspace_for(request.project.path())?;
    if &request.role != role.name() {
        return Err(PlanError::RoleMismatch {
            requested: request.role.clone(),
            loaded: role.name().clone(),
        });
    }
    if !workspace.roles.allows(&request.role) {
        return Err(PlanError::RoleNotAllowed {
            workspace: workspace_name.clone(),
            role: request.role.clone(),
        });
    }
    let agent = Agent::Codex;
    let spec = agent.spec();
    let selection = config.select_account(&AccountQuery {
        flag: request.account.as_ref(),
        workspace: workspace_name,
        role: &request.role,
        agent,
    })?;
    let account =
        config
            .account(&selection.account)
            .ok_or_else(|| ConfigError::MissingReference {
                kind: "account",
                name: selection.account.to_string(),
            })?;
    let mode = account
        .codex_auth_mode()
        .ok_or_else(|| SelectionError::FlagProviderMismatch {
            account: selection.account.clone(),
            expected: Provider::Codex,
        })?;
    let chatgpt_account = account
        .chatgpt_login()
        .map(|login| login.account_id.clone())
        .ok_or_else(|| PlanError::Unsupported {
            account: selection.account.clone(),
            mode,
        })?;
    let docker = DockerCapability::resolve(
        role.manifest().docker.enabled,
        workspace.docker,
        entry.docker_engine.as_ref(),
    )?;
    let (policy, warnings) = launch_policy(role, &docker)?;
    let policy_label = PolicyLabel::of(&policy.document().render()?);
    let layers = EnvLayers::of(config, workspace, &request.role);
    let mut values = resolve_env(&layers, role.manifest(), &request.env, request.prompting)?;
    values.extend(docker.environment());
    let files = PlannedFiles {
        codex_config: PlannedFile {
            path: spec.user_config.path.clone(),
            mode: spec.user_config.mode,
            text: render_codex_config(role.manifest().codex.model.as_ref(), &spec.workdir)?,
        },
        env: PlannedFile {
            path: sandbox_layout::env_file(&request.launch),
            mode: ENV_FILE_MODE,
            text: EnvFile::new(values)?.render(),
        },
    };
    let image_request = plan_image(config, role, gateway, Some(workspace), request.rebuild)?;
    let name = SandboxName::new(
        &workspace_name.slug(),
        &NameSuffix::for_launch(&request.launch),
    );
    let draft = SandboxDraft {
        name,
        labels: PlannedLabels {
            workspace: workspace_name.clone(),
            role: request.role.clone(),
            agent,
            account: selection.account.clone(),
            policy: policy_label,
            launch: request.launch.clone(),
        },
        provider: ProviderName::for_account(&selection.account, agent),
        policy,
        main: MainProcess {
            program: JACKIN_START,
            args: spec.launch_argv(),
        },
    };
    Ok(LaunchPlan {
        launch_id: request.launch,
        gateway: gateway.clone(),
        workspace: workspace_name.clone(),
        role: request.role,
        account: account.clone(),
        chatgpt_account,
        selection,
        project: request.project,
        docker,
        warnings,
        image_request,
        draft,
        files,
    })
}

impl LaunchPlan {
    pub fn launch_id(&self) -> &LaunchId {
        &self.launch_id
    }

    pub fn gateway(&self) -> &GatewayName {
        &self.gateway
    }

    pub fn sandbox_name(&self) -> &SandboxName {
        &self.draft.name
    }

    pub fn workspace(&self) -> &WorkspaceName {
        &self.workspace
    }

    pub fn role(&self) -> &RoleName {
        &self.role
    }

    pub fn account(&self) -> &AccountSelection {
        &self.selection
    }

    pub fn account_entry(&self) -> &Account {
        &self.account
    }

    pub fn chatgpt_account(&self) -> &ChatgptAccountId {
        &self.chatgpt_account
    }

    pub fn project(&self) -> &ProjectRoot {
        &self.project
    }

    pub fn docker(&self) -> &DockerCapability {
        &self.docker
    }

    pub fn warnings(&self) -> &[LaunchWarning] {
        &self.warnings
    }

    pub fn image_request(&self) -> &ImageRequest {
        &self.image_request
    }

    pub fn draft(&self) -> &SandboxDraft {
        &self.draft
    }

    pub fn files(&self) -> &PlannedFiles {
        &self.files
    }
}

impl SandboxDraft {
    pub fn name(&self) -> &SandboxName {
        &self.name
    }

    pub fn labels(&self) -> &PlannedLabels {
        &self.labels
    }

    pub fn provider(&self) -> &ProviderName {
        &self.provider
    }

    pub fn complete(&self, image: &jackin_image::BuiltImage, docker: DockerLabel) -> SandboxCreate {
        SandboxCreate::new(
            self.name.clone(),
            image.tag.clone(),
            self.labels.clone().complete(&image.id, docker),
            vec![self.provider.clone()],
            self.policy.clone(),
            self.main.clone(),
        )
    }
}
