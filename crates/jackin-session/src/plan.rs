// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU8;
use std::time::Duration;

use jackin_core::{
    Account, AccountName, AccountQuery, AccountSelection, Agent, ChatgptAccountId, CodexAuthMode,
    Config, ConfigError, DockerCapability, DockerError, DockerLabel, DockerLayer, EnvFile,
    EnvFileError, EnvName, EnvTable, EnvValueError, FileMode, GatewayName, LaunchId, ModelName,
    NameSuffix, PathError, PlainEnvValue, PlannedLabels, PolicyLabel, ProjectRoot, Provider,
    ProviderName, RoleManifest, RoleName, SandboxName, SandboxPath, SelectionError, Workspace,
    WorkspaceName, plain, plain_list, sandbox_layout,
};
use jackin_image::{ImageError, ImageRequest, Rebuild};
use jackin_openshell::{
    ClosedPolicy, ClosedPolicyError, MainProcess, NetworkRule, PolicyError, SandboxCreate,
};
use thiserror::Error;

use crate::files::{placeholder_file, render_codex_config, render_muse_trust};
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
    agent: Agent,
    provider: Provider,
    model: Option<String>,
    instances: Option<jackin_core::AgentInstances>,
    task: Option<String>,
    rebuild: Rebuild,
    env: BTreeMap<EnvName, PlainEnvValue>,
    prompting: Prompting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvQuestion {
    pub name: EnvName,
    pub prompt: Option<String>,
    pub default: Option<PlainEnvValue>,
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
    pub omp_models: Option<PlannedFile>,
    pub auth: PlannedFile,
    pub muse_trust: Option<PlannedFile>,
    pub multi: Vec<PlannedFile>,
    pub layout: crate::multi::MultiLayout,
}

#[derive(Clone, Debug)]
pub struct LaunchPlan {
    launch_id: LaunchId,
    gateway: GatewayName,
    workspace: WorkspaceName,
    role: RoleName,
    selection: AccountSelection,
    account: Account,
    instance_accounts: Vec<Account>,
    chatgpt_accounts: Vec<(Agent, ChatgptAccountId)>,
    chatgpt_account: Option<ChatgptAccountId>,
    project: ProjectRoot,
    docker: DockerCapability,
    warnings: Vec<LaunchWarning>,
    image_request: ImageRequest,
    draft: SandboxDraft,
    files: PlannedFiles,
    instances: jackin_core::AgentInstances,
}

#[derive(Clone, Debug)]
pub struct SandboxDraft {
    name: SandboxName,
    labels: PlannedLabels,
    provider: ProviderName,
    providers: Vec<ProviderName>,
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
    #[error("the default of {name} in the role manifest is refused")]
    EnvDefault {
        name: EnvName,
        #[source]
        source: EnvValueError,
    },
    #[error(transparent)]
    EnvFile(#[from] EnvFileError),
    #[error("account {account} uses {mode}, which is not supported yet")]
    Unsupported {
        account: AccountName,
        mode: CodexAuthMode,
    },
    #[error(transparent)]
    CodexConfig(#[from] toml::ser::Error),
    #[error(transparent)]
    Manifest(#[from] jackin_core::ManifestError),
    #[error("the request names role {requested} but the loaded role is {loaded}")]
    RoleMismatch {
        requested: RoleName,
        loaded: RoleName,
    },
    #[error("tuple {} with provider {} is not supported: {}", .agent.as_str(), .provider.as_str(), plain(.reason))]
    TupleNotSupported {
        agent: Agent,
        provider: Provider,
        reason: String,
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
            agent: Agent::Codex,
            provider: Provider::Codex,
            model: None,
            instances: None,
            task: None,
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
    pub fn tuple(mut self, agent: Agent, provider: Provider) -> LaunchRequest {
        self.agent = agent;
        self.provider = provider;
        self
    }

    #[must_use]
    pub fn instances(mut self, instances: jackin_core::AgentInstances) -> LaunchRequest {
        if let Some(lead) = instances.lead_opt() {
            self.agent = lead.agent;
            self.provider = lead.provider;
        }
        self.instances = Some(instances);
        self
    }

    #[must_use]
    pub fn task(mut self, task: Option<String>) -> LaunchRequest {
        self.task = task;
        self
    }

    pub fn task_text(&self) -> Option<&str> {
        self.task.as_deref()
    }

    #[must_use]
    pub fn model(mut self, model: Option<String>) -> LaunchRequest {
        self.model = model;
        self
    }

    pub fn agent(&self) -> Agent {
        self.agent
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    #[must_use]
    pub fn rebuild(mut self, rebuild: Rebuild) -> LaunchRequest {
        self.rebuild = rebuild;
        self
    }

    #[must_use]
    pub fn env(mut self, values: BTreeMap<EnvName, PlainEnvValue>) -> LaunchRequest {
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
    answers: &BTreeMap<EnvName, PlainEnvValue>,
    prompting: Prompting,
) -> Result<BTreeMap<EnvName, PlainEnvValue>, PlanError> {
    let undeclared: Vec<EnvName> = answers
        .keys()
        .filter(|name| !manifest.env.contains_key(*name))
        .cloned()
        .collect();
    if !undeclared.is_empty() {
        return Err(PlanError::EnvUndeclared(undeclared));
    }
    let mut values: BTreeMap<EnvName, PlainEnvValue> = BTreeMap::new();
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
                .map(|(name, value)| (name.name().clone(), value.clone())),
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
        let default = spec
            .default
            .as_deref()
            .map(PlainEnvValue::parse)
            .transpose()
            .map_err(|source| PlanError::EnvDefault {
                name: name.clone(),
                source,
            })?;
        match (prompting, default) {
            (Prompting::Never, Some(default)) => {
                values.insert(name.clone(), default);
            }
            (Prompting::Never | Prompting::Interactive, default) => {
                questions.push(EnvQuestion {
                    name: name.clone(),
                    prompt: spec.prompt.clone(),
                    default,
                });
            }
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
    plan_image_for(config, role, gateway, workspace, rebuild, Agent::Codex)
}

pub fn plan_image_for(
    config: &Config,
    role: &LoadedRole,
    gateway: &GatewayName,
    workspace: Option<&Workspace>,
    rebuild: Rebuild,
    agent: Agent,
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
        agent,
        agents: vec![agent],
        platform: entry.platform,
        builder: entry.builder.clone(),
        docker: layer,
        rebuild,
    })
}

fn launch_policy(
    role: &LoadedRole,
    docker: &DockerCapability,
    instances: &jackin_core::AgentInstances,
) -> Result<(ClosedPolicy, Vec<LaunchWarning>), PlanError> {
    let (document, warnings) = match docker {
        DockerCapability::Enabled(access) => (
            role.policy()
                .with_rule(NetworkRule::docker_engine(access.engine.port))?,
            vec![LaunchWarning::DockerCapability],
        ),
        DockerCapability::Disabled => (role.policy().clone(), Vec::new()),
    };
    let document = if matches!(docker, DockerCapability::Disabled)
        && instances
            .iter()
            .any(|instance| instance.agent == Agent::MuseCode)
    {
        document.with_read_only("/")?
    } else {
        document
    };
    Ok((document.close_for_instances(instances)?, warnings))
}

fn apply_agent_env_defaults(
    instances: &jackin_core::AgentInstances,
    values: &mut BTreeMap<EnvName, String>,
) {
    if instances
        .iter()
        .any(|instance| instance.agent == Agent::MimoCode)
    {
        if let Ok(name) = EnvName::parse("TMPDIR") {
            values.entry(name).or_insert_with(|| "/tmp".to_owned());
        }
    }
}

fn lead_tuple(
    agent: Agent,
    provider: Provider,
    model: Option<&str>,
) -> Result<&'static jackin_core::AgentTuple, PlanError> {
    let tuple =
        jackin_core::lookup_tuple(agent, provider).map_err(|_| PlanError::TupleNotSupported {
            agent,
            provider,
            reason: "unknown tuple".to_owned(),
        })?;
    if tuple.state != jackin_core::TupleState::Supported {
        return Err(PlanError::TupleNotSupported {
            agent,
            provider,
            reason: tuple.reason.unwrap_or("not supported").to_owned(),
        });
    }
    if let Some(model) = model {
        tuple
            .check_model(model)
            .map_err(|_| PlanError::TupleNotSupported {
                agent,
                provider,
                reason: format!("unknown model {model}"),
            })?;
    }
    Ok(tuple)
}

fn lead_agent_provider(request: &LaunchRequest) -> (Agent, Provider) {
    match request
        .instances
        .as_ref()
        .and_then(|instances| instances.lead_opt())
    {
        Some(lead) => (lead.agent, lead.provider),
        None => (request.agent, request.provider),
    }
}

fn validate_instances(instances: &jackin_core::AgentInstances) -> Result<(), PlanError> {
    for instance in instances.iter() {
        let tuple = jackin_core::lookup_tuple(instance.agent, instance.provider).map_err(|_| {
            PlanError::TupleNotSupported {
                agent: instance.agent,
                provider: instance.provider,
                reason: "unknown tuple".to_owned(),
            }
        })?;
        if tuple.state != jackin_core::TupleState::Supported {
            return Err(PlanError::TupleNotSupported {
                agent: instance.agent,
                provider: instance.provider,
                reason: tuple.reason.unwrap_or("not supported").to_owned(),
            });
        }
        tuple
            .check_model(&instance.model)
            .map_err(|_| PlanError::TupleNotSupported {
                agent: instance.agent,
                provider: instance.provider,
                reason: format!("unknown model {}", instance.model),
            })?;
    }
    Ok(())
}

fn validate_instruction_mappings(
    role: &LoadedRole,
    instances: &jackin_core::AgentInstances,
) -> Result<(), PlanError> {
    for instance in instances.iter() {
        if !role.manifest().agents.contains_key(&instance.agent) {
            return Err(jackin_core::ManifestError::NoInstructionMapping {
                role: role.name().clone(),
                agent: instance.agent,
            }
            .into());
        }
    }
    Ok(())
}

fn agent_config_file(
    is_omp: bool,
    tuple: &'static jackin_core::AgentTuple,
    codex_model: Option<&ModelName>,
    model: Option<&str>,
    agent: Agent,
    provider: Provider,
    account: &AccountName,
) -> Result<PlannedFile, PlanError> {
    if agent == Agent::KimiCode {
        return Ok(crate::files::render_kimi_config(
            &ProviderName::for_account(account, agent),
        ));
    }
    if agent == Agent::MimoCode {
        return Ok(crate::files::render_mimo_config());
    }
    if agent == Agent::MuseCode {
        return Ok(crate::files::render_muse_config(model));
    }
    if agent == Agent::GrokBuild {
        return Ok(crate::files::render_grok_config(model));
    }
    if agent == Agent::ClaudeCode {
        return Ok(crate::files::claude_config_file(tuple));
    }
    if is_omp {
        let spec = agent.spec();
        return Ok(PlannedFile {
            path: spec.user_config.path.clone(),
            mode: spec.user_config.mode,
            text: crate::files::render_omp_config(tuple, model).map_err(|_| {
                PlanError::TupleNotSupported {
                    agent,
                    provider,
                    reason: "unknown model".to_owned(),
                }
            })?,
        });
    }
    let spec = agent.spec();
    Ok(PlannedFile {
        path: spec.user_config.path.clone(),
        mode: spec.user_config.mode,
        text: render_codex_config(tuple, codex_model, model, &spec.workdir)?,
    })
}

fn muse_trust_file(agent: Agent, spec: &jackin_core::AgentSpec) -> Option<PlannedFile> {
    match agent {
        Agent::MuseCode => Some(render_muse_trust(&spec.workdir)),
        _ => None,
    }
}

fn plan_omp_models(
    instances: &jackin_core::AgentInstances,
) -> Result<Option<PlannedFile>, PlanError> {
    instances
        .iter()
        .find(|instance| instance.agent == Agent::Omp)
        .map(|instance| {
            let tuple = jackin_core::lookup_tuple(Agent::Omp, instance.provider).map_err(|_| {
                PlanError::TupleNotSupported {
                    agent: Agent::Omp,
                    provider: instance.provider,
                    reason: "unknown tuple".to_owned(),
                }
            })?;
            crate::files::omp_models_file(tuple, Some(instance.model.as_str()))
        })
        .transpose()
}

type InstanceAccounts = (Vec<Account>, Vec<(Agent, ChatgptAccountId)>);

fn collect_instance_accounts(
    config: &Config,
    instances: &jackin_core::AgentInstances,
) -> Result<InstanceAccounts, PlanError> {
    let mut instance_accounts = Vec::new();
    let mut chatgpt_accounts = Vec::new();
    for instance in instances.iter() {
        let entry =
            config
                .account(&instance.account)
                .ok_or_else(|| ConfigError::MissingReference {
                    kind: "account",
                    name: instance.account.to_string(),
                })?;
        instance_accounts.push(entry.clone());
        if instance.agent == Agent::Codex
            && let Some(login) = entry.chatgpt_login()
        {
            chatgpt_accounts.push((instance.agent, login.account_id.clone()));
        }
    }
    Ok((instance_accounts, chatgpt_accounts))
}

fn lead_chatgpt_account(
    agent: Agent,
    provider: Provider,
    account: &Account,
    selection: &AccountSelection,
) -> Result<Option<ChatgptAccountId>, PlanError> {
    Ok(if agent == Agent::Codex && provider == Provider::Codex {
        let mode =
            account
                .codex_auth_mode()
                .ok_or_else(|| SelectionError::FlagProviderMismatch {
                    account: selection.account.clone(),
                    expected: Provider::Codex,
                })?;
        Some(
            account
                .chatgpt_login()
                .map(|login| login.account_id.clone())
                .ok_or_else(|| PlanError::Unsupported {
                    account: selection.account.clone(),
                    mode,
                })?,
        )
    } else {
        None
    })
}

fn resolve_instances(
    request: &LaunchRequest,
    agent: Agent,
    provider: Provider,
    selection: &AccountSelection,
) -> jackin_core::AgentInstances {
    match request.instances.clone() {
        Some(instances) => instances,
        None => jackin_core::AgentInstances::single(jackin_core::AgentInstance {
            agent,
            provider,
            account: selection.account.clone(),
            model: request
                .model
                .clone()
                .unwrap_or_else(|| agent.spec().default_model.to_owned()),
            branch: format!("agent/{}", agent.as_str()),
        }),
    }
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
    let (agent, provider) = lead_agent_provider(&request);
    let tuple = lead_tuple(agent, provider, request.model.as_deref())?;
    let spec = agent.spec();
    let lead_flag = match request
        .instances
        .as_ref()
        .and_then(|items| items.lead_opt())
    {
        Some(lead) => Some(lead.account.clone()),
        None => request.account.clone(),
    };
    let selection = config.select_account(&AccountQuery {
        flag: lead_flag.as_ref(),
        workspace: workspace_name,
        role: &request.role,
        agent,
        provider,
    })?;
    let instances = resolve_instances(&request, agent, provider, &selection);
    validate_instances(&instances)?;
    validate_instruction_mappings(role, &instances)?;
    let account =
        config
            .account(&selection.account)
            .ok_or_else(|| ConfigError::MissingReference {
                kind: "account",
                name: selection.account.to_string(),
            })?;
    let (instance_accounts, chatgpt_accounts) = collect_instance_accounts(config, &instances)?;
    let is_omp = agent == Agent::Omp;
    let chatgpt_account = lead_chatgpt_account(agent, provider, account, &selection)?;
    let docker = DockerCapability::resolve(
        role.manifest().docker.enabled,
        workspace.docker,
        entry.docker_engine.as_ref(),
    )?;
    let (policy, warnings) = launch_policy(role, &docker, &instances)?;
    let policy_label = PolicyLabel::of(&policy.document().render()?);
    let layers = EnvLayers::of(config, workspace, &request.role);
    let mut values: BTreeMap<EnvName, String> =
        resolve_env(&layers, role.manifest(), &request.env, request.prompting)?
            .into_iter()
            .map(|(name, value)| (name, value.as_str().to_owned()))
            .collect();
    values.extend(docker.environment());
    apply_agent_env_defaults(&instances, &mut values);
    let layout = crate::multi::plan_multi(
        &instances,
        request.task.as_deref(),
        crate::multi::HeadState::Born,
    )?;
    let files = PlannedFiles {
        multi: layout.files.clone(),
        layout: layout.clone(),
        codex_config: agent_config_file(
            is_omp,
            tuple,
            role.manifest()
                .agents
                .get(&Agent::Codex)
                .and_then(|section| section.model.as_ref()),
            Some(instances.lead().model.as_str()),
            agent,
            provider,
            &selection.account,
        )?,
        env: PlannedFile {
            path: sandbox_layout::env_file(&request.launch),
            mode: ENV_FILE_MODE,
            text: EnvFile::new(values)?.render(),
        },
        auth: if is_omp {
            crate::files::omp_auth_file(tuple)
        } else {
            placeholder_file(spec)
        },
        omp_models: plan_omp_models(&instances)?,
        muse_trust: muse_trust_file(agent, spec),
    };
    let image_request = plan_image_for(
        config,
        role,
        gateway,
        Some(workspace),
        request.rebuild,
        agent,
    )?;
    let mut image_request = image_request;
    image_request.agents = instances.iter().map(|i| i.agent).collect();
    let name = SandboxName::new(
        &workspace_name.slug(),
        &NameSuffix::for_launch(&request.launch),
    );
    let draft = SandboxDraft {
        name,
        labels: PlannedLabels {
            workspace: workspace_name.clone(),
            role: request.role.clone(),
            agents: instances.clone(),
            policy: policy_label,
            launch: request.launch.clone(),
        },
        provider: ProviderName::for_account(&selection.account, agent),
        providers: instances
            .iter()
            .map(|i| ProviderName::for_account(&i.account, i.agent))
            .collect(),
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
        instance_accounts,
        chatgpt_accounts,
        project: request.project,
        docker,
        warnings,
        image_request,
        draft,
        files,
        instances,
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

    pub fn chatgpt_account(&self) -> Option<&ChatgptAccountId> {
        self.chatgpt_account.as_ref()
    }

    pub fn chatgpt_account_for(&self, agent: Agent) -> Option<&ChatgptAccountId> {
        self.chatgpt_accounts
            .iter()
            .find(|(found, _)| *found == agent)
            .map(|(_, id)| id)
    }

    pub fn instance_accounts(&self) -> &[Account] {
        &self.instance_accounts
    }

    pub fn is_omp(&self) -> bool {
        self.instances
            .iter()
            .any(|instance| instance.agent == Agent::Omp)
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

    pub fn instances(&self) -> &jackin_core::AgentInstances {
        &self.instances
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

    pub fn providers(&self) -> &[ProviderName] {
        &self.providers
    }

    pub fn complete(&self, image: &jackin_image::BuiltImage, docker: DockerLabel) -> SandboxCreate {
        SandboxCreate::new(
            self.name.clone(),
            image.tag.clone(),
            self.labels.clone().complete(&image.id, docker),
            if self.providers.is_empty() {
                vec![self.provider.clone()]
            } else {
                self.providers.clone()
            },
            self.policy.clone(),
            self.main.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU16;
    use std::sync::Arc;

    use jackin_core::{
        AgentInstance, AgentInstances, DockerSource, EngineAccess, EngineConfig, HostPath,
        StdFileSystem,
    };
    use jackin_testkit::{FakeClock, RoleFixture, TestRunner};

    fn fixture(
        agent: Agent,
        provider: Provider,
        codex_model: Option<&ModelName>,
        model: Option<&str>,
    ) -> PlannedFile {
        let tuple = jackin_core::lookup_tuple(agent, provider).expect("tuple");
        let account = AccountName::parse("cfgtest").expect("account");
        agent_config_file(false, tuple, codex_model, model, agent, provider, &account)
            .expect("config")
    }

    #[test]
    fn agent_config_muse_json() {
        let file = fixture(
            Agent::MuseCode,
            Provider::Muse,
            None,
            Some("muse-spark-1.2"),
        );
        assert!(file.path.as_str().ends_with("settings.json"));
        let value: serde_json::Value = serde_json::from_str(&file.text).expect("json");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["provider"], "meta");
        assert_eq!(value["model"], "muse-spark-1.2");
    }

    #[test]
    fn agent_config_muse_default_model() {
        let file = fixture(Agent::MuseCode, Provider::Muse, None, None);
        let value: serde_json::Value = serde_json::from_str(&file.text).expect("json");
        assert_eq!(value["model"], Agent::MuseCode.spec().default_model);
    }

    #[test]
    fn muse_trust_file_only_for_muse() {
        let spec = Agent::MuseCode.spec();
        let file = muse_trust_file(Agent::MuseCode, spec).expect("muse trust");
        assert_eq!(
            file.path,
            SandboxPath::parse("/sandbox/.config/muse/trust.json").expect("path")
        );
        assert_eq!(file.mode, FileMode(0o600));
        let value: serde_json::Value = serde_json::from_str(&file.text).expect("json");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(
            value["projects"][spec.workdir.as_str()]["decision"],
            "trusted"
        );
        assert!(muse_trust_file(Agent::Codex, Agent::Codex.spec()).is_none());
    }

    #[test]
    fn muse_trust_renderer_uses_given_workdir() {
        let workdir = SandboxPath::parse("/sandbox/custom").expect("workdir");
        let file = render_muse_trust(&workdir);
        let value: serde_json::Value = serde_json::from_str(&file.text).expect("json");
        assert_eq!(value["projects"]["/sandbox/custom"]["decision"], "trusted");
        assert_eq!(value["projects"].as_object().expect("projects").len(), 1);
    }

    #[test]
    fn agent_config_grok_toml() {
        let file = fixture(Agent::GrokBuild, Provider::Grok, None, Some("grok-4.7"));
        assert!(file.path.as_str().ends_with("config.toml"));
        let value: toml::Table = toml::from_str(&file.text).expect("toml");
        assert_eq!(value["cli"]["auto_update"], toml::Value::Boolean(false));
        assert_eq!(
            value["models"]["default"],
            toml::Value::String("grok-4.7".to_owned())
        );
    }

    #[test]
    fn agent_config_grok_no_model() {
        let file = fixture(Agent::GrokBuild, Provider::Grok, None, None);
        let value: toml::Table = toml::from_str(&file.text).expect("toml");
        assert_eq!(value["cli"]["auto_update"], toml::Value::Boolean(false));
        assert!(!value.contains_key("models"));
    }

    #[test]
    fn agent_config_codex_stays_toml() {
        let codex_model = ModelName::parse("gpt-5.1-codex").expect("model");
        let file = fixture(
            Agent::Codex,
            Provider::Codex,
            Some(&codex_model),
            Some("gpt-5.1-codex"),
        );
        let value: toml::Table = toml::from_str(&file.text).expect("toml");
        assert!(value.contains_key("model"));
        let projects = value["projects"].as_table().expect("projects");
        assert!(
            projects
                .values()
                .any(|entry| entry["trust_level"] == toml::Value::String("trusted".to_owned()))
        );
    }

    #[test]
    fn agent_config_claude_lead_slot_pins_the_settings_file() {
        for (provider, model) in [
            (Provider::Claude, "haiku"),
            (Provider::Kimi, "kimi-for-coding"),
            (Provider::Zai, "glm-5.3"),
            (Provider::Minimax, "minimax-m3"),
            (Provider::Mimo, "mimo-v2.6-pro"),
        ] {
            let tuple = jackin_core::lookup_tuple(Agent::ClaudeCode, provider).expect("tuple");
            let file = fixture(Agent::ClaudeCode, provider, None, Some(model));
            assert_eq!(
                file.path,
                SandboxPath::parse("/sandbox/.claude/settings.json").expect("path"),
                "{}",
                tuple.id
            );
            assert_eq!(file.mode, FileMode(0o600), "{}", tuple.id);
            assert_eq!(
                file.text,
                crate::files::render_claude_config(tuple),
                "{}",
                tuple.id
            );
        }
    }

    fn role_policy(read_only: &[&str]) -> String {
        use std::fmt::Write as _;

        let entries: String = read_only.iter().fold(String::new(), |mut out, path| {
            writeln!(out, "    - {path}").expect("write to a string");
            out
        });
        format!(
            "version: 1\nfilesystem_policy:\n  include_workdir: true\n  read_only:\n{entries}  read_write:\n    - /tmp\n    - /sandbox\nlandlock:\n  compatibility: hard_requirement\nprocess:\n  run_as_user: sandbox\n  run_as_group: sandbox\nnetwork_policies:\n  github:\n    name: github\n    endpoints:\n      - host: github.com\n        port: 443\n    binaries:\n      - path: /usr/bin/git\n"
        )
    }

    async fn loaded_role(policy: &str) -> LoadedRole {
        let fixture = RoleFixture::new("sentinel").expect("role fixture");
        let fixture = fixture.policy_yaml(policy);
        fixture.commit().await.expect("role commit");
        let cache = tempfile::tempdir().expect("cache dir");
        let cache = HostPath::parse(&std::fs::canonicalize(cache.path()).expect("cache path"))
            .expect("cache");
        let catalog = crate::roles::RoleCatalog::new(
            Arc::new(StdFileSystem::default()),
            Arc::new(TestRunner::new()),
            Arc::new(FakeClock::new()),
            cache,
        );
        catalog
            .load_path(fixture.root(), &tokio_util::sync::CancellationToken::new())
            .await
            .expect("role load")
    }

    fn read_only_entries(policy: &ClosedPolicy) -> Vec<String> {
        let rendered = policy.document().render().expect("render");
        let value: noyalib::Value = noyalib::from_str(&rendered).expect("yaml");
        value
            .get("filesystem_policy")
            .and_then(|filesystem| filesystem.get("read_only"))
            .and_then(noyalib::Value::as_sequence)
            .map(|items| {
                items
                    .iter()
                    .filter_map(noyalib::Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn account() -> AccountName {
        AccountName::parse("work").expect("account")
    }

    fn muse_instances() -> AgentInstances {
        AgentInstances::single(AgentInstance::lead(Agent::MuseCode, account()))
    }

    #[tokio::test]
    async fn muse_lead_instance_gains_root_read_only_at_launch() {
        let role = loaded_role(&role_policy(&["/bin", "/usr"])).await;
        let (policy, warnings) =
            launch_policy(&role, &DockerCapability::Disabled, &muse_instances()).expect("policy");
        assert_eq!(warnings, Vec::new());
        assert_eq!(read_only_entries(&policy), ["/bin", "/usr", "/"]);
    }

    #[tokio::test]
    async fn muse_non_lead_instance_gains_root_read_only_at_launch() {
        let role = loaded_role(&role_policy(&["/bin", "/usr"])).await;
        let instances = AgentInstances::new(vec![
            AgentInstance::lead(Agent::Codex, account()),
            AgentInstance::lead(Agent::MuseCode, account()),
        ])
        .expect("instances");
        let (policy, warnings) =
            launch_policy(&role, &DockerCapability::Disabled, &instances).expect("policy");
        assert_eq!(warnings, Vec::new());
        assert_eq!(read_only_entries(&policy), ["/bin", "/usr", "/"]);
    }

    #[tokio::test]
    async fn docker_enabled_muse_launch_keeps_read_only_without_root() {
        let role = loaded_role(&role_policy(&["/bin", "/usr"])).await;
        let engine = EngineAccess {
            engine: EngineConfig {
                port: NonZeroU16::new(12376).expect("port"),
                client_certs: HostPath::parse(std::path::Path::new("/tmp/engine-certs"))
                    .expect("certs"),
            },
            enabled_by: DockerSource::Role,
        };
        let (policy, warnings) =
            launch_policy(&role, &DockerCapability::Enabled(engine), &muse_instances())
                .expect("policy");
        assert_eq!(warnings, [LaunchWarning::DockerCapability]);
        assert_eq!(read_only_entries(&policy), ["/bin", "/usr"]);
    }

    #[tokio::test]
    async fn role_granted_root_forms_gain_no_duplicate_for_muse() {
        for form in ["/", "//", "/."] {
            let role = loaded_role(&role_policy(&["/bin", "/usr", form])).await;
            let (policy, warnings) =
                launch_policy(&role, &DockerCapability::Disabled, &muse_instances())
                    .expect("policy");
            assert_eq!(warnings, Vec::new());
            assert_eq!(read_only_entries(&policy), ["/bin", "/usr", form], "{form}");
        }
    }

    #[tokio::test]
    async fn every_non_muse_tuple_keeps_read_only_unchanged() {
        let plain = loaded_role(&role_policy(&["/bin", "/usr"])).await;
        let granting = loaded_role(&role_policy(&["/bin", "/usr", "/"])).await;
        for tuple in jackin_core::TUPLES {
            if tuple.agent == Agent::MuseCode {
                continue;
            }
            let instances = AgentInstances::single(AgentInstance::new(
                tuple.agent,
                tuple.provider,
                account(),
                tuple.default_model.to_owned(),
            ));
            let (policy, warnings) =
                launch_policy(&plain, &DockerCapability::Disabled, &instances).expect("policy");
            assert_eq!(warnings, Vec::new());
            assert_eq!(read_only_entries(&policy), ["/bin", "/usr"], "{}", tuple.id);
            let (granted, warnings) =
                launch_policy(&granting, &DockerCapability::Disabled, &instances).expect("policy");
            assert_eq!(warnings, Vec::new());
            assert_eq!(
                read_only_entries(&granted),
                ["/bin", "/usr", "/"],
                "{}",
                tuple.id
            );
        }
    }

    fn mimo_instances() -> AgentInstances {
        AgentInstances::single(AgentInstance::lead(Agent::MimoCode, account()))
    }

    #[test]
    fn mimo_instance_gains_tmpdir_default_in_the_agent_env() {
        let mut values = BTreeMap::new();
        apply_agent_env_defaults(&mimo_instances(), &mut values);
        assert_eq!(
            values,
            BTreeMap::from([(
                EnvName::parse("TMPDIR").expect("env name"),
                "/tmp".to_owned()
            )])
        );
    }

    #[test]
    fn mimo_instance_keeps_an_existing_tmpdir_from_the_role_layers() {
        let mut values = BTreeMap::from([(
            EnvName::parse("TMPDIR").expect("env name"),
            "/sandbox/tmp".to_owned(),
        )]);
        apply_agent_env_defaults(&mimo_instances(), &mut values);
        assert_eq!(
            values,
            BTreeMap::from([(
                EnvName::parse("TMPDIR").expect("env name"),
                "/sandbox/tmp".to_owned()
            )])
        );
    }

    #[test]
    fn non_mimo_instances_get_no_tmpdir_default() {
        let instances = AgentInstances::single(AgentInstance::lead(Agent::Codex, account()));
        let mut values = BTreeMap::new();
        apply_agent_env_defaults(&instances, &mut values);
        assert_eq!(values, BTreeMap::new());
    }
}
