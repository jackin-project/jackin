// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use thiserror::Error;

use super::{Config, join_display};
use crate::agent::Agent;
use crate::ids::{AccountName, RoleName, WorkspaceName};
use crate::provider::Provider;

#[derive(Clone, Copy, Debug)]
pub struct AccountQuery<'a> {
    pub flag: Option<&'a AccountName>,
    pub workspace: &'a WorkspaceName,
    pub role: &'a RoleName,
    pub agent: Agent,
    pub provider: Provider,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountSelection {
    pub account: AccountName,
    pub via: SelectionRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SelectionRule {
    Flag,
    WorkspaceBinding,
    RoleBinding,
    ProviderDefault,
    OnlyAccount,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MissingBinding {
    NoAccountForProvider {
        provider: Provider,
    },
    SeveralAccountsNoDefault {
        provider: Provider,
        accounts: Vec<AccountName>,
    },
}

impl fmt::Display for MissingBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MissingBinding::NoAccountForProvider { provider } => write!(
                f,
                "no {} account exists; add one with jackin account add",
                provider.as_str()
            ),
            MissingBinding::SeveralAccountsNoDefault { provider, accounts } => write!(
                f,
                "several {} accounts exist ({}) and none is bound or the default; \
                 pass --account, or bind one with jackin account bind or jackin account default",
                provider.as_str(),
                join_display(accounts)
            ),
        }
    }
}

#[derive(Debug, Error)]
pub enum SelectionError {
    #[error("unknown account {0}")]
    UnknownFlagAccount(AccountName),
    #[error(
        "account {account} does not belong to the {} provider",
        .expected.as_str()
    )]
    FlagProviderMismatch {
        account: AccountName,
        expected: Provider,
    },
    #[error("no account selected: {missing}")]
    NoAccount { missing: MissingBinding },
}

impl Config {
    pub fn select_account(
        &self,
        query: &AccountQuery<'_>,
    ) -> Result<AccountSelection, SelectionError> {
        let provider = query.provider;
        if let Some(flag) = query.flag {
            return self.select_flag(flag, provider);
        }
        let workspace = self
            .workspace(query.workspace)
            .and_then(|workspace| workspace.bindings.get(&provider));
        let role = self
            .role(query.role)
            .and_then(|role| role.bindings.get(&provider));
        let ranked = [
            (workspace, SelectionRule::WorkspaceBinding),
            (role, SelectionRule::RoleBinding),
            (
                self.default_account(provider),
                SelectionRule::ProviderDefault,
            ),
        ];
        if let Some((account, via)) = ranked
            .into_iter()
            .find_map(|(account, via)| account.map(|account| (account, via)))
        {
            return Ok(AccountSelection {
                account: account.clone(),
                via,
            });
        }
        self.select_only(provider)
    }

    fn select_flag(
        &self,
        flag: &AccountName,
        provider: Provider,
    ) -> Result<AccountSelection, SelectionError> {
        let account = self
            .account(flag)
            .ok_or_else(|| SelectionError::UnknownFlagAccount(flag.clone()))?;
        if account.provider() != provider {
            return Err(SelectionError::FlagProviderMismatch {
                account: flag.clone(),
                expected: provider,
            });
        }
        Ok(AccountSelection {
            account: flag.clone(),
            via: SelectionRule::Flag,
        })
    }

    fn select_only(&self, provider: Provider) -> Result<AccountSelection, SelectionError> {
        let mut candidates: Vec<AccountName> = self
            .accounts()
            .filter(|account| account.provider() == provider)
            .map(|account| account.name().clone())
            .collect();
        match candidates.len() {
            0 => Err(SelectionError::NoAccount {
                missing: MissingBinding::NoAccountForProvider { provider },
            }),
            1 => Ok(AccountSelection {
                account: candidates.remove(0),
                via: SelectionRule::OnlyAccount,
            }),
            _ => Err(SelectionError::NoAccount {
                missing: MissingBinding::SeveralAccountsNoDefault {
                    provider,
                    accounts: candidates,
                },
            }),
        }
    }
}
