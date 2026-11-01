// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

use crate::agent::Agent;
use crate::ids::{AccountName, RoleName, WorkspaceName};
use crate::provider::Provider;

#[derive(Clone, Copy, Debug)]
pub struct AccountQuery<'a> {
    pub flag: Option<&'a AccountName>,
    pub workspace: &'a WorkspaceName,
    pub role: &'a RoleName,
    pub agent: Agent,
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

#[derive(Debug, Error)]
pub enum SelectionError {
    #[error("unknown account {0:?}")]
    UnknownFlagAccount(AccountName),
    #[error("account {account:?} does not belong to provider {expected:?}")]
    FlagProviderMismatch {
        account: AccountName,
        expected: Provider,
    },
    #[error("no account selected: {missing:?}")]
    NoAccount { missing: MissingBinding },
}
