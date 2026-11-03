// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_core::{
    Account, AccountName, Agent, CodexAuthMode, Config, ConfigEdit, ConfigError, ConfigStore,
    FileLock, GatewayName, Provider, ProviderName,
};
use jackin_openshell::{
    GatewayError, PolicyError, ProfileSync, ProviderMaterial, ProviderProfile, ProviderSpec,
    ProviderState,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::credentials::{CredentialError, read_chatgpt_login};
use crate::gateways::{Gateways, UnreachableGateway};
use crate::lock::{self, LockError};
use crate::report::error_chain;
use crate::service::Session;

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
    #[error("gateway {gateway} was removed after the launch was planned")]
    GatewayRemoved { gateway: GatewayName },
    #[error("account {account} uses {mode}, which is not supported yet")]
    Unsupported {
        account: AccountName,
        mode: CodexAuthMode,
    },
    #[error("the login of account {account} belongs to another ChatGPT account")]
    AccountIdMismatch { account: AccountName },
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
    #[error("{original}; the provider of account {account} stays on gateway {gateway}")]
    RollbackFailed {
        account: AccountName,
        gateway: GatewayName,
        original: Box<AccountEditError>,
        rollback: AccountError,
    },
}

pub fn gateway_holds_providers(config: &Config) -> bool {
    !login_accounts(config).is_empty()
}

fn login_accounts(config: &Config) -> BTreeSet<AccountName> {
    config
        .accounts()
        .filter(|account| account.has_provider())
        .map(|account| account.name().clone())
        .collect()
}

fn unreachable_text(gateways: &[UnreachableGateway]) -> String {
    gateways
        .iter()
        .map(|gateway| error_chain(gateway))
        .collect::<Vec<_>>()
        .join("; ")
}

#[derive(Clone, Debug)]
pub enum AccountPlan {
    Offline(OfflinePlan),
    Gateway(GatewayPlan),
}

#[derive(Clone, Debug)]
pub struct OfflinePlan {
    edits: Vec<ConfigEdit>,
}

#[derive(Clone, Debug)]
pub struct GatewayPlan {
    edits: Vec<ConfigEdit>,
}

#[derive(Debug)]
struct Publish {
    account: Account,
    renew: bool,
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
        !self.forget.is_empty() || self.publish.iter().any(|publish| publish.renew)
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
                renew: before.is_some(),
            }),
            (Some(account), None) => forget.push(account.clone()),
            (None, None) => {}
        }
    }
    Ok(Effects { forget, publish })
}

pub fn plan_account_edits(
    config: &Config,
    edits: Vec<ConfigEdit>,
) -> Result<AccountPlan, ConfigError> {
    Ok(if effects(config, &edits)?.is_empty() {
        AccountPlan::Offline(OfflinePlan { edits })
    } else {
        AccountPlan::Gateway(GatewayPlan { edits })
    })
}

impl AccountPlan {
    pub fn needs_gateway(&self) -> bool {
        matches!(self, AccountPlan::Gateway(_))
    }

    pub async fn apply(
        self,
        gateways: &Gateways,
        store: &ConfigStore,
        cancel: &CancellationToken,
    ) -> Result<Config, AccountEditError> {
        match self {
            AccountPlan::Offline(plan) => Ok(plan.apply(store).await?),
            AccountPlan::Gateway(plan) => plan.apply(gateways, store, cancel).await,
        }
    }
}

impl OfflinePlan {
    pub async fn apply(self, store: &ConfigStore) -> Result<Config, ConfigError> {
        store.edit(self.edits).await
    }
}

fn on_gateway(session: &Session, error: AccountError) -> AccountEditError {
    match error {
        AccountError::Gateway(source) => AccountEditError::OnGateway {
            gateway: session.gateway_name().clone(),
            source,
        },
        other => AccountEditError::Account(other),
    }
}

impl GatewayPlan {
    pub async fn apply(
        self,
        gateways: &Gateways,
        store: &ConfigStore,
        cancel: &CancellationToken,
    ) -> Result<Config, AccountEditError> {
        let home = gateways.home();
        let names: BTreeSet<&AccountName> = self.edits.iter().filter_map(touched).collect();
        let _held = home.hold_accounts(names, cancel).await?;
        let effects = effects(&store.load().await?, &self.edits)?;
        if effects.is_empty() {
            return Ok(store.edit(self.edits).await?);
        }
        let sessions = if effects.needs_every_gateway() {
            gateways.every().map_err(AccountEditError::Unreachable)?
        } else {
            vec![home]
        };
        for account in &effects.forget {
            for session in &sessions {
                session
                    .forget_locked(account)
                    .await
                    .map_err(|error| on_gateway(session, error))?;
            }
        }
        let mut created = Vec::new();
        let written = match publish(&sessions, &effects.publish, &mut created).await {
            Ok(()) => store.edit(self.edits).await.map_err(AccountEditError::from),
            Err(error) => Err(error),
        };
        match written {
            Ok(config) => Ok(config),
            Err(original) => {
                for account in created {
                    if let Err(rollback) = home.forget_locked(account).await {
                        return Err(AccountEditError::RollbackFailed {
                            account: account.name().clone(),
                            gateway: home.gateway_name().clone(),
                            original: Box::new(original),
                            rollback,
                        });
                    }
                }
                Err(original)
            }
        }
    }
}

async fn publish<'a>(
    sessions: &[&Session],
    publish: &'a [Publish],
    created: &mut Vec<&'a Account>,
) -> Result<(), AccountEditError> {
    let Some((home, peers)) = sessions.split_first() else {
        return Ok(());
    };
    for entry in publish {
        if !entry.renew {
            created.push(&entry.account);
        }
        home.sync_locked(&entry.account)
            .await
            .map_err(|error| on_gateway(home, error))?;
        if !entry.renew {
            continue;
        }
        let provider = ProviderName::for_account(entry.account.name(), Agent::Codex);
        for peer in peers {
            let state = peer
                .deps
                .gateway
                .provider_state(&provider)
                .await
                .map_err(|error| on_gateway(peer, error.into()))?;
            if state == ProviderState::Present {
                peer.sync_locked(&entry.account)
                    .await
                    .map_err(|error| on_gateway(peer, error))?;
            }
        }
    }
    Ok(())
}

impl Session {
    pub async fn sync_account(
        &self,
        account: &Account,
        cancel: &CancellationToken,
    ) -> Result<AccountSync, AccountError> {
        let _held = self.hold_account(account.name(), cancel).await?;
        self.sync_locked(account).await
    }

    async fn forget_locked(&self, account: &Account) -> Result<(), AccountError> {
        if !account.has_provider() {
            return Ok(());
        }
        let provider = ProviderName::for_account(account.name(), Agent::Codex);
        self.deps.gateway.delete_provider(&provider).await?;
        Ok(())
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
        if let Err(original) = self.sync_locked(account).await {
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

    pub async fn remove_gateway(
        &self,
        store: &ConfigStore,
        cancel: &CancellationToken,
    ) -> Result<Config, AccountEditError> {
        let edit = ConfigEdit::RemoveGateway(self.gateway_name().clone());
        let named = login_accounts(&store.load().await?);
        let _held = self.hold_accounts(named.iter().collect(), cancel).await?;
        let config = store.load().await?;
        config.clone().apply(edit.clone())?;
        for account in config.accounts().filter(|account| account.has_provider()) {
            self.forget_locked(account)
                .await
                .map_err(|error| on_gateway(self, error))?;
        }
        Ok(store.edit(vec![edit]).await?)
    }

    async fn hold_accounts(
        &self,
        names: BTreeSet<&AccountName>,
        cancel: &CancellationToken,
    ) -> Result<Vec<FileLock>, AccountError> {
        let mut held = Vec::new();
        for name in names {
            held.push(self.hold_account(name, cancel).await?);
        }
        Ok(held)
    }

    async fn hold_account(
        &self,
        name: &AccountName,
        cancel: &CancellationToken,
    ) -> Result<FileLock, AccountError> {
        Ok(lock::hold(&self.deps, &format!("account-{name}.lock"), cancel).await?)
    }

    async fn sync_locked(&self, account: &Account) -> Result<AccountSync, AccountError> {
        if account.provider() != Provider::Codex {
            return Ok(AccountSync::HostOnly);
        }
        let spec = Agent::Codex.spec();
        let Some(stored) = account.chatgpt_login() else {
            return Err(AccountError::Unsupported {
                account: account.name().clone(),
                mode: CodexAuthMode::ApiKey,
            });
        };
        let login = read_chatgpt_login(self.deps.host.fs.as_ref(), stored.home).await?;
        if login.account_id != *stored.account_id {
            return Err(AccountError::AccountIdMismatch {
                account: account.name().clone(),
            });
        }
        let profile = ProviderProfile::parse(&spec.openshell.chatgpt_profile)?;
        let synced = self.deps.gateway.ensure_profile(&profile).await?;
        let name = ProviderName::for_account(account.name(), Agent::Codex);
        self.deps
            .gateway
            .put_provider(&ProviderSpec {
                name: name.clone(),
                profile: profile.id,
                keys: spec.openshell.keys,
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
}
