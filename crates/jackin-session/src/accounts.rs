// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, CodexAuthMode, CredentialSource, FileLock, Provider, ProviderName,
};
use jackin_openshell::{
    GatewayError, PolicyError, ProfileSync, ProviderMaterial, ProviderProfile, ProviderSpec,
    ProviderState,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::credentials::{CredentialError, read_chatgpt_login};
use crate::lock::{self, LockError};
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
    #[error("account {account:?} uses {mode:?}, which is not supported yet")]
    Unsupported {
        account: AccountName,
        mode: CodexAuthMode,
    },
    #[error("the login of account {account:?} belongs to another ChatGPT account")]
    AccountIdMismatch { account: AccountName },
}

impl Session {
    pub async fn sync_account(
        &self,
        account: &Account,
        cancel: &CancellationToken,
    ) -> Result<AccountSync, AccountError> {
        let _held = self.hold_account(account, cancel).await?;
        self.sync_locked(account).await
    }

    pub async fn forget_account(
        &self,
        account: &Account,
        cancel: &CancellationToken,
    ) -> Result<(), AccountError> {
        if !matches!(account.credential(), CredentialSource::CodexLogin { .. }) {
            return Ok(());
        }
        let _held = self.hold_account(account, cancel).await?;
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
        let _held = self.hold_account(account, cancel).await?;
        if self.deps.gateway.provider_state(provider).await? == ProviderState::Present {
            return Ok(false);
        }
        self.sync_locked(account).await?;
        Ok(true)
    }

    async fn hold_account(
        &self,
        account: &Account,
        cancel: &CancellationToken,
    ) -> Result<FileLock, AccountError> {
        Ok(lock::hold(
            &self.deps,
            &format!("account-{}.lock", account.name()),
            cancel,
        )
        .await?)
    }

    async fn sync_locked(&self, account: &Account) -> Result<AccountSync, AccountError> {
        if account.provider() != Provider::Codex {
            return Ok(AccountSync::HostOnly);
        }
        let spec = Agent::Codex.spec();
        let CredentialSource::CodexLogin {
            home,
            chatgpt_account_id,
        } = account.credential()
        else {
            return Err(AccountError::Unsupported {
                account: account.name().clone(),
                mode: CodexAuthMode::ApiKey,
            });
        };
        let login = read_chatgpt_login(self.deps.host.fs.as_ref(), home).await?;
        if login.account_id != *chatgpt_account_id {
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
