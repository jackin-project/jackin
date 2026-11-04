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
    DaemonSnapshot, Instance, InstanceId, InstanceStatus, SessionRecord, SessionStatus, instance_id,
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

pub type RoleDocs = BTreeMap<RoleName, Result<RoleManifestDoc, RoleDocError>>;

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

pub fn provider(provider: CoreProvider) -> Provider {
    match provider {
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
        CoreProvider::Muse => Provider::Muse,
        CoreProvider::Opencode => Provider::OpenCode,
        CoreProvider::Openrouter => Provider::Openrouter,
        CoreProvider::Zai => Provider::Zai,
    }
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
        Provider::OpenCode => CoreProvider::Opencode,
        Provider::Openrouter => CoreProvider::Openrouter,
        Provider::Zai => CoreProvider::Zai,
    }
}

fn agent(agent: CoreAgent) -> Agent {
    match agent {
        CoreAgent::Codex => Agent::Codex,
    }
}

fn op_reference(reference: &jackin_core::OpReference) -> OpReference {
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
        account: String::new(),
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
        | Provider::Muse => DetectedKind::Unknown,
    }
}

pub(crate) fn source(credential: &CoreCredential, provider: Provider) -> CredentialSource {
    match credential {
        CoreCredential::Env { var } => CredentialSource::HostEnv {
            var: var.as_str().to_owned(),
            detected: env_kind(provider),
        },
        CoreCredential::OnePassword { reference } => {
            CredentialSource::OnePassword(op_reference(reference))
        }
        CoreCredential::File { path } => CredentialSource::LocalFolder {
            path: path.as_path().display().to_string(),
            detected: file_kind(provider),
        },
        CoreCredential::CodexLogin { home, .. } => CredentialSource::LocalFolder {
            path: home.as_path().display().to_string(),
            detected: DetectedKind::CodexAuthJson,
        },
    }
}

fn account(config: &Config, core: &jackin_core::Account) -> Account {
    let provider = provider(core.provider());
    let credential = source(core.credential(), provider);
    let local_login = core.chatgpt_login().is_some();
    let edit_blocked = match &credential {
        CredentialSource::HostEnv { var, .. } => Some(normalize::env_edit_block(var)),
        CredentialSource::OnePassword(_)
        | CredentialSource::LocalFolder { .. }
        | CredentialSource::PlainApiKey { .. } => None,
    };
    let mut a = Account::registered(
        core.name().as_str(),
        core.name().as_str(),
        provider,
        credential,
    );
    a.lifecycle = jackin_console::domain::account::Lifecycle::Available;
    a.default_for_provider = config.default_account(core.provider()) == Some(core.name());
    a.edit_blocked = edit_blocked;
    a.launch_blocked = (core.provider() == CoreProvider::Codex && !local_login)
        .then_some(LaunchBlock::UsageOnlySource);
    normalize::account(&mut a);
    a
}

fn registry(config: &Config) -> AccountRegistry {
    AccountRegistry {
        accounts: config
            .accounts()
            .map(|core| account(config, core))
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
        .map(|(p, account)| (provider(*p), account.to_string()))
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
        ) if left.unpushed() > 0 => InstanceStatus::PreservedUnpushed,
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
        agent: Some(agent(summary.labels.planned.agent)),
        label: Agent::Codex.label().to_owned(),
        status,
        started_secs: summary.created_at.map_or(0, |t| t.0),
    }]
}

fn split_name(name: &str) -> (&str, &str) {
    let split = name.len().saturating_sub(NameSuffix::LEN);
    name.split_at_checked(split).unwrap_or((name, name))
}

pub fn sandbox_id(name: &SandboxName) -> InstanceId {
    instance_id(split_name(name.as_str()).1)
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

fn instance(
    observed: &ObservedSandbox,
    config: &Config,
    origins: &Origins,
    ids: &IdTable,
    now_secs: i64,
) -> Instance {
    let ObservedSandbox {
        gateway: _,
        summary,
        supervisor,
        work,
    } = observed;
    let (head, suffix) = split_name(summary.name.as_str());
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
        id: instance_id(suffix),
        container: head.trim_end_matches('-').to_owned(),
        workspace: config
            .workspace(&labels.workspace)
            .and_then(|_| ids.lookup(&labels.workspace)),
        workdir: String::new(),
        role: labels.role.to_string(),
        agent: agent(labels.agent),
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
        account: Some(labels.account.to_string()),
        accounts: vec![labels.account.to_string()],
    };
    normalize::instance(&mut record);
    record
}

pub fn manifest_doc(doc: &jackin_core::RoleManifest) -> RoleManifestDoc {
    RoleManifestDoc {
        name: doc.role.name.to_string(),
        dockerfile: doc.role.dockerfile.to_string(),
        policy: doc.role.policy.to_string(),
        codex_model: doc.codex.model.as_ref().map(ToString::to_string),
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
        agents: vec![Agent::Codex],
        order: AccountOrder::Jackin,
        roles: config
            .roles()
            .map(|(name, entry)| RoleRecord {
                name: name.to_string(),
                source: role_source(&entry.source),
                manifest: match docs.get(name) {
                    Some(Ok(doc)) => Ok(doc.clone()),
                    Some(Err(error)) => Err(error.to_string()),
                    None => Err(RoleDocError::Rejected(Rejection::RoleNotLoaded).to_string()),
                },
                check: checks.get(name).map_or(RoleCheckState::Unchecked, |found| {
                    RoleCheckState::Done(found.clone())
                }),
                bindings: entry
                    .bindings
                    .iter()
                    .map(|(a, account)| (agent(*a), account.to_string()))
                    .collect(),
                files: Vec::new(),
            })
            .collect(),
    }
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
        risks: DockerRisk::ALL
            .into_iter()
            .map(|risk| DockerRiskText {
                label: risk.label().to_owned(),
                text: risk.text().to_owned(),
            })
            .collect(),
        role_enabled: docs
            .iter()
            .filter(|(_, doc)| doc.as_ref().is_ok_and(|d| d.docker))
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
    Surfaces::openshell(LiveFacts {
        version: facts.version.clone(),
        config_path: facts.config_path.clone(),
        gateway: gateway.as_ref().map(ToString::to_string),
    })
    .roles(role_admin(config, roles, checks))
    .docker(docker_setting(config, roles, gateway.as_ref()))
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
        .map(|observed| instance(observed, config, origins, ids, facts.now_secs))
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
