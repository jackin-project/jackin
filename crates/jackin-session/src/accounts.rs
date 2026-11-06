// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_core::{
    Account, AccountName, Agent, CodexAuthMode, Config, ConfigEdit, ConfigError, ConfigStore,
    FileLock, GatewayEntry, GatewayName, HostPath, Provider, ProviderName,
};
use jackin_openshell::{
    Gateway, GatewayError, PolicyError, ProfileSync, ProviderMaterial, ProviderProfile,
    ProviderSpec, ProviderState,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::connect::{ConnectError, GatewayConnector};
use crate::credentials::{CredentialError, read_chatgpt_login, resolve_static_key};
use crate::gateways::{Gateways, UnreachableGateway};
use crate::lock::{self, LockError};
use crate::report::error_chain;
use crate::service::{HostPorts, Session};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountSync {
    HostOnly,
    ProviderReady {
        provider: ProviderName,
        profile: ProfileSync,
    },
}

#[derive(Debug, Error)]
pub enum AccountError {
    #[error(transparent)]
    Credential(#[from] CredentialError),
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(
        "the provider of account {account} stays on the gateway, because removing it failed: {rollback}"
    )]
    RollbackFailed {
        account: AccountName,
        #[source]
        original: Box<AccountError>,
        rollback: Box<AccountError>,
    },
    #[error("account {account} was removed after the launch was planned")]
    Removed { account: AccountName },
    #[error("account {account} changed after the launch was planned")]
    Changed { account: AccountName },
    #[error("gateway {gateway} is not in the config any more")]
    GatewayRemoved { gateway: GatewayName },
    #[error("gateway {gateway} was added to the config after the gateways were connected")]
    GatewayAdded { gateway: GatewayName },
    #[error("account {account} uses {mode}, which is not supported yet")]
    Unsupported {
        account: AccountName,
        mode: CodexAuthMode,
    },
    #[error("the login of account {account} belongs to another ChatGPT account")]
    AccountIdMismatch { account: AccountName },
    #[error("no credential is available for account {account}")]
    MissingCredential { account: AccountName },
}

#[derive(Debug, Error)]
pub enum AccountEditError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Account(#[from] AccountError),
    #[error("gateway {gateway} failed")]
    OnGateway {
        gateway: GatewayName,
        source: GatewayError,
    },
    #[error("{}", unreachable_text(.0))]
    Unreachable(Vec<UnreachableGateway>),
    #[error(transparent)]
    Connect(ConnectError),
    #[error("{}; rolling back account {account} on gateway {gateway} failed", error_chain(.original.as_ref()))]
    RollbackFailed {
        account: AccountName,
        gateway: GatewayName,
        original: Box<AccountEditError>,
        #[source]
        rollback: AccountError,
    },
}

pub(crate) fn account_agent(account: &Account) -> Agent {
    if account.provider() == Provider::Codex {
        Agent::Codex
    } else if jackin_core::lookup_tuple(Agent::Omp, account.provider()).is_ok() {
        Agent::Omp
    } else {
        Agent::Codex
    }
}

fn login_accounts(config: &Config) -> BTreeSet<AccountName> {
    config
        .accounts()
        .filter(|account| account.has_provider())
        .map(|account| account.name().clone())
        .collect()
}

pub(crate) fn unreachable_text(gateways: &[UnreachableGateway]) -> String {
    gateways
        .iter()
        .map(|gateway| {
            format!(
                "{}; if it is gone for good, run `jackin gateway rm {} --offline`",
                error_chain(gateway),
                gateway.gateway()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[derive(Clone, Debug)]
pub struct AccountDesk {
    host: HostPorts,
    cache_dir: HostPath,
    store: ConfigStore,
}

#[derive(Debug)]
struct Locked {
    desk: AccountDesk,
    config: Config,
    _held: Vec<FileLock>,
}

fn account_lock(name: &AccountName) -> String {
    format!("account-{name}.lock")
}

impl AccountDesk {
    pub fn new(host: HostPorts, cache_dir: HostPath, store: ConfigStore) -> AccountDesk {
        AccountDesk {
            host,
            cache_dir,
            store,
        }
    }

    async fn lock(
        &self,
        names_of: impl Fn(&Config) -> BTreeSet<AccountName>,
        cancel: &CancellationToken,
    ) -> Result<Locked, AccountError> {
        loop {
            let names = names_of(&self.store.load().await?);
            let mut held = Vec::with_capacity(names.len());
            for name in &names {
                held.push(
                    lock::hold(&self.host, &self.cache_dir, &account_lock(name), cancel).await?,
                );
            }
            let config = self.store.load().await?;
            if names_of(&config).is_subset(&names) {
                return Ok(Locked {
                    desk: self.clone(),
                    config,
                    _held: held,
                });
            }
        }
    }
}

#[derive(Debug)]
pub enum AccountPlan {
    Offline(OfflinePlan),
    Gateway(GatewayPlan),
}

#[derive(Debug)]
pub struct OfflinePlan {
    locked: Locked,
    edits: Vec<ConfigEdit>,
}

#[derive(Debug)]
pub struct GatewayPlan {
    locked: Locked,
    edits: Vec<ConfigEdit>,
    effects: Effects,
}

#[derive(Debug)]
pub enum GatewayRemoval {
    Offline(OfflinePlan),
    Online(RemovalPlan),
}

#[derive(Debug)]
pub struct RemovalPlan {
    locked: Locked,
    name: GatewayName,
    entry: GatewayEntry,
    forget: Vec<Account>,
}

#[derive(Debug)]
struct Publish {
    account: Account,
    previous: Option<Account>,
}

#[derive(Debug)]
struct Effects {
    forget: Vec<Account>,
    publish: Vec<Publish>,
}

impl Effects {
    fn is_empty(&self) -> bool {
        self.forget.is_empty() && self.publish.is_empty()
    }

    fn needs_every_gateway(&self) -> bool {
        !self.forget.is_empty()
            || self
                .publish
                .iter()
                .any(|publish| publish.previous.is_some())
    }
}

fn touched(edit: &ConfigEdit) -> Option<&AccountName> {
    match edit {
        ConfigEdit::AddAccount(account) => Some(account.name()),
        ConfigEdit::RemoveAccount(name)
        | ConfigEdit::RemoveAccountClearingBindings(name)
        | ConfigEdit::SetAccountCredential { account: name, .. } => Some(name),
        _ => None,
    }
}

fn effects(config: &Config, edits: &[ConfigEdit]) -> Result<Effects, ConfigError> {
    let mut after = config.clone();
    for edit in edits {
        after.apply(edit.clone())?;
    }
    let names: BTreeSet<&AccountName> = edits.iter().filter_map(touched).collect();
    let mut forget = Vec::new();
    let mut publish = Vec::new();
    for name in names {
        let before = config
            .account(name)
            .filter(|account| account.has_provider());
        let now = after.account(name).filter(|account| account.has_provider());
        match (before, now) {
            (before, Some(account)) => publish.push(Publish {
                account: account.clone(),
                previous: before.cloned(),
            }),
            (Some(account), None) => forget.push(account.clone()),
            (None, None) => {}
        }
    }
    Ok(Effects { forget, publish })
}

pub async fn plan_account_edits(
    desk: &AccountDesk,
    edits: Vec<ConfigEdit>,
    cancel: &CancellationToken,
) -> Result<AccountPlan, AccountEditError> {
    let names: BTreeSet<AccountName> = edits.iter().filter_map(touched).cloned().collect();
    let locked = desk.lock(|_| names.clone(), cancel).await?;
    let effects = effects(&locked.config, &edits)?;
    Ok(if effects.is_empty() {
        AccountPlan::Offline(OfflinePlan { locked, edits })
    } else {
        AccountPlan::Gateway(GatewayPlan {
            locked,
            edits,
            effects,
        })
    })
}

pub async fn plan_gateway_removal(
    desk: &AccountDesk,
    name: &GatewayName,
    cancel: &CancellationToken,
) -> Result<GatewayRemoval, AccountEditError> {
    let locked = desk.lock(login_accounts, cancel).await?;
    let edit = ConfigEdit::RemoveGateway(name.clone());
    locked.config.clone().apply(edit.clone())?;
    let forget: Vec<Account> = locked
        .config
        .accounts()
        .filter(|account| account.has_provider())
        .cloned()
        .collect();
    let entry = locked.config.gateway(name).cloned();
    Ok(match entry {
        Some(entry) if !forget.is_empty() => GatewayRemoval::Online(RemovalPlan {
            locked,
            name: name.clone(),
            entry,
            forget,
        }),
        _ => GatewayRemoval::Offline(OfflinePlan {
            locked,
            edits: vec![edit],
        }),
    })
}

impl AccountPlan {
    pub fn needs_gateway(&self) -> bool {
        matches!(self, AccountPlan::Gateway(_))
    }

    pub async fn apply(self, gateways: &Gateways) -> Result<Config, AccountEditError> {
        match self {
            AccountPlan::Offline(plan) => Ok(plan.apply().await?),
            AccountPlan::Gateway(plan) => plan.apply(gateways).await,
        }
    }
}

impl OfflinePlan {
    pub async fn apply(self) -> Result<Config, ConfigError> {
        self.locked.desk.store.edit(self.edits).await
    }
}

impl RemovalPlan {
    pub fn gateway(&self) -> &GatewayName {
        &self.name
    }

    pub async fn apply(
        self,
        connector: &dyn GatewayConnector,
        cancel: &CancellationToken,
    ) -> Result<Config, AccountEditError> {
        let gateway = match connector.connect(&self.name, &self.entry, cancel).await {
            Ok(gateway) => gateway,
            Err(error) if error.is_cancelled() => return Err(AccountEditError::Connect(error)),
            Err(error) => {
                return Err(AccountEditError::Unreachable(vec![
                    UnreachableGateway::new(self.name, error),
                ]));
            }
        };
        let host = &self.locked.desk.host;
        let edit = ConfigEdit::RemoveGateway(self.name.clone());
        let mut journal = Vec::new();
        let written = async {
            for account in &self.forget {
                let held = holds(gateway.as_ref(), account)
                    .await
                    .map_err(|error| on_gateway(&self.name, error))?;
                if held {
                    journal.push(Undo {
                        host,
                        gateway: gateway.as_ref(),
                        account,
                        action: Action::Restore,
                    });
                }
                forget_login(gateway.as_ref(), account)
                    .await
                    .map_err(|error| on_gateway(&self.name, error))?;
            }
            Ok(self.locked.desk.store.edit(vec![edit]).await?)
        }
        .await;
        match written {
            Ok(config) => Ok(config),
            Err(original) => Err(roll_back(original, journal).await),
        }
    }
}

#[derive(Debug)]
enum Action {
    Remove,
    Restore,
}

#[derive(Debug)]
struct Undo<'a> {
    host: &'a HostPorts,
    gateway: &'a dyn Gateway,
    account: &'a Account,
    action: Action,
}

impl Undo<'_> {
    async fn run(&self) -> Result<(), AccountError> {
        match self.action {
            Action::Remove => forget_login(self.gateway, self.account).await,
            Action::Restore => sync_login(
                self.host,
                self.gateway,
                self.account,
                &CancellationToken::new(),
            )
            .await
            .map(drop),
        }
    }
}

fn committed(error: &AccountEditError) -> bool {
    matches!(error, AccountEditError::Config(config) if config.committed())
}

async fn roll_back(original: AccountEditError, journal: Vec<Undo<'_>>) -> AccountEditError {
    if committed(&original) {
        return original;
    }
    let mut failure = None;
    for undo in journal.iter().rev() {
        if let Err(rollback) = undo.run().await
            && failure.is_none()
        {
            failure = Some((undo, rollback));
        }
    }
    match failure {
        None => original,
        Some((undo, rollback)) => AccountEditError::RollbackFailed {
            account: undo.account.name().clone(),
            gateway: undo.gateway.name().clone(),
            original: Box::new(original),
            rollback,
        },
    }
}

fn on_gateway(gateway: &GatewayName, error: AccountError) -> AccountEditError {
    match error {
        AccountError::Gateway(source) => AccountEditError::OnGateway {
            gateway: gateway.clone(),
            source,
        },
        other => AccountEditError::Account(other),
    }
}

fn check_gateways(config: &Config, gateways: &Gateways) -> Result<(), AccountError> {
    let connected = gateways.names();
    if let Some(gone) = connected.iter().find(|name| config.gateway(name).is_none()) {
        return Err(AccountError::GatewayRemoved {
            gateway: (*gone).clone(),
        });
    }
    match config
        .gateways()
        .find(|(name, _)| !connected.contains(name))
    {
        Some((added, _)) => Err(AccountError::GatewayAdded {
            gateway: added.clone(),
        }),
        None => Ok(()),
    }
}

impl GatewayPlan {
    pub fn config(&self) -> &Config {
        &self.locked.config
    }

    pub async fn apply(self, gateways: &Gateways) -> Result<Config, AccountEditError> {
        let GatewayPlan {
            locked,
            edits,
            effects,
        } = self;
        check_gateways(&locked.config, gateways)?;
        let store = &locked.desk.store;
        let home = gateways.home();
        let sessions = if effects.needs_every_gateway() {
            gateways.every().map_err(AccountEditError::Unreachable)?
        } else {
            vec![home]
        };
        let mut journal = Vec::new();
        let written = async {
            forget(&sessions, &effects.forget, &mut journal).await?;
            publish(&sessions, &effects.publish, &mut journal).await?;
            store.edit(edits).await.map_err(AccountEditError::from)
        }
        .await;
        match written {
            Ok(config) => Ok(config),
            Err(original) => Err(roll_back(original, journal).await),
        }
    }
}

async fn forget<'a>(
    sessions: &[&'a Session],
    accounts: &'a [Account],
    journal: &mut Vec<Undo<'a>>,
) -> Result<(), AccountEditError> {
    for account in accounts {
        for &session in sessions {
            let held = holds(session.deps.gateway.as_ref(), account)
                .await
                .map_err(|error| on_gateway(session.gateway_name(), error))?;
            if held {
                journal.push(session.undo(Action::Restore, account));
            }
            forget_login(session.deps.gateway.as_ref(), account)
                .await
                .map_err(|error| on_gateway(session.gateway_name(), error))?;
        }
    }
    Ok(())
}

async fn publish<'a>(
    sessions: &[&'a Session],
    publish: &'a [Publish],
    journal: &mut Vec<Undo<'a>>,
) -> Result<(), AccountEditError> {
    let Some((home, _)) = sessions.split_first() else {
        return Ok(());
    };
    for entry in publish {
        let Some(previous) = &entry.previous else {
            journal.push(home.undo(Action::Remove, &entry.account));
            home.sync_locked(&entry.account)
                .await
                .map_err(|error| on_gateway(home.gateway_name(), error))?;
            continue;
        };
        let provider =
            ProviderName::for_account(entry.account.name(), account_agent(&entry.account));
        for (index, &session) in sessions.iter().enumerate() {
            let state = session
                .deps
                .gateway
                .provider_state(&provider)
                .await
                .map_err(|error| on_gateway(session.gateway_name(), error.into()))?;
            let held = state == ProviderState::Present;
            if !held && index > 0 {
                continue;
            }
            journal.push(if held {
                session.undo(Action::Restore, previous)
            } else {
                session.undo(Action::Remove, &entry.account)
            });
            session
                .sync_locked(&entry.account)
                .await
                .map_err(|error| on_gateway(session.gateway_name(), error))?;
        }
    }
    Ok(())
}

impl Session {
    fn undo<'a>(&'a self, action: Action, account: &'a Account) -> Undo<'a> {
        Undo {
            host: &self.deps.host,
            gateway: self.deps.gateway.as_ref(),
            account,
            action,
        }
    }

    pub async fn sync_account(
        &self,
        account: &Account,
        cancel: &CancellationToken,
    ) -> Result<AccountSync, AccountError> {
        let _held = self.hold_current(account, cancel).await?;
        self.sync_locked_c(account, cancel).await
    }

    async fn forget_locked(&self, account: &Account) -> Result<(), AccountError> {
        forget_login(self.deps.gateway.as_ref(), account).await
    }

    pub(crate) async fn sync_if_absent(
        &self,
        account: &Account,
        provider: &ProviderName,
        cancel: &CancellationToken,
    ) -> Result<bool, AccountError> {
        let _held = self.hold_current(account, cancel).await?;
        if self.deps.gateway.provider_state(provider).await? == ProviderState::Present {
            return Ok(false);
        }
        if let Err(original) = self.sync_locked_c(account, cancel).await {
            return Err(match self.forget_locked(account).await {
                Ok(()) => original,
                Err(rollback) => AccountError::RollbackFailed {
                    account: account.name().clone(),
                    original: Box::new(original),
                    rollback: Box::new(rollback),
                },
            });
        }
        Ok(true)
    }

    async fn hold_current(
        &self,
        account: &Account,
        cancel: &CancellationToken,
    ) -> Result<FileLock, AccountError> {
        let held = self.hold_account(account.name(), cancel).await?;
        let config = self.deps.config.load().await?;
        if config.gateway(self.gateway_name()).is_none() {
            return Err(AccountError::GatewayRemoved {
                gateway: self.gateway_name().clone(),
            });
        }
        match config.account(account.name()) {
            None => Err(AccountError::Removed {
                account: account.name().clone(),
            }),
            Some(current) if current != account => Err(AccountError::Changed {
                account: account.name().clone(),
            }),
            Some(_) => Ok(held),
        }
    }

    async fn hold_account(
        &self,
        name: &AccountName,
        cancel: &CancellationToken,
    ) -> Result<FileLock, AccountError> {
        Ok(lock::hold(
            &self.deps.host,
            &self.deps.cache_dir,
            &account_lock(name),
            cancel,
        )
        .await?)
    }

    async fn sync_locked(&self, account: &Account) -> Result<AccountSync, AccountError> {
        self.sync_locked_c(account, &CancellationToken::new()).await
    }

    async fn sync_locked_c(
        &self,
        account: &Account,
        cancel: &CancellationToken,
    ) -> Result<AccountSync, AccountError> {
        sync_login(&self.deps.host, self.deps.gateway.as_ref(), account, cancel).await
    }
}

async fn forget_login(gateway: &dyn Gateway, account: &Account) -> Result<(), AccountError> {
    if !account.has_provider() {
        return Ok(());
    }
    let provider = ProviderName::for_account(account.name(), account_agent(account));
    gateway.delete_provider(&provider).await?;
    Ok(())
}

async fn holds(gateway: &dyn Gateway, account: &Account) -> Result<bool, AccountError> {
    let provider = ProviderName::for_account(account.name(), account_agent(account));
    Ok(gateway.provider_state(&provider).await? == ProviderState::Present)
}

async fn sync_login(
    host: &HostPorts,
    gateway: &dyn Gateway,
    account: &Account,
    cancel: &CancellationToken,
) -> Result<AccountSync, AccountError> {
    if account.provider() != Provider::Codex {
        let Ok(tuple) = jackin_core::lookup_tuple(Agent::Omp, account.provider()) else {
            return Ok(AccountSync::HostOnly);
        };
        if tuple.auth_class != jackin_core::AuthClass::StaticKey {
            return Ok(AccountSync::HostOnly);
        }
        let provider_id = tuple.omp_provider_id().unwrap_or_default();
        let key = resolve_static_key(host, account, provider_id, cancel)
            .await
            .map_err(|_| AccountError::MissingCredential {
                account: account.name().clone(),
            })?;
        let asset = jackin_core::tuple_profile_asset(tuple);
        let profile = ProviderProfile::parse(asset)?;
        let synced = gateway.ensure_profile(&profile).await?;
        let name = ProviderName::for_account(account.name(), Agent::Omp);
        gateway
            .put_provider(&ProviderSpec {
                name: name.clone(),
                profile: profile.id,
                keys: jackin_core::CredentialKeys {
                    access_token: tuple.omp_credential_env().ok_or(
                        AccountError::MissingCredential {
                            account: account.name().clone(),
                        },
                    )?,
                    refresh_token: "",
                    account_id: "",
                    id_token: "",
                    token_url: "",
                },
                material: ProviderMaterial::StaticKey { key },
            })
            .await?;
        return Ok(AccountSync::ProviderReady {
            provider: name,
            profile: synced,
        });
    }
    let spec = Agent::Codex.spec();
    let Some(stored) = account.chatgpt_login() else {
        return Err(AccountError::Unsupported {
            account: account.name().clone(),
            mode: CodexAuthMode::ApiKey,
        });
    };
    let login = read_chatgpt_login(host.fs.as_ref(), stored.home).await?;
    if login.account_id != *stored.account_id {
        return Err(AccountError::AccountIdMismatch {
            account: account.name().clone(),
        });
    }
    let profile = ProviderProfile::parse(&spec.codex.openshell.chatgpt_profile)?;
    let synced = gateway.ensure_profile(&profile).await?;
    let name = ProviderName::for_account(account.name(), Agent::Codex);
    gateway
        .put_provider(&ProviderSpec {
            name: name.clone(),
            profile: profile.id,
            keys: spec.codex.openshell.keys,
            material: ProviderMaterial::ChatgptLogin {
                access_token: login.access_token,
                refresh_token: login.refresh_token,
                client_id: login.client_id,
                chatgpt_account_id: login.account_id,
            },
        })
        .await?;
    Ok(AccountSync::ProviderReady {
        provider: name,
        profile: synced,
    })
}
