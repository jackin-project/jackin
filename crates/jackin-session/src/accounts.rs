// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, CodexAuthMode, CredentialSource, Provider, ProviderName,
};
use jackin_openshell::{
    GatewayError, PolicyError, ProfileSync, ProviderMaterial, ProviderProfile, ProviderSpec,
};
use thiserror::Error;

use crate::credentials::{CredentialError, read_chatgpt_login};
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
    #[error("account {account:?} uses {mode:?}, which is not supported yet")]
    Unsupported {
        account: AccountName,
        mode: CodexAuthMode,
    },
    #[error("the login of account {account:?} belongs to another ChatGPT account")]
    AccountIdMismatch { account: AccountName },
}

impl Session {
    pub async fn sync_account(&self, account: &Account) -> Result<AccountSync, AccountError> {
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
