// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use jackin_console::arbiter::DiscoveryError;
use jackin_console::domain::account::{
    Account, AccountRegistry, CredentialSource, DetectedKind, LaunchBlock,
};
use jackin_console::domain::agent::{Agent, AuthMode, Provider};
use jackin_console::domain::docker::{DockerRiskText, DockerSetting, EngineEndpoint, EngineState};
use jackin_console::domain::instance::RepoRef;
use jackin_console::domain::instance::{
    DaemonSnapshot, Instance, InstanceEntry, InstanceId, InstanceStatus, SandboxPhase,
    SessionRecord, SessionStatus, UnavailableInstance,
};
use jackin_console::domain::onepassword::OpReference;
use jackin_console::domain::openshell as normalize;
use jackin_console::domain::role::{
    AccountOrder, RoleAdmin, RoleCheckState, RoleEnvEntry, RoleFinding, RoleManifestDoc, RoleRecord,
};
use jackin_console::domain::surfaces::{LiveFacts, Surfaces};
use jackin_console::domain::vocabulary::Rejection;
use jackin_console::domain::workspace::{
    AllowedRoles as ConsoleAllowedRoles, EnvVar, RoleEntry, RolePolicy, RoleSource,
    Workspace as ConsoleWorkspace,
};
use jackin_console::world::{
    DaemonHealth, GithubRepo, GlobalConfig, Snapshot, TrustKind, TrustRow,
};
use jackin_core::{Agent as CoreAgent, NameSuffix, Provider as CoreProvider, plain};
use jackin_core::{
    AllowedRoles, Config, CredentialSource as CoreCredential, DockerRisk, EnvTable, GatewayName,
    Phase, RoleName, RoleSource as CoreRoleSource, SandboxName, SandboxSummary, SupervisorReading,
    SupervisorState, WorkspaceName,
};
use jackin_session::{GithubOrigin, WorkState};
use thiserror::Error;

use crate::fleet::{Fleet, ObservedSandbox};
use crate::ids::IdTable;
use crate::persist::sentence;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum RoleDocError {
    #[error("{}", sentence(.0))]
    Rejected(Rejection),
    #[error("{}", plain(.0))]
    Unreadable(String),
}

pub type RoleDocs = BTreeMap<RoleName, Result<jackin_core::RoleManifest, RoleDocError>>;

pub type RoleChecks = BTreeMap<RoleName, Vec<RoleFinding>>;

pub type Origins = BTreeMap<WorkspaceName, GithubOrigin>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    pub version: String,
    pub config_path: String,
    pub home: String,
    pub now_secs: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectInput<'a> {
    pub config: &'a Config,
    pub fleet: &'a Fleet,
    pub facts: &'a Facts,
    pub roles: &'a RoleDocs,
    pub checks: &'a RoleChecks,
    pub origins: &'a Origins,
}

pub fn provider(provider: CoreProvider) -> Option<Provider> {
    Some(match provider {
        CoreProvider::Amp => Provider::Amp,
        CoreProvider::Antigravity => Provider::Antigravity,
        CoreProvider::Claude => Provider::Anthropic,
        CoreProvider::Codex => Provider::OpenAi,
        CoreProvider::Cursor => Provider::Cursor,
        CoreProvider::Gemini => Provider::Gemini,
        CoreProvider::Grok => Provider::XAi,
        CoreProvider::Hermes => Provider::Hermes,
        CoreProvider::Kimi => Provider::Moonshot,
        CoreProvider::Minimax => Provider::MiniMax,
        CoreProvider::Mimo => Provider::Mimo,
        CoreProvider::Muse => Provider::Muse,
        CoreProvider::Opencode => Provider::OpenCode,
        CoreProvider::Openrouter => Provider::Openrouter,
        CoreProvider::Zai => Provider::Zai,
    })
}

pub fn core_provider(provider: Provider) -> CoreProvider {
    match provider {
        Provider::Amp => CoreProvider::Amp,
        Provider::Antigravity => CoreProvider::Antigravity,
        Provider::Anthropic => CoreProvider::Claude,
        Provider::OpenAi => CoreProvider::Codex,
        Provider::Cursor => CoreProvider::Cursor,
        Provider::Gemini => CoreProvider::Gemini,
        Provider::XAi => CoreProvider::Grok,
        Provider::Hermes => CoreProvider::Hermes,
        Provider::Moonshot => CoreProvider::Kimi,
        Provider::MiniMax => CoreProvider::Minimax,
        Provider::Muse => CoreProvider::Muse,
        Provider::Mimo => CoreProvider::Mimo,
        Provider::OpenCode => CoreProvider::Opencode,
        Provider::Openrouter => CoreProvider::Openrouter,
        Provider::Zai => CoreProvider::Zai,
    }
}

pub(crate) fn agent(agent: CoreAgent) -> Agent {
    match agent {
        CoreAgent::Codex => Agent::Codex,
        CoreAgent::ClaudeCode => Agent::ClaudeCode,
        CoreAgent::GrokBuild => Agent::GrokBuild,
        CoreAgent::MuseCode => Agent::MuseCode,
        CoreAgent::Antigravity => Agent::Antigravity,
        CoreAgent::MinimaxCode => Agent::MinimaxCode,
        CoreAgent::MimoCode => Agent::MimoCode,
        CoreAgent::KimiCode => Agent::KimiCode,
        CoreAgent::Omp => Agent::Omp,
    }
}

pub(crate) fn core_agent(agent: Agent) -> Option<CoreAgent> {
    Some(match agent {
        Agent::Codex => CoreAgent::Codex,
        Agent::ClaudeCode => CoreAgent::ClaudeCode,
        Agent::GrokBuild => CoreAgent::GrokBuild,
        Agent::MuseCode => CoreAgent::MuseCode,
        Agent::Antigravity => CoreAgent::Antigravity,
        Agent::MinimaxCode => CoreAgent::MinimaxCode,
        Agent::MimoCode => CoreAgent::MimoCode,
        Agent::KimiCode => CoreAgent::KimiCode,
        Agent::Omp => CoreAgent::Omp,
        Agent::Amp | Agent::OpenCode => return None,
    })
}

fn op_reference(
    reference: &jackin_core::OpReference,
    account: Option<&jackin_core::OpAccountId>,
) -> OpReference {
    let mut parts = reference.as_str().split('/').skip(2).map(str::to_owned);
    let vault = parts.next().unwrap_or_default();
    let item = parts.next().unwrap_or_default();
    let rest: Vec<String> = parts.collect();
    let (section, field) = match rest.as_slice() {
        [field] => (None, field.clone()),
        [section, field, ..] => (Some(section.clone()), field.clone()),
        [] => (None, String::new()),
    };
    OpReference {
        account: account.map(ToString::to_string).unwrap_or_default(),
        vault_id: vault.clone(),
        vault_name: vault,
        item_id: item.clone(),
        item_title: item,
        section,
        field_id: field.clone(),
        field_label: field,
    }
}

fn env_kind(provider: Provider) -> DetectedKind {
    match provider {
        Provider::Anthropic => DetectedKind::ClaudeApiKeyEnv,
        Provider::Zai => DetectedKind::ZaiApiKeyEnv,
        Provider::Moonshot => DetectedKind::KimiApiKeyEnv,
        Provider::MiniMax => DetectedKind::MinimaxTokenEnv,
        Provider::OpenAi
        | Provider::Amp
        | Provider::XAi
        | Provider::OpenCode
        | Provider::Cursor
        | Provider::Gemini
        | Provider::Openrouter
        | Provider::Antigravity
        | Provider::Hermes
        | Provider::Mimo
        | Provider::Muse => DetectedKind::Unknown,
    }
}

fn file_kind(provider: Provider) -> DetectedKind {
    match provider {
        Provider::Anthropic => DetectedKind::ClaudeOAuthProfile,
        Provider::OpenAi => DetectedKind::CodexAuthJson,
        Provider::XAi => DetectedKind::GrokAuthJson,
        Provider::OpenCode => DetectedKind::OpenCodeGoAuthJson,
        Provider::Amp => DetectedKind::AmpSecrets,
        Provider::Zai
        | Provider::Moonshot
        | Provider::MiniMax
        | Provider::Cursor
        | Provider::Gemini
        | Provider::Openrouter
        | Provider::Antigravity
        | Provider::Hermes
        | Provider::Mimo
        | Provider::Muse => DetectedKind::Unknown,
    }
}

pub(crate) fn source(credential: &CoreCredential, provider: Option<Provider>) -> CredentialSource {
    match credential {
        CoreCredential::Env { var } => CredentialSource::HostEnv {
            var: var.as_str().to_owned(),
            detected: provider.map_or(DetectedKind::Unknown, env_kind),
        },
        CoreCredential::OnePassword { reference, account } => {
            CredentialSource::OnePassword(op_reference(reference, account.as_ref()))
        }
        CoreCredential::File { path } => CredentialSource::LocalFolder {
            path: path.as_path().display().to_string(),
            detected: provider.map_or(DetectedKind::Unknown, file_kind),
        },
        CoreCredential::CodexLogin { home, .. } => CredentialSource::LocalFolder {
            path: home.as_path().display().to_string(),
            detected: DetectedKind::CodexAuthJson,
        },
        CoreCredential::HostLogin { path } => CredentialSource::LocalFolder {
            path: path.as_path().display().to_string(),
            detected: DetectedKind::Unknown,
        },
        CoreCredential::OmpHost => CredentialSource::HostEnv {
            var: String::new(),
            detected: DetectedKind::Unknown,
        },
    }
}

fn account(config: &Config, core: &jackin_core::Account) -> Option<Account> {
    let provider = provider(core.provider())?;
    let credential = source(core.credential(), Some(provider));
    let local_login = core.chatgpt_login().is_some();
    let mut a = Account::registered(
        core.name().as_str(),
        core.name().as_str(),
        provider,
        credential,
    );
    a.lifecycle = jackin_console::domain::account::Lifecycle::Available;
    a.default_for_provider = config.default_account(core.provider()) == Some(core.name());
    a.launch_blocked = (core.provider() == CoreProvider::Codex && !local_login)
        .then_some(LaunchBlock::UsageOnlySource);
    normalize::account(&mut a);
    Some(a)
}

fn registry(config: &Config) -> AccountRegistry {
    AccountRegistry {
        accounts: config
            .accounts()
            .filter_map(|core| account(config, core))
            .collect(),
        revision: 0,
    }
}

fn env_vars(table: &EnvTable) -> Vec<EnvVar> {
    table
        .iter()
        .map(|(name, value)| EnvVar::plain(name.as_str(), value.as_str()))
        .collect()
}

fn role_policy(allowed: &AllowedRoles, default: Option<&RoleName>) -> RolePolicy {
    RolePolicy {
        allowed: match allowed {
            AllowedRoles::All => ConsoleAllowedRoles::All,
            AllowedRoles::Only(set) => {
                ConsoleAllowedRoles::Custom(set.iter().map(ToString::to_string).collect())
            }
        },
        default: default.map(ToString::to_string),
        last: None,
    }
}

fn workspace(
    id: u32,
    name: &WorkspaceName,
    core: &jackin_core::Workspace,
    accounts: &AccountRegistry,
    origin: Option<&GithubOrigin>,
) -> ConsoleWorkspace {
    let mut ws = ConsoleWorkspace::new(id, name.as_str(), normalize::PROJECT_DIR);
    ws.roles = role_policy(&core.roles, core.default_role.as_ref());
    ws.env = env_vars(&core.env);
    ws.role_env = core
        .role_env
        .iter()
        .map(|(role, table)| (role.to_string(), env_vars(table)))
        .collect();
    ws.docker = core.docker;
    ws.github = origin.map(repo_ref);
    ws.accounts.preferred = core
        .bindings
        .iter()
        .filter_map(|(p, account)| provider(*p).map(|p| (p, account.to_string())))
        .collect();
    normalize::workspace(
        &mut ws,
        &core.path.as_path().display().to_string(),
        accounts,
    );
    ws
}

fn role_source(source: &CoreRoleSource) -> RoleSource {
    match source {
        CoreRoleSource::Path(path) => RoleSource::Local {
            path: path.as_path().display().to_string(),
        },
        CoreRoleSource::Git { url, rev } => RoleSource::Git {
            url: url.to_string(),
            branch: rev.as_ref().map(ToString::to_string),
        },
    }
}

fn namespace(source: &CoreRoleSource) -> String {
    let text = match source {
        CoreRoleSource::Path(path) => path
            .as_path()
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        CoreRoleSource::Git { url, .. } => url.to_string(),
    };
    let mut segments = text.rsplit(['/', ':']).filter(|s| !s.is_empty());
    match source {
        CoreRoleSource::Path(_) => segments.next(),
        CoreRoleSource::Git { .. } => segments.nth(1),
    }
    .unwrap_or_default()
    .to_owned()
}

fn role_entries(config: &Config) -> Vec<RoleEntry> {
    config
        .roles()
        .map(|(name, entry)| {
            let mut role = RoleEntry {
                namespace: namespace(&entry.source),
                name: name.to_string(),
                source: role_source(&entry.source),
                trusted: true,
                in_registry: true,
                description: String::new(),
                load_error: None,
                fetch_error: None,
            };
            normalize::role(&mut role);
            role
        })
        .collect()
}

fn trust_rows(config: &Config) -> Vec<TrustRow> {
    let mut by_source: BTreeMap<String, (TrustKind, usize)> = BTreeMap::new();
    for (_, entry) in config.roles() {
        let (label, kind) = match &entry.source {
            CoreRoleSource::Path(path) => (path.as_path().display().to_string(), TrustKind::Path),
            CoreRoleSource::Git { url, .. } => (url.to_string(), TrustKind::Git),
        };
        by_source.entry(label).or_insert((kind, 0)).1 += 1;
    }
    by_source
        .into_iter()
        .map(|(source, (kind, roles))| TrustRow {
            source,
            kind,
            trusted: true,
            roles,
        })
        .collect()
}

fn codex_mode(config: &Config) -> BTreeMap<Agent, AuthMode> {
    let codex: Vec<&jackin_core::Account> = config
        .accounts()
        .filter(|a| a.provider() == CoreProvider::Codex)
        .collect();
    let chosen = config
        .default_account(CoreProvider::Codex)
        .and_then(|name| config.account(name))
        .or(match codex.as_slice() {
            [only] => Some(*only),
            _ => None,
        });
    chosen
        .and_then(jackin_core::Account::codex_auth_mode)
        .map(|mode| match mode {
            jackin_core::CodexAuthMode::ChatgptLogin => AuthMode::Sync,
            jackin_core::CodexAuthMode::ApiKey => AuthMode::ApiKey,
        })
        .map(|mode| BTreeMap::from([(Agent::Codex, mode)]))
        .unwrap_or_default()
}

fn global(config: &Config) -> GlobalConfig {
    let mut g = GlobalConfig {
        env: env_vars(config.env()),
        agent_modes: codex_mode(config),
        trust: trust_rows(config),
        ..GlobalConfig::default()
    };
    normalize::global(&mut g);
    g
}

fn status_of(phase: Phase, supervisor: Option<&SupervisorReading>) -> InstanceStatus {
    match phase {
        Phase::Ready => match supervisor {
            Some(SupervisorReading::Reported(SupervisorState::Running { .. })) => {
                InstanceStatus::Running
            }
            Some(SupervisorReading::Reported(SupervisorState::Exited { code: 0, .. })) => {
                InstanceStatus::CleanExited
            }
            Some(SupervisorReading::Reported(SupervisorState::Exited { .. })) => {
                InstanceStatus::Crashed
            }
            _ => InstanceStatus::RestoreAvailable,
        },
        Phase::Completed { exit_code: 0 } => InstanceStatus::CleanExited,
        Phase::Completed { .. } | Phase::Error { exit_code: Some(_) } => InstanceStatus::Crashed,
        Phase::Error { exit_code: None } => InstanceStatus::FailedSetup,
        Phase::Provisioning
        | Phase::Starting
        | Phase::Stopping
        | Phase::Stopped
        | Phase::Deleting
        | Phase::Suspending
        | Phase::Suspended
        | Phase::Unknown => InstanceStatus::RestoreAvailable,
    }
}

fn preserved(status: InstanceStatus, work: Option<&WorkState>) -> InstanceStatus {
    match (status, work) {
        (
            InstanceStatus::CleanExited | InstanceStatus::RestoreAvailable,
            Some(WorkState::Unexported(left)),
        ) if left.uncommitted > 0 => InstanceStatus::PreservedDirty,
        (
            InstanceStatus::CleanExited | InstanceStatus::RestoreAvailable,
            Some(WorkState::Unexported(left)),
        ) if left.unpushed() > 0 || !left.unusable.is_empty() => InstanceStatus::PreservedUnpushed,
        _ => status,
    }
}

fn session_record(
    summary: &SandboxSummary,
    supervisor: Option<&SupervisorReading>,
) -> Vec<SessionRecord> {
    let status = match supervisor {
        Some(SupervisorReading::Reported(SupervisorState::Running { .. })) => SessionStatus::Active,
        Some(SupervisorReading::Reported(SupervisorState::Exited { code, .. })) => {
            SessionStatus::Exited(*code)
        }
        _ => return Vec::new(),
    };
    vec![SessionRecord {
        id: summary.labels.planned.launch.to_string(),
        agent: Some(agent(
            summary
                .labels
                .planned
                .agents
                .lead_opt()
                .map_or(jackin_core::Agent::Codex, |i| i.agent),
        )),
        label: Agent::Codex.label().to_owned(),
        status,
        started_secs: summary.created_at.map_or(0, |t| t.0),
    }]
}

fn split_name(name: &str) -> (&str, &str) {
    let split = name.len().saturating_sub(NameSuffix::LEN);
    name.split_at_checked(split).unwrap_or((name, name))
}

pub fn sandbox_id(gateway: &GatewayName, name: &SandboxName) -> InstanceId {
    InstanceId::new(gateway.to_string(), name.to_string())
}

fn repo_ref(origin: &GithubOrigin) -> RepoRef {
    RepoRef {
        full_name: origin.repo.full_name(),
        url: origin.repo.to_string(),
    }
}

fn github_repos(origins: &Origins) -> Vec<GithubRepo> {
    let mut seen = BTreeSet::new();
    origins
        .values()
        .filter_map(|origin| {
            let repo = repo_ref(origin);
            let default_branch = origin
                .default_branch
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();
            seen.insert(repo.full_name.clone()).then(|| GithubRepo {
                full_name: repo.full_name,
                branches: origin
                    .default_branch
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                default_branch,
                updated: String::new(),
                url: repo.url,
            })
        })
        .collect()
}

pub(crate) fn instance(
    observed: &ObservedSandbox,
    config: &Config,
    origins: &Origins,
    ids: &IdTable,
    now_secs: i64,
) -> Option<InstanceEntry> {
    let ObservedSandbox {
        gateway,
        summary,
        supervisor,
        work,
        task,
    } = observed;
    let (head, _) = split_name(summary.name.as_str());
    let labels = &summary.labels.planned;
    let origin = origins.get(&labels.workspace);
    let status = preserved(status_of(summary.phase, supervisor.as_ref()), work.as_ref());
    let (uncommitted, unpushed, branch) = match work {
        Some(WorkState::Unexported(left)) => (
            left.uncommitted,
            left.unpushed(),
            left.branches.first().map(ToString::to_string),
        ),
        Some(WorkState::Exported | WorkState::Unknown(_)) | None => (0, 0, None),
    };
    let mut record = Instance {
        id: sandbox_id(gateway, &summary.name),
        container: head.trim_end_matches('-').to_owned(),
        workspace: config
            .workspace(&labels.workspace)
            .and_then(|_| ids.lookup(&labels.workspace)),
        workdir: None,
        role: Some(labels.role.to_string()),
        agent: agent(
            labels
                .agents
                .lead_opt()
                .map_or(jackin_core::Agent::Codex, |i| i.agent),
        ),
        status,
        created_secs: summary.created_at.map_or(0, |t| t.0),
        last_seen_secs: now_secs,
        run_id: labels.launch.to_string(),
        sessions: Ok(session_record(summary, supervisor.as_ref())),
        daemon: DaemonSnapshot::NoTabs,
        branch,
        pr: None,
        default_branch: origin
            .and_then(|o| o.default_branch.as_ref())
            .map(ToString::to_string)
            .unwrap_or_default(),
        github: origin.map(repo_ref),
        uncommitted,
        unpushed,
        account: labels.agents.lead_opt().map(|i| i.account.to_string()),
        accounts: labels
            .agents
            .iter()
            .map(|i| i.account.to_string())
            .collect(),
        agents: labels
            .agents
            .iter()
            .filter_map(|item| {
                provider(item.provider).map(|found| {
                    jackin_console::domain::instance::AgentInstanceView {
                        agent: agent(item.agent),
                        provider: found,
                        account: Some(item.account.to_string()),
                        model: item.model.clone(),
                        branch: Some(item.branch.clone()),
                        state: match status {
                            InstanceStatus::Running => {
                                jackin_console::domain::instance::AgentState::Working
                            }
                            InstanceStatus::CleanExited => {
                                jackin_console::domain::instance::AgentState::Done
                            }
                            InstanceStatus::Crashed => {
                                jackin_console::domain::instance::AgentState::Blocked
                            }
                            _ => jackin_console::domain::instance::AgentState::Unknown,
                        },
                    }
                })
            })
            .collect(),
        task: task
            .as_ref()
            .map(|found| jackin_console::domain::instance::TaskView {
                text: found.text.clone(),
                statuses: found
                    .statuses
                    .iter()
                    .map(
                        |(core, text)| jackin_console::domain::instance::InstanceStatusView {
                            agent: agent(*core),
                            text: text.clone(),
                        },
                    )
                    .collect(),
            }),
    };
    normalize::instance(&mut record);
    Some(InstanceEntry::available(record))
}

pub(crate) fn sandbox_phase(phase: Phase) -> SandboxPhase {
    match phase {
        Phase::Provisioning => SandboxPhase::Provisioning,
        Phase::Starting => SandboxPhase::Starting,
        Phase::Ready => SandboxPhase::Ready,
        Phase::Stopping => SandboxPhase::Stopping,
        Phase::Stopped => SandboxPhase::Stopped,
        Phase::Completed { exit_code } => SandboxPhase::Completed { exit_code },
        Phase::Error { exit_code } => SandboxPhase::Error { exit_code },
        Phase::Deleting => SandboxPhase::Deleting,
        Phase::Suspending => SandboxPhase::Suspending,
        Phase::Suspended => SandboxPhase::Suspended,
        Phase::Unknown => SandboxPhase::Unknown,
    }
}

fn unreadable(
    observed: &crate::fleet::ObservedUnreadable,
    config: &Config,
    ids: &IdTable,
) -> InstanceEntry {
    let sandbox = &observed.sandbox;
    let workspace = sandbox
        .workspace
        .as_ref()
        .and_then(|name| config.workspace(name).and_then(|_| ids.lookup(name)));
    InstanceEntry::Unavailable(UnavailableInstance {
        id: sandbox_id(&observed.gateway, &sandbox.name),
        workspace,
        workspace_name: sandbox.workspace.as_ref().map(ToString::to_string),
        role: sandbox.role.as_ref().map(ToString::to_string),
        phase: sandbox_phase(sandbox.phase),
        created_secs: sandbox.created_at.map(|created| created.0),
        reason: sandbox.error.to_string(),
    })
}

pub fn manifest_doc(doc: &jackin_core::RoleManifest) -> RoleManifestDoc {
    RoleManifestDoc {
        name: doc.role.name.to_string(),
        dockerfile: doc.role.dockerfile.to_string(),
        policy: doc.role.policy.to_string(),
        codex_model: doc
            .agents
            .get(&jackin_core::Agent::Codex)
            .and_then(|s| s.model.as_ref())
            .map(ToString::to_string),
        docker: doc.docker.enabled,
        hook_setup: doc.hooks.setup.as_ref().map(ToString::to_string),
        hook_preflight: doc.hooks.preflight.as_ref().map(ToString::to_string),
        env: doc
            .env
            .iter()
            .map(|(name, spec)| RoleEnvEntry {
                name: name.to_string(),
                default: spec.default.clone(),
                prompt: spec.prompt.clone(),
            })
            .collect(),
    }
}

fn role_admin(config: &Config, docs: &RoleDocs, checks: &RoleChecks) -> RoleAdmin {
    RoleAdmin {
        agents: CoreAgent::ALL.into_iter().map(agent).collect(),
        order: AccountOrder::Jackin,
        roles: config
            .roles()
            .map(|(name, entry)| RoleRecord {
                name: name.to_string(),
                source: role_source(&entry.source),
                manifest: match docs.get(name) {
                    Some(Ok(doc)) => Ok(manifest_doc(doc)),
                    Some(Err(error)) => Err(error.to_string()),
                    None => Err(RoleDocError::Rejected(Rejection::RoleNotLoaded).to_string()),
                },
                check: checks.get(name).map_or(RoleCheckState::Unchecked, |found| {
                    RoleCheckState::Done(found.clone())
                }),
                bindings: entry
                    .bindings
                    .iter()
                    .map(|(bound_agent, account)| (agent(*bound_agent), account.to_string()))
                    .collect(),
                files: Vec::new(),
            })
            .collect(),
    }
}

pub(crate) fn docker_risks() -> Vec<DockerRiskText> {
    DockerRisk::ALL
        .into_iter()
        .map(|risk| DockerRiskText {
            label: risk.label().to_owned(),
            text: risk.text().to_owned(),
        })
        .collect()
}

fn docker_setting(
    config: &Config,
    docs: &RoleDocs,
    gateway: Option<&GatewayName>,
) -> DockerSetting {
    let entry = gateway.and_then(|name| config.gateway(name));
    DockerSetting {
        gateway: gateway.map(ToString::to_string).unwrap_or_default(),
        engine: match entry.and_then(|g| g.docker_engine.as_ref()) {
            Some(engine) => EngineState::Configured {
                endpoint: EngineEndpoint::TlsPort(engine.port),
            },
            None => EngineState::Missing,
        },
        risks: docker_risks(),
        role_enabled: docs
            .iter()
            .filter(|(_, doc)| doc.as_ref().is_ok_and(|d| d.docker.enabled))
            .map(|(name, _)| name.to_string())
            .collect::<BTreeSet<_>>(),
    }
}

fn gateway_name(config: &Config) -> Option<GatewayName> {
    config
        .resolve_gateway(None)
        .ok()
        .map(|(name, _)| name.clone())
}

fn surfaces(input: &ProjectInput<'_>) -> Surfaces {
    let ProjectInput {
        config,
        fleet: _,
        facts,
        roles,
        checks,
        origins: _,
    } = input;
    let gateway = gateway_name(config);
    let mut built = Surfaces::openshell(LiveFacts {
        version: facts.version.clone(),
        config_path: facts.config_path.clone(),
        gateway: gateway.as_ref().map(ToString::to_string),
    })
    .roles(role_admin(config, roles, checks))
    .docker(docker_setting(config, roles, gateway.as_ref()));
    built.tuples = tuple_offers(roles);
    built
}

fn tuple_offers(roles: &RoleDocs) -> Vec<jackin_console::domain::tuple::TupleOffer> {
    let mut offers: Vec<jackin_console::domain::tuple::TupleOffer> = Vec::new();
    for doc in roles.values() {
        let Ok(manifest) = doc else { continue };
        for tuple in &jackin_core::TUPLES {
            if !manifest.agents.contains_key(&tuple.agent) {
                continue;
            }
            let Some(found_provider) = provider(tuple.provider) else {
                continue;
            };
            let offer = jackin_console::domain::tuple::TupleOffer {
                agent: agent(tuple.agent),
                provider: found_provider,
                state: match tuple.state {
                    jackin_core::TupleState::Supported => {
                        jackin_console::domain::tuple::TupleState::Supported
                    }
                    jackin_core::TupleState::Unavailable => {
                        jackin_console::domain::tuple::TupleState::Unavailable
                    }
                    jackin_core::TupleState::BlockedVendor => {
                        jackin_console::domain::tuple::TupleState::BlockedVendor
                    }
                },
                reason: match tuple.state {
                    jackin_core::TupleState::Supported => None,
                    _ => tuple.reason.map(str::to_owned),
                },
                allowed_models: tuple
                    .allowed_models
                    .iter()
                    .map(|model| (*model).to_owned())
                    .collect(),
                default_model: tuple.default_model.to_owned(),
            };
            if !offers.contains(&offer) {
                offers.push(offer);
            }
        }
    }
    offers
}

pub fn project(input: &ProjectInput<'_>, ids: &mut IdTable) -> Snapshot {
    let ProjectInput {
        config,
        fleet,
        facts,
        roles: _,
        checks: _,
        origins,
    } = *input;
    let accounts = registry(config);
    let workspaces = config
        .workspaces()
        .map(|(name, core)| workspace(ids.id(name), name, core, &accounts, origins.get(name)))
        .collect();
    let instances = fleet
        .sandboxes
        .iter()
        .filter_map(|observed| instance(observed, config, origins, ids, facts.now_secs))
        .chain(
            fleet
                .unreadable
                .iter()
                .map(|observed| unreadable(observed, config, ids)),
        )
        .collect();
    Snapshot {
        workspaces,
        roles: role_entries(config),
        instances,
        global: global(config),
        accounts,
        surfaces: surfaces(input),
        github: github_repos(origins),
        index_error: fleet
            .all_failed()
            .then_some(DiscoveryError::IndexUnreadable),
        daemon_health: if fleet.failed().next().is_some() {
            DaemonHealth::Stale
        } else {
            DaemonHealth::Healthy
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_maps_all_core_agents() {
        let expected = [
            (CoreAgent::Codex, Agent::Codex),
            (CoreAgent::ClaudeCode, Agent::ClaudeCode),
            (CoreAgent::GrokBuild, Agent::GrokBuild),
            (CoreAgent::MuseCode, Agent::MuseCode),
            (CoreAgent::Antigravity, Agent::Antigravity),
            (CoreAgent::MinimaxCode, Agent::MinimaxCode),
            (CoreAgent::MimoCode, Agent::MimoCode),
            (CoreAgent::KimiCode, Agent::KimiCode),
            (CoreAgent::Omp, Agent::Omp),
        ];
        assert_eq!(CoreAgent::ALL.len(), expected.len());
        for (core, console) in expected {
            assert_eq!(agent(core), console);
            assert_eq!(core_agent(console), Some(core));
        }
    }

    #[test]
    fn provider_maps_all_core_providers() {
        assert_eq!(CoreProvider::ALL.len(), 15);
        for core in CoreProvider::ALL {
            let console = provider(core).expect("provider");
            assert_eq!(core_provider(console), core);
        }
    }
}
