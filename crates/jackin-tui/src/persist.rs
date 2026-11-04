// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use jackin_console::domain::account::{
    Account, AccountRegistry, CredentialSource as ConsoleSource,
};
use jackin_console::domain::agent::Provider;
use jackin_console::domain::workspace::{
    AllowedRoles as ConsoleAllowed, EnvValue, EnvVar, Workspace,
};
use jackin_console::world::GlobalConfig;
use jackin_core::plain;
use jackin_core::{
    Account as CoreAccount, AccountName, AllowedRoles, ChatgptAccountId, Config, ConfigEdit,
    ConfigError, CredentialSource, EnvName, EnvScope, EnvValueError, FsError, HostPath, IdError,
    OpReference, PathError, PlainEnvValue, Provider as CoreProvider, ReservedEnvName, RoleName,
    RoleSet, Workspace as CoreWorkspace, WorkspaceName,
};
use thiserror::Error;

use crate::project::core_provider;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum Field {
    #[error("workdir")]
    Workdir,
    #[error("mounts")]
    Mounts,
    #[error("keep_awake")]
    KeepAwake,
    #[error("git_pull")]
    GitPull,
    #[error("dirty_policy")]
    DirtyPolicy,
    #[error("disabled_defaults")]
    DisabledDefaults,
    #[error("enabled accounts")]
    EnabledAccounts,
    #[error("role accounts")]
    RoleAccounts,
    #[error("coauthor_trailer")]
    CoauthorTrailer,
    #[error("dco_signoff")]
    DcoSignoff,
    #[error("global mounts")]
    GlobalMounts,
    #[error("global role variables")]
    GlobalRoleVariables,
    #[error("agent modes")]
    AgentModes,
    #[error("trust")]
    Trust,
    #[error("account name")]
    AccountName,
    #[error("account provider")]
    AccountProvider,
    #[error("purpose")]
    Purpose,
    #[error("endpoint")]
    Endpoint,
    #[error("env source")]
    EnvSource,
    #[error("api key")]
    ApiKey,
    #[error("agent")]
    Agent,
    #[error("role source")]
    RoleSource,
    #[error("role name")]
    RoleName,
}

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("{field} cannot be saved")]
    Unrepresentable { field: Field },
    #[error("a workspace needs a project folder")]
    NoProject,
    #[error("{} is not a folder jackin can use", plain(.folder))]
    Folder { folder: String },
    #[error("no allowed role is left")]
    NoRoles,
    #[error(transparent)]
    Reserved(#[from] ReservedEnvName),
    #[error(transparent)]
    Name(#[from] IdError),
    #[error(transparent)]
    Value(#[from] EnvValueError),
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Config(#[from] ConfigError),
}

#[derive(Clone, Debug)]
pub struct Folder {
    pub path: HostPath,
    pub login: Option<ChatgptAccountId>,
}

#[derive(Clone, Debug, Default)]
pub struct Resolved {
    pub project: Option<HostPath>,
    pub folders: BTreeMap<String, Folder>,
}

fn same_set(a: &[String], b: &[String]) -> bool {
    let a: BTreeSet<&String> = a.iter().collect();
    let b: BTreeSet<&String> = b.iter().collect();
    a == b
}

fn keep<T: PartialEq>(payload: &T, base: &T, field: Field) -> Result<(), PersistError> {
    if payload == base {
        Ok(())
    } else {
        Err(PersistError::Unrepresentable { field })
    }
}

fn plain_value(value: &EnvValue) -> Result<PlainEnvValue, PersistError> {
    match value {
        EnvValue::Plain(text) => Ok(PlainEnvValue::parse(text)?),
        EnvValue::OnePassword(_) | EnvValue::HostEnv(_) => Err(PersistError::Unrepresentable {
            field: Field::EnvSource,
        }),
    }
}

fn env_map(vars: &[EnvVar]) -> BTreeMap<&str, &EnvValue> {
    vars.iter().map(|v| (v.key.as_str(), &v.value)).collect()
}

fn env_edits(
    scope: &EnvScope,
    base: &[EnvVar],
    payload: &[EnvVar],
) -> Result<Vec<ConfigEdit>, PersistError> {
    let before = env_map(base);
    let after = env_map(payload);
    let mut edits = Vec::new();
    for (key, value) in &after {
        if before.get(key) != Some(value) {
            edits.push(ConfigEdit::SetEnv {
                scope: scope.clone(),
                name: EnvName::parse(key)?,
                value: Some(plain_value(value)?),
            });
        }
    }
    for key in before.keys().filter(|key| !after.contains_key(*key)) {
        edits.push(ConfigEdit::SetEnv {
            scope: scope.clone(),
            name: EnvName::parse(key)?,
            value: None,
        });
    }
    Ok(edits)
}

fn core_roles(allowed: &ConsoleAllowed) -> Result<AllowedRoles, PersistError> {
    match allowed {
        ConsoleAllowed::All => Ok(AllowedRoles::All),
        ConsoleAllowed::Custom(list) => {
            let names = list
                .iter()
                .map(|role| RoleName::parse(role))
                .collect::<Result<Vec<_>, _>>()?;
            RoleSet::new(names)
                .map(AllowedRoles::Only)
                .ok_or(PersistError::NoRoles)
        }
    }
}

fn account_name(id: &str) -> Result<AccountName, PersistError> {
    Ok(AccountName::parse(id)?)
}

fn bindings(
    workspace: &WorkspaceName,
    base: &BTreeMap<Provider, String>,
    payload: &BTreeMap<Provider, String>,
) -> Result<Vec<ConfigEdit>, PersistError> {
    let mut edits = Vec::new();
    for (provider, account) in payload {
        if base.get(provider) != Some(account) {
            edits.push(ConfigEdit::BindWorkspaceAccount {
                workspace: workspace.clone(),
                account: account_name(account)?,
            });
        }
    }
    for provider in base.keys().filter(|p| !payload.contains_key(*p)) {
        edits.push(ConfigEdit::UnbindWorkspaceAccount {
            workspace: workspace.clone(),
            provider: core_provider(*provider),
        });
    }
    Ok(edits)
}

fn created(
    name: &str,
    payload: &Workspace,
    resolved: &Resolved,
) -> Result<Vec<ConfigEdit>, PersistError> {
    let Workspace {
        id: _,
        name: _,
        workdir: _,
        mounts: _,
        roles,
        env,
        role_env,
        keep_awake: _,
        git_pull: _,
        docker,
        accounts,
        dirty_policy: _,
    } = payload;
    let path = resolved.project.clone().ok_or(PersistError::NoProject)?;
    let mut workspace = CoreWorkspace::new(path, *docker);
    workspace.roles = core_roles(&roles.allowed)?;
    workspace.default_role = roles.default.as_deref().map(RoleName::parse).transpose()?;
    for (provider, account) in &accounts.preferred {
        workspace
            .bindings
            .insert(core_provider(*provider), account_name(account)?);
    }
    for var in env {
        workspace
            .env
            .insert(scoped(&var.key)?, plain_value(&var.value)?);
    }
    for (role, vars) in role_env {
        let table = vars
            .iter()
            .map(|var| Ok((scoped(&var.key)?, plain_value(&var.value)?)))
            .collect::<Result<_, PersistError>>()?;
        workspace.role_env.insert(RoleName::parse(role)?, table);
    }
    Ok(vec![ConfigEdit::AddWorkspace {
        name: WorkspaceName::parse(name)?,
        workspace,
    }])
}

fn scoped(key: &str) -> Result<jackin_core::ScopedEnvName, PersistError> {
    Ok(jackin_core::ScopedEnvName::new(EnvName::parse(key)?)?)
}

fn fixed(base: &Workspace, payload: &Workspace) -> Result<(), PersistError> {
    keep(&payload.workdir, &base.workdir, Field::Workdir)?;
    keep(&payload.mounts, &base.mounts, Field::Mounts)?;
    keep(&payload.keep_awake, &base.keep_awake, Field::KeepAwake)?;
    keep(&payload.git_pull, &base.git_pull, Field::GitPull)?;
    keep(
        &payload.dirty_policy,
        &base.dirty_policy,
        Field::DirtyPolicy,
    )?;
    keep(
        &payload.accounts.disabled_defaults,
        &base.accounts.disabled_defaults,
        Field::DisabledDefaults,
    )?;
    keep(
        &payload.accounts.enabled,
        &base.accounts.enabled,
        Field::EnabledAccounts,
    )?;
    keep(
        &payload.accounts.role_preferred,
        &base.accounts.role_preferred,
        Field::RoleAccounts,
    )
}

fn role_edits(
    config: &Config,
    target: &WorkspaceName,
    base: &Workspace,
    payload: &Workspace,
) -> Result<Vec<ConfigEdit>, PersistError> {
    let roles_changed = match (&base.roles.allowed, &payload.roles.allowed) {
        (ConsoleAllowed::All, ConsoleAllowed::All) => false,
        (ConsoleAllowed::Custom(a), ConsoleAllowed::Custom(b)) => !same_set(a, b),
        _ => true,
    };
    let default_changed = base.roles.default != payload.roles.default;
    let default = |role: Option<RoleName>| ConfigEdit::SetWorkspaceDefaultRole {
        workspace: target.clone(),
        role,
    };
    let new_default = payload
        .roles
        .default
        .as_deref()
        .map(RoleName::parse)
        .transpose()?;
    let mut edits = Vec::new();
    if roles_changed && default_changed {
        if config
            .workspace(target)
            .is_some_and(|saved| saved.default_role.is_some())
        {
            edits.push(default(None));
        }
        edits.push(ConfigEdit::SetWorkspaceRoles {
            workspace: target.clone(),
            roles: core_roles(&payload.roles.allowed)?,
        });
        if new_default.is_some() {
            edits.push(default(new_default));
        }
    } else if roles_changed {
        edits.push(ConfigEdit::SetWorkspaceRoles {
            workspace: target.clone(),
            roles: core_roles(&payload.roles.allowed)?,
        });
    } else if default_changed {
        edits.push(default(new_default));
    }
    Ok(edits)
}

fn updated(
    config: &Config,
    base: &Workspace,
    payload: &Workspace,
) -> Result<Vec<ConfigEdit>, PersistError> {
    fixed(base, payload)?;
    let old = WorkspaceName::parse(&base.name)?;
    let target = WorkspaceName::parse(&payload.name)?;
    let mut edits = Vec::new();
    let renamed = old != target;
    if renamed {
        edits.push(ConfigEdit::RemoveWorkspace(old.clone()));
        if let Some(saved) = config.workspace(&old) {
            edits.push(ConfigEdit::AddWorkspace {
                name: target.clone(),
                workspace: saved.clone(),
            });
        }
    }
    if payload.docker != base.docker {
        edits.push(ConfigEdit::SetWorkspaceDocker {
            workspace: target.clone(),
            docker: payload.docker,
        });
    }
    edits.extend(role_edits(config, &target, base, payload)?);
    edits.extend(env_edits(
        &EnvScope::Workspace(target.clone()),
        &base.env,
        &payload.env,
    )?);
    let roles: BTreeSet<&String> = base
        .role_env
        .keys()
        .chain(payload.role_env.keys())
        .collect();
    for role in roles {
        let before = base.role_env.get(role).map_or(&[][..], Vec::as_slice);
        let after = payload.role_env.get(role).map_or(&[][..], Vec::as_slice);
        edits.extend(env_edits(
            &EnvScope::WorkspaceRole(target.clone(), RoleName::parse(role)?),
            before,
            after,
        )?);
    }
    edits.extend(bindings(
        &target,
        &base.accounts.preferred,
        &payload.accounts.preferred,
    )?);
    Ok(edits)
}

pub fn workspace(
    config: &Config,
    base: Option<&Workspace>,
    payload: &Workspace,
    resolved: &Resolved,
) -> Result<Vec<ConfigEdit>, PersistError> {
    match base {
        None => created(&payload.name, payload, resolved),
        Some(base) => updated(config, base, payload),
    }
}

pub fn global(
    base: &GlobalConfig,
    payload: &GlobalConfig,
) -> Result<Vec<ConfigEdit>, PersistError> {
    let GlobalConfig {
        coauthor_trailer,
        dco_signoff,
        mounts,
        env,
        role_env,
        agent_modes,
        trust,
    } = payload;
    keep(
        coauthor_trailer,
        &base.coauthor_trailer,
        Field::CoauthorTrailer,
    )?;
    keep(dco_signoff, &base.dco_signoff, Field::DcoSignoff)?;
    keep(mounts, &base.mounts, Field::GlobalMounts)?;
    keep(role_env, &base.role_env, Field::GlobalRoleVariables)?;
    keep(agent_modes, &base.agent_modes, Field::AgentModes)?;
    keep(trust, &base.trust, Field::Trust)?;
    env_edits(&EnvScope::Global, &base.env, env)
}

fn credential(
    provider: Provider,
    source: &ConsoleSource,
    resolved: &Resolved,
) -> Result<CredentialSource, PersistError> {
    match source {
        ConsoleSource::OnePassword(reference) => Ok(CredentialSource::OnePassword {
            reference: OpReference::parse(&reference.canonical())?,
        }),
        ConsoleSource::HostEnv { var, .. } => Ok(CredentialSource::Env {
            var: EnvName::parse(var)?,
        }),
        ConsoleSource::LocalFolder { path, .. } => {
            let folder = resolved
                .folders
                .get(path)
                .ok_or_else(|| PersistError::Folder {
                    folder: path.clone(),
                })?;
            if core_provider(provider) == CoreProvider::Codex {
                let chatgpt_account_id =
                    folder.login.clone().ok_or_else(|| PersistError::Folder {
                        folder: path.clone(),
                    })?;
                Ok(CredentialSource::CodexLogin {
                    home: folder.path.clone(),
                    chatgpt_account_id,
                })
            } else {
                Ok(CredentialSource::File {
                    path: folder.path.clone(),
                })
            }
        }
        ConsoleSource::PlainApiKey { .. } => Err(PersistError::Unrepresentable {
            field: Field::ApiKey,
        }),
    }
}

pub fn folders(registry: &AccountRegistry, base: &AccountRegistry) -> Vec<(Provider, String)> {
    registry
        .accounts
        .iter()
        .filter(|a| base.get(&a.id).is_none_or(|b| b.source != a.source))
        .filter_map(|a| match &a.source {
            ConsoleSource::LocalFolder { path, .. } => Some((a.provider, path.clone())),
            _ => None,
        })
        .collect()
}

pub fn accounts(
    base: &AccountRegistry,
    payload: &AccountRegistry,
    resolved: &Resolved,
) -> Result<Vec<ConfigEdit>, PersistError> {
    let AccountRegistry {
        accounts: after,
        revision: _,
    } = payload;
    let mut removed = Vec::new();
    let mut added = Vec::new();
    let mut changed = Vec::new();
    for gone in base
        .accounts
        .iter()
        .filter(|b| !after.iter().any(|a| a.id == b.id))
    {
        removed.push(ConfigEdit::RemoveAccountClearingBindings(account_name(
            &gone.id,
        )?));
    }
    for account in after {
        let Account {
            id,
            origin: _,
            display_name,
            agent: _,
            provider,
            surface: _,
            source,
            identity: _,
            provenance: _,
            confidence: _,
            lifecycle: _,
            purpose,
            enabled: _,
            default_for_provider: _,
            validation: _,
            last_refresh_secs: _,
            issue: _,
            endpoint,
            usage: _,
            launch_blocked: _,
            edit_blocked: _,
        } = account;
        match base.get(id) {
            None => {
                if purpose.is_some() {
                    return Err(PersistError::Unrepresentable {
                        field: Field::Purpose,
                    });
                }
                if endpoint.is_some() {
                    return Err(PersistError::Unrepresentable {
                        field: Field::Endpoint,
                    });
                }
                added.push(ConfigEdit::AddAccount(CoreAccount::new(
                    account_name(display_name)?,
                    core_provider(*provider),
                    credential(*provider, source, resolved)?,
                )?));
            }
            Some(before) => {
                keep(display_name, &before.display_name, Field::AccountName)?;
                keep(provider, &before.provider, Field::AccountProvider)?;
                keep(purpose, &before.purpose, Field::Purpose)?;
                keep(endpoint, &before.endpoint, Field::Endpoint)?;
                if source != &before.source {
                    changed.push(ConfigEdit::SetAccountCredential {
                        account: account_name(id)?,
                        credential: credential(*provider, source, resolved)?,
                    });
                }
            }
        }
    }
    let mut edits = removed;
    edits.extend(added);
    edits.extend(changed);
    edits.extend(default_edits(base, payload)?);
    Ok(edits)
}

fn default_for(registry: &AccountRegistry, provider: Provider) -> Option<&Account> {
    registry
        .accounts
        .iter()
        .find(|a| a.provider == provider && a.default_for_provider)
}

fn default_edits(
    base: &AccountRegistry,
    payload: &AccountRegistry,
) -> Result<Vec<ConfigEdit>, PersistError> {
    let providers: BTreeSet<Provider> = base
        .accounts
        .iter()
        .chain(&payload.accounts)
        .map(|a| a.provider)
        .collect();
    let mut edits = Vec::new();
    for provider in providers {
        let before = default_for(base, provider);
        let now = default_for(payload, provider);
        if before.map(|a| &a.id) == now.map(|a| &a.id) {
            continue;
        }
        if let Some(account) = now {
            let known = base.get(&account.id).is_some();
            edits.push(ConfigEdit::SetDefaultAccount {
                account: account_name(if known {
                    &account.id
                } else {
                    &account.display_name
                })?,
            });
        } else if before.is_some_and(|a| payload.get(&a.id).is_some()) {
            edits.push(ConfigEdit::ClearDefaultAccount {
                provider: core_provider(provider),
            });
        }
    }
    Ok(edits)
}
