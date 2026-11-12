// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Account, AccountName, CredentialSource, Provider};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageAccount {
    pub name: AccountName,
    pub provider: Provider,
    pub credential: CredentialSource,
}

impl UsageAccount {
    pub fn from_account(account: &Account) -> UsageAccount {
        UsageAccount {
            name: account.name().clone(),
            provider: account.provider(),
            credential: account.credential().clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    Cached,
    Force,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageRequest {
    pub accounts: Vec<UsageAccount>,
    pub refresh: Refresh,
}
